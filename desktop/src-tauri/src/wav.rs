//! Потоковый WAV — порт WavIo с Android. Запись уходит на диск по мере
//! поступления: двухчасовая встреча — это ~230 МБ, копить её в памяти,
//! как делает диктовка, нельзя. Формат фиксированный — PCM16 моно 16 кГц,
//! тот же, что ждёт движок.
//!
//! Размеры в заголовке дописываются при закрытии; если процесс погиб до
//! finish(), файл чинится по фактической длине при чтении.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::Result;

pub const SAMPLE_RATE: usize = 16_000;
const HEADER_BYTES: u64 = 44;

pub struct WavWriter {
    out: BufWriter<File>,
    data_bytes: u64,
}

impl WavWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path)?;
        let mut out = BufWriter::with_capacity(1 << 16, file);
        out.write_all(&header(0))?;
        Ok(Self { out, data_bytes: 0 })
    }

    pub fn samples_written(&self) -> u64 {
        self.data_bytes / 2
    }

    pub fn write(&mut self, pcm: &[f32]) -> Result<()> {
        let mut bytes = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.out.write_all(&bytes)?;
        self.data_bytes += bytes.len() as u64;
        Ok(())
    }

    /// Сбрасывает поток и вписывает настоящие размеры в заголовок.
    pub fn finish(mut self) -> Result<u64> {
        self.out.flush()?;
        let mut file = self.out.into_inner()?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.data_bytes))?;
        Ok(self.data_bytes / 2)
    }
}

fn header(data: u64) -> [u8; HEADER_BYTES as usize] {
    let mut b = [0u8; HEADER_BYTES as usize];
    b[0..4].copy_from_slice(b"RIFF");
    b[4..8].copy_from_slice(&((data + HEADER_BYTES - 8) as u32).to_le_bytes());
    b[8..12].copy_from_slice(b"WAVE");
    b[12..16].copy_from_slice(b"fmt ");
    b[16..20].copy_from_slice(&16u32.to_le_bytes());
    b[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    b[22..24].copy_from_slice(&1u16.to_le_bytes()); // моно
    b[24..28].copy_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    b[28..32].copy_from_slice(&(SAMPLE_RATE as u32 * 2).to_le_bytes());
    b[32..34].copy_from_slice(&2u16.to_le_bytes()); // байт на кадр
    b[34..36].copy_from_slice(&16u16.to_le_bytes()); // бит на отсчёт
    b[36..40].copy_from_slice(b"data");
    b[40..44].copy_from_slice(&(data as u32).to_le_bytes());
    b
}

/// Чтение WAV кусками — файл целиком в память не поднимается. Заголовку не
/// верим на слово: свой после обрыва записи хранит нули, а afconvert пишет
/// лишние чанки, поэтому начало данных ищется по чанку `data`, а длина
/// берётся по факту.
pub struct WavReader {
    file: File,
    data_offset: u64,
    pub total_samples: u64,
}

impl WavReader {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();

        // Обход чанков: afconvert кладёт перед данными FLLR-заполнитель на
        // 4 КБ, поэтому фиксированные 44 байта — только запасной вариант.
        let mut head = vec![0u8; 16384.min(len as usize)];
        let n = file.read(&mut head)?;
        head.truncate(n);

        let mut data_offset = HEADER_BYTES;
        let mut off = 12usize; // после RIFF-размера и "WAVE"
        while off + 8 <= head.len() {
            let id = &head[off..off + 4];
            let size = u32::from_le_bytes([
                head[off + 4],
                head[off + 5],
                head[off + 6],
                head[off + 7],
            ]) as usize;
            if id == b"data" {
                data_offset = off as u64 + 8;
                break;
            }
            off += 8 + size;
        }

        Ok(Self {
            file,
            data_offset,
            total_samples: len.saturating_sub(data_offset) / 2,
        })
    }

    /// Читает [count] отсчётов начиная с [from], как f32 в [-1, 1].
    pub fn read(&mut self, from: u64, count: usize) -> Result<Vec<f32>> {
        let n = (count as u64).min(self.total_samples.saturating_sub(from)) as usize;
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut bytes = vec![0u8; n * 2];
        self.file.seek(SeekFrom::Start(self.data_offset + from * 2))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect())
    }
}
