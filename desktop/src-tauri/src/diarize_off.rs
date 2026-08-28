//! Диаризация без sherpa-onnx: тот же набор функций, но каждая честно
//! говорит, что разделение говорящих в этой сборке не собрано. Подставляется
//! вместо `diarize.rs`, когда выключена фича `diarize` (см. lib.rs) — так
//! Windows-сборка живёт без статических библиотек k2-fsa, а весь остальной
//! код о разнице не знает.

use std::path::Path;

use anyhow::{anyhow, Result};

pub const DOWNLOAD_MB: u64 = 0;

pub fn models_ready(_app: &tauri::AppHandle) -> bool {
    false
}

pub fn download(
    _app: &tauri::AppHandle,
    _on_progress: &dyn Fn(u8),
    _cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    Err(unsupported())
}

pub fn run(
    _app: &tauri::AppHandle,
    _audio: &Path,
    _segments: &[(f32, f32)],
    _num_speakers: i32,
    _on_progress: &dyn Fn(u8),
    _cancelled: &dyn Fn() -> bool,
) -> Result<Vec<usize>> {
    Err(unsupported())
}

pub fn voice_similarity(_a: &Path, _b: &Path, _emb_model: &Path) -> Result<f32> {
    Err(unsupported())
}

pub fn turns_for_example(
    _audio: &Path,
    _seg_model: &Path,
    _emb_model: &Path,
    _num_speakers: i32,
) -> Result<Vec<(f32, f32, usize)>> {
    Err(unsupported())
}

fn unsupported() -> anyhow::Error {
    anyhow!("разделение говорящих в этой сборке недоступно")
}
