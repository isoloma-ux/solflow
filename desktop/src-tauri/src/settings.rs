//! Настройки приложения — один JSON в Application Support.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Сочетание диктовки, строкой вида "alt+space" — настраивается в окне.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Имя файла активной модели из каталога.
    #[serde(default)]
    pub active_model: Option<String>,
    /// Имя микрофона; None — системный по умолчанию. С наушниками system
    /// default может оказаться не тем, который нужен.
    #[serde(default)]
    pub input_device: Option<String>,
    /// Короткий сигнал в начале записи — чтобы не гадать, слышит ли она.
    #[serde(default = "yes")]
    pub start_sound: bool,
    /// Держать звуковой выход проснувшимся тихим потоком: на Windows
    /// спящая после простоя звуковая карта просыпается до двух секунд, и
    /// сигнал начала записи запаздывал. Действует только вместе с сигналом.
    #[serde(default = "yes")]
    pub keep_audio_awake: bool,
    /// Тема окна: "system" — как в macOS, иначе "light" или "dark".
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Куда класть скачанное по ссылке. None — не оставлять: приложению
    /// нужен только звук, а исходник может весить гигабайты.
    #[serde(default)]
    pub downloads_dir: Option<String>,

    /// Куда складывать экспорт встреч. None — папка «Загрузки», как было
    /// раньше.
    #[serde(default)]
    pub export_dir: Option<String>,

    /// Язык интерфейса: "auto" — как в системе, иначе "ru" или "en".
    #[serde(default = "default_language")]
    pub language: String,

    /// Спрашивать папку при каждом экспорте. Перевешивает export_dir.
    #[serde(default)]
    pub export_ask: bool,

    /// Считать на видеокарте, если она подходит. Выключенное — строго
    /// процессор: на редких драйверах видеокарта считает неверно, и человеку
    /// нужен способ это обойти.
    #[serde(default = "default_use_gpu")]
    pub use_gpu: bool,

    // --- запуск и оформление ---
    /// Запускаться без окна: приложение сразу уходит в меню-бар.
    #[serde(default)]
    pub start_hidden: bool,
    /// Иконка в меню-баре. Без неё окно возвращается повторным запуском.
    #[serde(default = "yes")]
    pub show_tray_icon: bool,
    /// Пилюля во время записи: "live" — с волной и таймером,
    /// "minimal" — точка, "none" — не показывать.
    #[serde(default = "default_overlay_style")]
    pub overlay_style: String,
    /// Где она появляется: "bottom" или "top".
    #[serde(default = "default_overlay_position")]
    pub overlay_position: String,

    // --- поведение диктовки ---
    /// Когда выгружать модель из памяти: "never", "immediately", "min2",
    /// "min5", "min10", "min15", "hour1".
    #[serde(default = "default_unload")]
    pub model_unload: String,
    /// Буфер обмена: "restore" — вернуть, что там было, "keep" — оставить
    /// распознанный текст.
    #[serde(default = "default_clipboard")]
    pub clipboard_handling: String,
    /// Нажимать ли ввод после вставки текста.
    #[serde(default)]
    pub auto_submit: bool,
    /// Чем именно: "enter", "ctrl_enter", "cmd_enter".
    #[serde(default = "default_submit_key")]
    pub auto_submit_key: String,
    /// Приглушать системный звук на время записи, чтобы музыка из колонок
    /// не лезла в микрофон.
    #[serde(default)]
    pub mute_while_recording: bool,
    /// Убирать слова-паразиты из распознанного текста.
    #[serde(default)]
    pub remove_fillers: bool,

    // --- история ---
    /// Сколько последних диктовок держать.
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    /// Когда чистить: "keep_limit" — только по числу, "never" — не хранить
    /// вовсе, "days3", "weeks2", "months3".
    #[serde(default = "default_retention")]
    pub history_retention: String,
    /// Хранить ли звук диктовок — чтобы можно было переслушать.
    #[serde(default = "yes")]
    pub keep_audio: bool,
    /// Звук записей встреч: "keep" — хранить, "delete_done" — удалять
    /// сразу после расшифровки. Без звука не переразобрать и не выгрузить
    /// .wav, зато часовая запись не лежит гигабайтом.
    #[serde(default = "default_meeting_audio")]
    pub meeting_audio: String,

    // --- синхронизация через облако ---
    /// Какое облако подключено: "yandex" или "google". Пусто при токене —
    /// настройки от версий до Google Drive, значит Яндекс.
    #[serde(default)]
    pub sync_provider: String,
    /// OAuth-токен вошедшего человека; None — не подключено. Имена полей в
    /// файле остались яндексовскими через alias — старые настройки читаются.
    #[serde(default, alias = "yandex_token")]
    pub sync_token: Option<String>,
    #[serde(default, alias = "yandex_refresh")]
    pub sync_refresh: Option<String>,
    /// Когда токен перестанет работать (millis); продлевается заранее.
    #[serde(default, alias = "yandex_expires_at")]
    pub sync_expires_at: i64,
    /// Кто вошёл — показывается в настройках.
    #[serde(default, alias = "yandex_login")]
    pub sync_login: String,
    /// Передавать ли звук записей. По умолчанию нет: часовая встреча —
    /// больше ста мегабайт, а расшифровке и саммери звук не нужен.
    #[serde(default)]
    pub sync_audio: bool,
    /// Считать саммери и название для готовых встреч, приехавших с других
    /// устройств, — на телефоне модели нет, компьютер считает за него.
    #[serde(default = "yes")]
    pub sync_auto_summary: bool,
    /// Как часто заглядывать на Диск за чужими изменениями: "min1", "min2",
    /// "min5", "min15", "hour1" или "manual" — только по кнопке. Свои правки
    /// уезжают всегда, через 20 секунд после последней.
    #[serde(default = "default_sync_interval")]
    pub sync_interval: String,
}

impl Settings {
    /// Интервал проверки Диска; None — только вручную.
    pub fn sync_period(&self) -> Option<std::time::Duration> {
        use std::time::Duration;
        match self.sync_interval.as_str() {
            "manual" => None,
            "min1" => Some(Duration::from_secs(60)),
            "min5" => Some(Duration::from_secs(5 * 60)),
            "min15" => Some(Duration::from_secs(15 * 60)),
            "hour1" => Some(Duration::from_secs(3600)),
            _ => Some(Duration::from_secs(2 * 60)),
        }
    }
}

fn default_sync_interval() -> String {
    "min2".to_string()
}

impl Settings {
    /// Через сколько секунд простоя выгружать модель; None — никогда.
    pub fn unload_after(&self) -> Option<u64> {
        match self.model_unload.as_str() {
            "never" => None,
            "immediately" => Some(0),
            "min2" => Some(120),
            "min10" => Some(600),
            "min15" => Some(900),
            "hour1" => Some(3600),
            _ => Some(300),
        }
    }

    /// Сколько миллисекунд держать историю; None — без срока.
    pub fn retention_ms(&self) -> Option<i64> {
        match self.history_retention.as_str() {
            "days3" => Some(3 * 24 * 3600 * 1000),
            "weeks2" => Some(14 * 24 * 3600 * 1000),
            "months3" => Some(90 * 24 * 3600 * 1000),
            _ => None,
        }
    }
}

/// На Windows Alt+Space занят системой — им открывается меню окна, —
/// поэтому там по умолчанию Ctrl+Space.
fn default_language() -> String {
    "auto".to_string()
}

fn default_use_gpu() -> bool {
    true
}

fn default_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "alt+space".to_string()
    } else {
        "ctrl+space".to_string()
    }
}

fn yes() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_overlay_style() -> String {
    "live".to_string()
}

fn default_overlay_position() -> String {
    "bottom".to_string()
}

fn default_unload() -> String {
    "min5".to_string()
}

fn default_clipboard() -> String {
    "restore".to_string()
}

fn default_submit_key() -> String {
    "enter".to_string()
}

fn default_history_limit() -> usize {
    50
}

fn default_meeting_audio() -> String {
    "keep".to_string()
}

fn default_retention() -> String {
    "keep_limit".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            active_model: None,
            input_device: None,
            start_sound: true,
            keep_audio_awake: true,
            theme: default_theme(),
            downloads_dir: None,
            export_dir: None,
            export_ask: false,
            language: default_language(),
            use_gpu: default_use_gpu(),
            start_hidden: false,
            show_tray_icon: true,
            overlay_style: default_overlay_style(),
            overlay_position: default_overlay_position(),
            model_unload: default_unload(),
            clipboard_handling: default_clipboard(),
            auto_submit: false,
            auto_submit_key: default_submit_key(),
            mute_while_recording: false,
            remove_fillers: false,
            history_limit: default_history_limit(),
            history_retention: default_retention(),
            keep_audio: true,
            meeting_audio: default_meeting_audio(),
            sync_provider: String::new(),
            sync_token: None,
            sync_refresh: None,
            sync_expires_at: 0,
            sync_login: String::new(),
            sync_audio: false,
            sync_auto_summary: true,
            sync_interval: default_sync_interval(),
        }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) {
    if let Some(p) = path(app) {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, serde_json::to_string_pretty(settings).unwrap());
    }
}
