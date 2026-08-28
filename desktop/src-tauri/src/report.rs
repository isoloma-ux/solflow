//! Отчёт о проблеме: что за машина, что за версия, что в последних
//! ошибках. Собирается в текст, который человек отправляет почтой или
//! кладёт в буфер — сам ничего никуда не шлёт.
//!
//! Логи приложения, запущенного из Finder, уходят в никуда, поэтому
//! предупреждения и ошибки попутно копятся в кольцевом буфере: без них
//! отчёт бесполезен.

use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Сколько последних записей держим. Больше и не нужно: интересен хвост
/// перед тем, как что-то сломалось.
const KEEP: usize = 60;

static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Логгер поверх env_logger: печатает как раньше и запоминает жалобы.
struct Recorder {
    inner: env_logger::Logger,
}

impl Log for Recorder {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if record.level() <= Level::Warn {
            if let Ok(mut recent) = RECENT.lock() {
                if recent.len() >= KEEP {
                    recent.remove(0);
                }
                recent.push(format!(
                    "{} [{}] {}",
                    now_label(),
                    record.level(),
                    record.args()
                ));
            }
        }
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

fn now_label() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Часы:минуты:секунды по UTC — точной даты в отчёте достаточно.
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

pub fn init() {
    let inner = env_logger::Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .build();
    let max = inner.filter();
    if log::set_boxed_logger(Box::new(Recorder { inner })).is_ok() {
        log::set_max_level(max);
    }
}

/// Текст отчёта: версия, машина, настройки, последние жалобы в логе.
pub fn build(app: &tauri::AppHandle, description: &str) -> String {
    use tauri::Manager;

    let state = app.state::<crate::AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let model = state
        .engine
        .model_name
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "не загружена".to_string());

    let os = crate::sys::os_version();

    let recent = RECENT
        .lock()
        .map(|r| r.join("\n"))
        .unwrap_or_default();

    let mut out = String::new();
    if !description.trim().is_empty() {
        out.push_str(description.trim());
        out.push_str("\n\n");
    }
    out.push_str("— — —\n");
    out.push_str(&format!("Sol Flow {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "{} {os}, {}\n",
        crate::sys::OS_NAME,
        crate::sys::cpu_name()
    ));
    out.push_str(&format!(
        "Память {} ГБ\n",
        crate::sys::memory_bytes() / 1_073_741_824
    ));
    out.push_str(&format!("Модель: {model}\n"));
    out.push_str(&format!(
        "Микрофон: {}\n",
        settings.input_device.unwrap_or_else(|| "системный".to_string())
    ));
    out.push_str(&format!("Сочетание: {}\n", settings.hotkey));
    out.push_str(&format!(
        "Универсальный доступ: {}\n",
        if crate::paste::accessibility_granted() { "выдан" } else { "нет" }
    ));
    out.push_str(&format!(
        "Загрузчик ссылок: {}\n",
        if crate::tools::ready() { "стоит" } else { "нет" }
    ));
    out.push_str(&format!(
        "Разделение голосов: {}\n",
        if crate::diarize::models_ready(app) { "модели скачаны" } else { "нет" }
    ));
    out.push_str(&format!(
        "Встреч: {}\n",
        crate::meetings::rows(app).len()
    ));

    if !recent.is_empty() {
        out.push_str("\nПоследние сообщения:\n");
        out.push_str(&recent);
        out.push('\n');
    }
    out
}
