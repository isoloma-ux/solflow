//! Язык сообщений, которые приходят из Rust: меню в трее, подписи клавиш,
//! ошибки. Словарь тот же по устройству, что в окне: ключ — русская строка,
//! без перевода остаётся она сама.
//!
//! Язык не определяется здесь, а приходит из окна: там его уже выбрали по
//! настройке и системе, и два независимых определения рано или поздно
//! разошлись бы.

use std::sync::Mutex;

pub struct Language(pub Mutex<String>);

impl Language {
    pub fn new() -> Self {
        Self(Mutex::new("ru".to_string()))
    }
}

/// Английский язык интерфейса включён?
pub fn is_english(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<Language>()
        .map(|l| l.0.lock().map(|v| *v == "en").unwrap_or(false))
        .unwrap_or(false)
}

/// Перевод строки для текущего языка окна.
pub fn t(app: &tauri::AppHandle, text: &str) -> String {
    if !is_english(app) {
        return text.to_string();
    }
    EN.iter()
        .find(|(ru, _)| *ru == text)
        .map(|(_, en)| en.to_string())
        .unwrap_or_else(|| text.to_string())
}

/// Строки, которые человек видит из Rust.
const EN: &[(&str, &str)] = &[
    // трей
    ("Открыть Sol Flow", "Open Sol Flow"),
    ("Выйти", "Quit"),
    // подписи клавиш
    ("Пробел", "Space"),
    // вставка текста
    (
        "Текст в буфере обмена — нажмите ⌘V. Для автовставки включите Универсальный доступ",
        "The text is in the clipboard — press ⌘V. Turn on Accessibility for automatic pasting",
    ),
    (
        "Текст в буфере обмена — нажмите Ctrl+V",
        "The text is in the clipboard — press Ctrl+V",
    ),
];
