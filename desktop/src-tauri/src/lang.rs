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

/// Use the English catalog as the source for all non-Russian UI translations.
pub fn is_english(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<Language>()
        .map(|l| l.0.lock().map(|v| *v != "ru").unwrap_or(false))
        .unwrap_or(false)
}

/// Перевод строки для текущего языка окна.
pub fn t(app: &tauri::AppHandle, text: &str) -> String {
    use tauri::Manager;
    if !is_english(app) {
        return text.to_string();
    }
    let english = EN.iter()
        .find(|(ru, _)| *ru == text)
        .map(|(_, en)| *en)
        .unwrap_or(text);
    let language = app.try_state::<Language>()
        .and_then(|state| state.0.lock().ok().map(|value| value.clone()))
        .unwrap_or_else(|| "en".to_string());
    static DICTIONARIES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let dictionaries = DICTIONARIES.get_or_init(|| serde_json::from_str(include_str!("../../ui/guide/locales.json")).expect("validated UI translations"));
    dictionaries.get(&language).and_then(|dictionary| dictionary.get(english)).and_then(|value| value.as_str()).unwrap_or(english).to_string()
}

/// Строки, которые человек видит из Rust.
const EN: &[(&str, &str)] = &[
    ("Расставляю знаки препинания…", "Adjusting punctuation…"),
    ("Не удалось сохранить историю", "Could not save history"),
    ("Не удалось прочитать историю", "Could not read history"),
    ("Запись не найдена", "Recording not found"),
    ("Дождитесь завершения текущей операции", "Wait for the current operation to finish"),
    ("Речь не распознана. Прежний текст и звук сохранены.", "Speech was not recognized. The previous transcript and audio have been preserved."),
    // трей
    ("Открыть Sol Flow", "Open Sol Flow"),
    ("Выйти", "Quit"),
    // обновление
    (
        "Вышла версия {0} — обновление ждет в подвале окна",
        "Version {0} is out — the update is waiting at the bottom of the window",
    ),
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
