//! Свой генератор .docx. Раньше его собирал textutil из промежуточного
//! HTML — но это утилита macOS, а на Windows ничего похожего нет. Формат
//! несложный: zip с тремя XML внутри, а deflate у нас уже есть — им
//! сжимаются потоки в генераторе PDF.
//!
//! Вид повторяет PDF и окно приложения: название, длительность серым,
//! имена говорящих и реплики с метками времени.

use std::collections::HashMap;
use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::meetings::Segment;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

/// Собирает готовый файл .docx.
pub fn build(
    title: &str,
    duration: &str,
    segments: &[Segment],
    names: &HashMap<String, String>,
    speaker_at: &dyn Fn(usize) -> Option<String>,
    clock: &dyn Fn(f32) -> String,
) -> Vec<u8> {
    let _ = names;
    let mut body = String::new();

    // Размеры шрифта в OOXML — в половинах пункта: 18pt это 36.
    body.push_str(&paragraph(title, 36, true, None, 0));
    body.push_str(&paragraph(duration, 20, false, Some("8A8A8A"), 0));

    for (i, s) in segments.iter().enumerate() {
        if let Some(name) = speaker_at(i) {
            body.push_str(&paragraph(&name, 24, true, None, 320));
        }
        // Метка времени и текст — одним абзацем, как в HTML-экспорте:
        // время серым и мельче, дальше сама реплика.
        body.push_str(&format!(
            r#"<w:p><w:r><w:rPr><w:sz w:val="18"/><w:color w:val="8A8A8A"/></w:rPr><w:t xml:space="preserve">{}  </w:t></w:r><w:r><w:rPr><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            esc(&clock(s.s)),
            esc(&s.text)
        ));
    }

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
    );

    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("word/document.xml", document.as_bytes()),
    ])
}

/// Абзац: размер в половинах пункта, жирность, цвет и отступ сверху в
/// двадцатых долях пункта (так их считает Word).
fn paragraph(text: &str, half_points: u32, bold: bool, color: Option<&str>, before: u32) -> String {
    let mut props = format!("<w:sz w:val=\"{half_points}\"/>");
    if bold {
        props.push_str("<w:b/>");
    }
    if let Some(color) = color {
        props.push_str(&format!("<w:color w:val=\"{color}\"/>"));
    }
    let spacing = if before > 0 {
        format!("<w:pPr><w:spacing w:before=\"{before}\"/></w:pPr>")
    } else {
        String::new()
    };
    format!(
        r#"<w:p>{spacing}<w:r><w:rPr>{props}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        esc(text)
    )
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Zip со сжатием deflate. Своя сборка, потому что архив нужен ровно один
/// раз и ровно из трёх файлов: тянуть ради этого крейт незачем.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();

    for (name, data) in files {
        let offset = out.len() as u32;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();

        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        let _ = encoder.write_all(data);
        let packed = encoder.finish().unwrap_or_default();

        // Локальный заголовок файла.
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // нужная версия
        out.extend_from_slice(&0u16.to_le_bytes()); // флаги
        out.extend_from_slice(&8u16.to_le_bytes()); // метод: deflate
        out.extend_from_slice(&0u16.to_le_bytes()); // время
        out.extend_from_slice(&0u16.to_le_bytes()); // дата
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);

        // Запись в центральном каталоге — та же шапка плюс смещение.
        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes()); // кем создан
        directory.extend_from_slice(&20u16.to_le_bytes()); // нужная версия
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&8u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // extra
        directory.extend_from_slice(&0u16.to_le_bytes()); // комментарий
        directory.extend_from_slice(&0u16.to_le_bytes()); // диск
        directory.extend_from_slice(&0u16.to_le_bytes()); // внутренние атрибуты
        directory.extend_from_slice(&0u32.to_le_bytes()); // внешние атрибуты
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }

    let dir_offset = out.len() as u32;
    let dir_size = directory.len() as u32;
    out.extend_from_slice(&directory);

    // Конец центрального каталога.
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // номер диска
    out.extend_from_slice(&0u16.to_le_bytes()); // диск с каталогом
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&dir_size.to_le_bytes());
    out.extend_from_slice(&dir_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // комментарий
    out
}
