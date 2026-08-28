//! Минимальный разбор TrueType: столько, сколько нужно, чтобы встроить
//! шрифт в PDF и посчитать ширину строки — номер глифа по символу,
//! ширины глифов и размеры из head/hhea/OS2.
//!
//! Сабсеттинга нет: Inter Regular и Medium весят по 400 КБ, в PDF они
//! уходят сжатыми целиком. Резать таблицы glyf ради экономии половины
//! мегабайта — работа несоразмерная выигрышу.

pub struct Font {
    data: &'static [u8],
    pub units_per_em: f32,
    /// Ширины глифов в единицах шрифта; последняя повторяется для хвоста.
    widths: Vec<u16>,
    cmap: Vec<(u32, u32, u16)>, // начало, конец, номер первого глифа
    pub ascent: i16,
    pub descent: i16,
    pub cap_height: i16,
    pub bbox: [i16; 4],
    pub num_glyphs: u16,
}

fn u16_at(d: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([d[off], d[off + 1]])
}

fn i16_at(d: &[u8], off: usize) -> i16 {
    i16::from_be_bytes([d[off], d[off + 1]])
}

fn u32_at(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

impl Font {
    pub fn parse(data: &'static [u8]) -> Option<Self> {
        let num_tables = u16_at(data, 4) as usize;
        let mut tables = std::collections::HashMap::new();
        for i in 0..num_tables {
            let off = 12 + i * 16;
            if off + 16 > data.len() {
                return None;
            }
            let tag = &data[off..off + 4];
            let start = u32_at(data, off + 8) as usize;
            let len = u32_at(data, off + 12) as usize;
            tables.insert(tag.to_vec(), (start, len));
        }
        let table = |name: &[u8]| tables.get(name).copied();

        let (head, _) = table(b"head")?;
        let units_per_em = u16_at(data, head + 18) as f32;
        let bbox = [
            i16_at(data, head + 36),
            i16_at(data, head + 38),
            i16_at(data, head + 40),
            i16_at(data, head + 42),
        ];

        let (hhea, _) = table(b"hhea")?;
        let ascent = i16_at(data, hhea + 4);
        let descent = i16_at(data, hhea + 6);
        let num_h_metrics = u16_at(data, hhea + 34) as usize;

        let (maxp, _) = table(b"maxp")?;
        let num_glyphs = u16_at(data, maxp + 4);

        let (hmtx, _) = table(b"hmtx")?;
        let widths: Vec<u16> = (0..num_h_metrics)
            .map(|i| u16_at(data, hmtx + i * 4))
            .collect();

        // OS/2 знает высоту прописных; без него берём три четверти подъёма.
        let cap_height = table(b"OS/2")
            .filter(|(os2, len)| *len >= 90 && u16_at(data, *os2) >= 2)
            .map(|(os2, _)| i16_at(data, os2 + 88))
            .filter(|v| *v > 0)
            .unwrap_or((ascent as f32 * 0.72) as i16);

        let (cmap_off, _) = table(b"cmap")?;
        let cmap = parse_cmap(data, cmap_off)?;

        Some(Font {
            data,
            units_per_em,
            widths,
            cmap,
            ascent,
            descent,
            cap_height,
            bbox,
            num_glyphs,
        })
    }

    pub fn bytes(&self) -> &'static [u8] {
        self.data
    }

    /// Номер глифа; 0 (.notdef) — если символа в шрифте нет.
    pub fn glyph(&self, ch: char) -> u16 {
        let code = ch as u32;
        for &(start, end, first) in &self.cmap {
            if code >= start && code <= end {
                return first.wrapping_add((code - start) as u16);
            }
        }
        0
    }

    /// Ширина глифа в тысячных долях кегля — как принято в PDF.
    pub fn advance(&self, glyph: u16) -> f32 {
        let raw = self
            .widths
            .get(glyph as usize)
            .or_else(|| self.widths.last())
            .copied()
            .unwrap_or(0) as f32;
        raw * 1000.0 / self.units_per_em
    }

    /// Ширина строки в пунктах при заданном кегле.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| self.advance(self.glyph(c)) * size / 1000.0)
            .sum()
    }
}

/// Берём подтаблицу Unicode: формат 12 предпочтительнее (полный диапазон),
/// иначе формат 4.
fn parse_cmap(d: &[u8], cmap: usize) -> Option<Vec<(u32, u32, u16)>> {
    let count = u16_at(d, cmap + 2) as usize;
    let mut best: Option<(usize, u16)> = None;
    for i in 0..count {
        let rec = cmap + 4 + i * 8;
        let platform = u16_at(d, rec);
        let encoding = u16_at(d, rec + 2);
        let offset = u32_at(d, rec + 4) as usize;
        let unicode = (platform == 0) || (platform == 3 && (encoding == 1 || encoding == 10));
        if !unicode {
            continue;
        }
        let format = u16_at(d, cmap + offset);
        let rank = match format {
            12 => 2,
            4 => 1,
            _ => continue,
        };
        if best.map(|(_, r)| rank > r).unwrap_or(true) {
            best = Some((cmap + offset, rank));
        }
    }

    let (table, _) = best?;
    match u16_at(d, table) {
        12 => {
            let groups = u32_at(d, table + 12) as usize;
            Some(
                (0..groups)
                    .map(|i| {
                        let g = table + 16 + i * 12;
                        (u32_at(d, g), u32_at(d, g + 4), u32_at(d, g + 8) as u16)
                    })
                    .collect(),
            )
        }
        4 => {
            let seg_x2 = u16_at(d, table + 6) as usize;
            let segs = seg_x2 / 2;
            let ends = table + 14;
            let starts = ends + seg_x2 + 2;
            let deltas = starts + seg_x2;
            let ranges = deltas + seg_x2;

            let mut out = Vec::with_capacity(segs);
            for i in 0..segs {
                let end = u16_at(d, ends + i * 2) as u32;
                let start = u16_at(d, starts + i * 2) as u32;
                if start > end || start == 0xFFFF {
                    continue;
                }
                let delta = u16_at(d, deltas + i * 2);
                let range_offset = u16_at(d, ranges + i * 2) as usize;
                if range_offset == 0 {
                    out.push((start, end, (start as u16).wrapping_add(delta)));
                } else {
                    // Разреженный сегмент: каждый символ смотрится отдельно.
                    for code in start..=end {
                        let idx = ranges + i * 2 + range_offset + (code - start) as usize * 2;
                        if idx + 1 >= d.len() {
                            break;
                        }
                        let glyph = u16_at(d, idx);
                        if glyph != 0 {
                            out.push((code, code, glyph.wrapping_add(delta)));
                        }
                    }
                }
            }
            Some(out)
        }
        _ => None,
    }
}
