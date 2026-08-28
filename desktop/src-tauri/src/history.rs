//! История диктовки — что и когда надиктовали. Один JSON рядом с
//! настройками; записи удаляются по одной или всей пачкой.
//!
//! Здесь же короткий сигнал начала записи: он живёт рядом, потому что
//! оба относятся к диктовке и оба читают настройки.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    /// Момент диктовки в миллисекундах — он же ключ для удаления и имя
    /// файла со звуком.
    pub at: i64,
    pub text: String,
    /// Есть ли рядом звук, который можно переслушать.
    #[serde(default)]
    pub audio: bool,
    /// Длительность записи в секундах — для подписи в плеере.
    #[serde(default)]
    pub seconds: f32,
}

fn audio_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join("history"))
        .unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn audio_path(app: &AppHandle, at: i64) -> PathBuf {
    audio_dir(app).join(format!("{at}.wav"))
}

fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("history.json"))
        .unwrap_or_default()
}

pub fn all(app: &AppHandle) -> Vec<Entry> {
    std::fs::read_to_string(path(app))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, entries: &[Entry]) {
    let file = path(app);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(file, serde_json::to_string_pretty(entries).unwrap());
    let _ = app.emit("solflow-history", ());
}

/// Новая запись идёт наверх; пустой текст не сохраняем. Звук кладём
/// рядом отдельным WAV — из него потом играет плеер и идёт повторная
/// расшифровка.
pub fn add(app: &AppHandle, text: &str, pcm: Option<&[f32]>) {
    if text.trim().is_empty() {
        return;
    }
    let settings = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .clone();
    if settings.history_retention == "never" {
        return;
    }

    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut seconds = 0.0;
    let mut has_audio = false;
    if let Some(pcm) = pcm {
        seconds = pcm.len() as f32 / crate::wav::SAMPLE_RATE as f32;
        if let Ok(mut wav) = crate::wav::WavWriter::create(&audio_path(app, at)) {
            has_audio = wav.write(pcm).is_ok() && wav.finish().is_ok();
        }
    }

    let mut entries = all(app);
    entries.insert(
        0,
        Entry {
            at,
            text: text.to_string(),
            audio: has_audio,
            seconds,
        },
    );
    prune(app, &mut entries, &settings);
    save(app, &entries);
}

/// Чистка по правилам настроек: сначала по сроку, потом по количеству.
/// Файлы со звуком уходят вместе с записями, иначе папка растёт молча.
fn prune(app: &AppHandle, entries: &mut Vec<Entry>, settings: &crate::settings::Settings) {
    if let Some(ttl) = settings.retention_ms() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        entries.retain(|e| {
            let keep = now - e.at <= ttl;
            if !keep {
                let _ = std::fs::remove_file(audio_path(app, e.at));
            }
            keep
        });
    }
    if entries.len() > settings.history_limit {
        for gone in entries.iter().skip(settings.history_limit) {
            let _ = std::fs::remove_file(audio_path(app, gone.at));
        }
        entries.truncate(settings.history_limit);
    }
}

/// Применяет текущие правила к тому, что уже лежит: вызывается после
/// смены настроек, иначе новый лимит подействовал бы только на новые
/// записи.
pub fn apply_limits(app: &AppHandle) {
    let settings = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .clone();
    if settings.history_retention == "never" {
        clear(app);
        return;
    }
    let mut entries = all(app);
    prune(app, &mut entries, &settings);
    save(app, &entries);
}

pub fn remove(app: &AppHandle, at: i64) {
    let entries: Vec<Entry> = all(app).into_iter().filter(|e| e.at != at).collect();
    let _ = std::fs::remove_file(audio_path(app, at));
    save(app, &entries);
}

pub fn clear(app: &AppHandle) {
    for entry in all(app) {
        let _ = std::fs::remove_file(audio_path(app, entry.at));
    }
    save(app, &[]);
}

/// Заменяет текст записи — после повторной расшифровки другой моделью.
pub fn update_text(app: &AppHandle, at: i64, text: &str) {
    let mut entries = all(app);
    if let Some(entry) = entries.iter_mut().find(|e| e.at == at) {
        entry.text = text.to_string();
    }
    save(app, &entries);
}

/// Сигнал начала записи — тот же pop, что в десктопном Handy. Играет его
/// система (см. sys::play_wav), поэтому файл сначала кладётся на диск.
pub fn play_start_sound(app: &AppHandle) {
    const SOUND: &[u8] = include_bytes!("../sounds/start.wav");
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let file = dir.join("start.wav");
    if file.metadata().map(|m| m.len()).unwrap_or(0) != SOUND.len() as u64 {
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(&file, SOUND).is_err() {
            return;
        }
    }
    crate::sys::play_wav(&file);
}
