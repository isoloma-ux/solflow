//! Свёрстанный PDF со встроенным Inter — тот же вид, что у экспорта на
//! Android: заголовок, серая длительность, подписи говорящих, серые метки
//! времени. Системный `cupsfilter` умеет только Courier без вёрстки,
//! поэтому PDF собирается здесь.
//!
//! Шрифт встраивается как CIDFontType2 с Identity-H: тогда в поток
//! контента идут номера глифов, и кириллица не зависит от кодировок
//! читалки. ToUnicode добавляется, чтобы из готового файла копировался
//! текст, а не мусор.

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::ttf::Font;

const PAGE_W: f32 = 595.0; // A4 в пунктах
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 56.0;
const LINE_SPACING: f32 = 1.35;

/// Начертание строки: обычное или среднее (полужирным Inter не пользуемся).
#[derive(Clone, Copy, PartialEq)]
pub enum Face {
    Regular,
    Medium,
}

/// Кусок текста с его видом — из таких строится документ.
pub struct Block {
    pub text: String,
    pub face: Face,
    pub size: f32,
    pub gray: bool,
    /// Отступ сверху перед блоком, в пунктах.
    pub gap: f32,
    /// Отступ слева — им сдвигается текст реплики под метку времени.
    pub indent: f32,
}

impl Block {
    pub fn new(text: impl Into<String>, face: Face, size: f32) -> Self {
        Self {
            text: text.into(),
            face,
            size,
            gray: false,
            gap: 0.0,
            indent: 0.0,
        }
    }

    pub fn gray(mut self) -> Self {
        self.gray = true;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }
}

/// Строка, уже разложенная по странице: текст, координаты, вид.
struct Placed {
    text: String,
    x: f32,
    y: f32,
    face: Face,
    size: f32,
    gray: bool,
}

pub struct Document<'a> {
    regular: &'a Font,
    medium: &'a Font,
    pages: Vec<Vec<Placed>>,
    y: f32,
}

impl<'a> Document<'a> {
    pub fn new(regular: &'a Font, medium: &'a Font) -> Self {
        Self {
            regular,
            medium,
            pages: vec![Vec::new()],
            y: MARGIN,
        }
    }

    fn font(&self, face: Face) -> &'a Font {
        match face {
            Face::Regular => self.regular,
            Face::Medium => self.medium,
        }
    }

    /// Кладёт блок, перенося слова по ширине и заводя новые страницы.
    /// Возвращает страницу и базовую линию первой строки — по ним ставится
    /// метка времени рядом с репликой.
    pub fn add(&mut self, block: &Block) -> Option<(usize, f32)> {
        let font = self.font(block.face);
        let width = PAGE_W - 2.0 * MARGIN - block.indent;
        let line_height = block.size * LINE_SPACING;
        self.y += block.gap;

        let mut first = None;
        for line in wrap(font, &block.text, block.size, width) {
            if self.y + line_height > PAGE_H - MARGIN {
                self.pages.push(Vec::new());
                self.y = MARGIN;
            }
            // В PDF начало координат внизу, а базовая линия ниже верха
            // строки на подъём шрифта.
            let baseline = PAGE_H - self.y - block.size * font.ascent as f32
                / font.units_per_em;
            first.get_or_insert((self.pages.len() - 1, baseline));
            self.pages.last_mut().unwrap().push(Placed {
                text: line,
                x: MARGIN + block.indent,
                y: baseline,
                face: block.face,
                size: block.size,
                gray: block.gray,
            });
            self.y += line_height;
        }
        first
    }

    /// Метка времени слева в своей колонке, текст реплики — с отступом под
    /// неё, как в окне приложения. Метка встаёт на базовую линию первой
    /// строки текста, даже если реплика переехала на новую страницу.
    pub fn add_row(&mut self, clock: &str, text: &str, size: f32, indent: f32) {
        let placed = self.add(&Block::new(text, Face::Regular, size).indent(indent).gap(6.0));
        if let Some((page, baseline)) = placed {
            self.pages[page].push(Placed {
                text: clock.to_string(),
                x: MARGIN,
                y: baseline,
                face: Face::Regular,
                size: size - 2.0,
                gray: true,
            });
        }
    }

    /// Начать новую страницу: в склееном экспорте каждая встреча идёт со
    /// своей, как подшивка. На пустой странице ничего не делает.
    pub fn page_break(&mut self) {
        if self.pages.last().map(|p| p.is_empty()) == Some(true) {
            return;
        }
        self.pages.push(Vec::new());
        self.y = MARGIN;
    }

    pub fn finish(self) -> Vec<u8> {
        build(self.regular, self.medium, &self.pages)
    }
}

/// Перенос по словам; слово длиннее строки рвётся по символам.
fn wrap(font: &Font, text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if font.text_width(&candidate, size) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
            // Одно слово шире строки — режем посимвольно.
            while font.text_width(&line, size) > width && line.chars().count() > 1 {
                let mut cut = String::new();
                for ch in line.chars() {
                    if font.text_width(&format!("{cut}{ch}"), size) > width && !cut.is_empty() {
                        break;
                    }
                    cut.push(ch);
                }
                let rest = line[cut.len()..].to_string();
                lines.push(cut);
                line = rest;
            }
        }
        lines.push(line);
    }
    lines
}

// --- сборка файла ----------------------------------------------------------

fn compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_else(|_| data.to_vec())
}

/// Экранирование для текстовых строк PDF (заголовок документа и подобное).
fn escape(text: &str) -> String {
    text.replace('\\', r"\\").replace('(', r"\(").replace(')', r"\)")
}

fn build(regular: &Font, medium: &Font, pages: &[Vec<Placed>]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    let object = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| -> usize {
        offsets.push(out.len());
        let id = offsets.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
        id
    };

    // Шрифты: файл, дескриптор, CID-шрифт, Type0 и карта в Unicode.
    let mut font_ids = Vec::new();
    for (font, name) in [(regular, "Inter"), (medium, "InterMedium")] {
        let raw = font.bytes();
        let packed = compress(raw);
        offsets.push(out.len());
        let file_id = offsets.len();
        out.extend_from_slice(format!("{file_id} 0 obj\n").as_bytes());
        out.extend_from_slice(
            format!(
                "<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n",
                packed.len(),
                raw.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&packed);
        out.extend_from_slice(b"\nendstream\nendobj\n");

        let scale = 1000.0 / font.units_per_em;
        let descriptor = format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 32 \
             /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} \
             /CapHeight {} /StemV 80 /FontFile2 {file_id} 0 R >>",
            (font.bbox[0] as f32 * scale) as i32,
            (font.bbox[1] as f32 * scale) as i32,
            (font.bbox[2] as f32 * scale) as i32,
            (font.bbox[3] as f32 * scale) as i32,
            (font.ascent as f32 * scale) as i32,
            (font.descent as f32 * scale) as i32,
            (font.cap_height as f32 * scale) as i32,
        );
        let descriptor_id = object(&mut out, &mut offsets, descriptor.as_bytes());

        // Ширины только для тех глифов, что реально встречаются в тексте,
        // здесь не выделить — пишем весь диапазон одним пробегом.
        let mut widths = String::from("[");
        let mut cid = 0u16;
        while cid < font.num_glyphs {
            let mut run = Vec::new();
            let start = cid;
            while cid < font.num_glyphs && run.len() < 512 {
                run.push(format!("{}", font.advance(cid) as i32));
                cid += 1;
            }
            widths.push_str(&format!("{start} [{}] ", run.join(" ")));
        }
        widths.push(']');

        let cid_font = format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} \
             /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
             /FontDescriptor {descriptor_id} 0 R /DW 500 /W {widths} \
             /CIDToGIDMap /Identity >>"
        );
        let cid_id = object(&mut out, &mut offsets, cid_font.as_bytes());

        let to_unicode = to_unicode_cmap(font);
        let packed_cmap = compress(to_unicode.as_bytes());
        offsets.push(out.len());
        let cmap_id = offsets.len();
        out.extend_from_slice(format!("{cmap_id} 0 obj\n").as_bytes());
        out.extend_from_slice(
            format!(
                "<< /Length {} /Filter /FlateDecode >>\nstream\n",
                packed_cmap.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&packed_cmap);
        out.extend_from_slice(b"\nendstream\nendobj\n");

        let type0 = format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H \
             /DescendantFonts [{cid_id} 0 R] /ToUnicode {cmap_id} 0 R >>"
        );
        font_ids.push(object(&mut out, &mut offsets, type0.as_bytes()));
    }

    // Содержимое страниц.
    let mut content_ids = Vec::new();
    for page in pages {
        let mut content = String::new();
        for line in page {
            let font = if line.face == Face::Regular { regular } else { medium };
            let resource = if line.face == Face::Regular { "F1" } else { "F2" };
            let hex: String = line
                .text
                .chars()
                .map(|c| format!("{:04X}", font.glyph(c)))
                .collect();
            let color = if line.gray { "0.54 0.54 0.54" } else { "0.07 0.07 0.09" };
            content.push_str(&format!(
                "BT /{resource} {size} Tf {color} rg 1 0 0 1 {x:.2} {y:.2} Tm <{hex}> Tj ET\n",
                size = line.size,
                x = line.x,
                y = line.y,
            ));
        }
        let packed = compress(content.as_bytes());
        offsets.push(out.len());
        let id = offsets.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(
            format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).as_bytes(),
        );
        out.extend_from_slice(&packed);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        content_ids.push(id);
    }

    // Дерево страниц: id родителя знаем заранее — он идёт сразу за ними.
    let pages_id = offsets.len() + content_ids.len() + 1;
    let mut page_ids = Vec::new();
    for content_id in &content_ids {
        let page = format!(
            "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
             /Resources << /Font << /F1 {} 0 R /F2 {} 0 R >> >> /Contents {content_id} 0 R >>",
            font_ids[0], font_ids[1]
        );
        page_ids.push(object(&mut out, &mut offsets, page.as_bytes()));
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    let pages_obj = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        page_ids.len(),
        kids.join(" ")
    );
    let real_pages_id = object(&mut out, &mut offsets, pages_obj.as_bytes());
    debug_assert_eq!(real_pages_id, pages_id);

    let catalog_id = object(
        &mut out,
        &mut offsets,
        format!("<< /Type /Catalog /Pages {real_pages_id} 0 R >>").as_bytes(),
    );
    let info_id = object(
        &mut out,
        &mut offsets,
        format!("<< /Producer ({}) >>", escape("Sol Flow")).as_bytes(),
    );

    // Таблица ссылок и трейлер.
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {catalog_id} 0 R /Info {info_id} 0 R >>\n\
             startxref\n{xref_at}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Карта «номер глифа → символ»: без неё текст из PDF копируется мусором.
fn to_unicode_cmap(font: &Font) -> String {
    let mut pairs: Vec<(u16, u32)> = Vec::new();
    // Латиница, кириллица, пунктуация и типографские кавычки — всё, что
    // может встретиться в расшифровке.
    let ranges = [
        (0x20u32, 0x7Eu32),
        (0xA0, 0xFF),
        (0x400, 0x4FF),
        (0x2010, 0x2027),
        (0x20AC, 0x20BF),
    ];
    for (from, to) in ranges {
        for code in from..=to {
            if let Some(ch) = char::from_u32(code) {
                let glyph = font.glyph(ch);
                if glyph != 0 {
                    pairs.push((glyph, code));
                }
            }
        }
    }

    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in pairs.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, code) in chunk {
            out.push_str(&format!("<{glyph:04X}> <{code:04X}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}
