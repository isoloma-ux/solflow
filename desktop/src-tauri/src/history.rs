//! История диктовки — что и когда надиктовали. Один JSON рядом с
//! настройками; записи удаляются по одной или всей пачкой.
//!
//! Здесь же короткий сигнал начала записи: он живёт рядом, потому что
//! оба относятся к диктовке и оба читают настройки.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

static WRITE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    /// Момент диктовки в миллисекундах — он же ключ для удаления и имя
    /// файла со звуком.
    pub at: i64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punctuation_status: Option<String>,
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
    read_for_update(app).unwrap_or_default()
}

fn read_for_update(app: &AppHandle) -> Result<Vec<Entry>, String> {
    match std::fs::read(path(app)) {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }.map_err(|e| {
        let message = format!("{}: {e}", crate::lang::t(app, "Не удалось прочитать историю"));
        let _ = app.emit("solflow-history-failed", &message);
        message
    })
}

fn save(app: &AppHandle, entries: &[Entry]) -> Result<(), String> {
    let file = path(app);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?;
    if let Err(e) = crate::transcript_store::atomic_write(&file, &bytes) {
        let message = format!("{}: {e}", crate::lang::t(app, "Не удалось сохранить историю"));
        let _ = app.emit("solflow-history-failed", &message);
        return Err(message);
    }
    let _ = app.emit("solflow-history", ());
    Ok(())
}

/// Новая запись идёт наверх; пустой текст не сохраняем. Звук кладём
/// рядом отдельным WAV — из него потом играет плеер и идёт повторная
/// расшифровка.
pub fn add(app: &AppHandle, text: &str, original: &str, pcm: Option<&[f32]>) -> Result<Option<i64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let settings = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .clone();
    if settings.history_retention == "never" {
        return Ok(None);
    }
    let _guard = WRITE_GATE.lock().unwrap();

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

    let mut entries = read_for_update(app)?;
    entries.insert(
        0,
        Entry {
            at,
            text: text.to_string(),
            original_text: Some(original.to_string()),
            previous_text: None,
            punctuation_status: None,
            audio: has_audio,
            seconds,
        },
    );
    prune(app, &mut entries, &settings);
    save(app, &entries)?;
    Ok(Some(at))
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
    let _guard = WRITE_GATE.lock().unwrap();
    let Ok(mut entries) = read_for_update(app) else { return; };
    prune(app, &mut entries, &settings);
    let _ = save(app, &entries);
}

pub fn remove(app: &AppHandle, at: i64) {
    let _guard = WRITE_GATE.lock().unwrap();
    let Ok(entries) = read_for_update(app) else { return; };
    let entries: Vec<Entry> = entries.into_iter().filter(|e| e.at != at).collect();
    let _ = std::fs::remove_file(audio_path(app, at));
    let _ = save(app, &entries);
}

pub fn clear(app: &AppHandle) {
    let _guard = WRITE_GATE.lock().unwrap();
    let Ok(entries) = read_for_update(app) else { return; };
    for entry in entries {
        let _ = std::fs::remove_file(audio_path(app, entry.at));
    }
    let _ = save(app, &[]);
}

/// Заменяет текст записи — после повторной расшифровки другой моделью.
pub fn update_text(app: &AppHandle, at: i64, text: &str, original: &str, status: &str, keep_previous: bool) -> Result<(), String> {
    let _guard = WRITE_GATE.lock().unwrap();
    let mut entries = read_for_update(app)?;
    if let Some(entry) = entries.iter_mut().find(|e| e.at == at) {
        if keep_previous && entry.text != text {
            entry.previous_text = Some(entry.text.clone());
        }
        entry.text = text.to_string();
        entry.original_text = Some(original.to_string());
        entry.punctuation_status = Some(status.to_string());
        save(app, &entries)?;
    }
    Ok(())
}

/// Сигнал начала записи — тот же pop, что в десктопном Handy. Играет его
/// система (см. sys::play_wav), поэтому файл сначала кладётся на диск.
/// Сигнал уходит в отдельный поток: на Windows PlaySound возвращается,
/// только когда звуковое устройство проснулось и заиграло, а спящий
/// после простоя выход просыпается до секунд. Раньше это ожидание сидело
/// прямо перед открытием микрофона и задерживало старт записи.
pub fn play_start_sound(app: &AppHandle) {
    const SOUND: &[u8] = include_bytes!("../sounds/start.wav");
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    std::thread::spawn(move || {
        let file = dir.join("start.wav");
        if file.metadata().map(|m| m.len()).unwrap_or(0) != SOUND.len() as u64 {
            let _ = std::fs::create_dir_all(&dir);
            if std::fs::write(&file, SOUND).is_err() {
                return;
            }
        }
        let started = std::time::Instant::now();
        crate::sys::play_wav(&file);
        let ms = started.elapsed().as_millis();
        // Долгое пробуждение звука — та же жалоба, что и долгий старт
        // микрофона: пусть попадёт в отчёт о проблеме.
        if ms >= 300 {
            log::warn!("сигнал старта зазвучал через {ms} мс");
        } else {
            log::info!("сигнал старта зазвучал через {ms} мс");
        }
    });
}
