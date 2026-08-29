//! Разбор сочетания из строки настроек в Shortcut и обратно в подпись
//! для интерфейса («⌥ Пробел» на Mac, «Ctrl + Пробел» на Windows).

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// "alt+space", "cmd+shift+d" → Shortcut. Регистр не важен.
pub fn parse(text: &str) -> Option<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in text.split('+').map(|p| p.trim().to_lowercase()) {
        match part.as_str() {
            "cmd" | "meta" | "command" | "super" => mods |= Modifiers::META,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            key => code = key_code(key),
        }
    }
    // Без модификатора глобальное сочетание было бы слишком легко нажать
    // случайно, поэтому хотя бы один обязателен.
    let code = code?;
    if mods.is_empty() {
        return None;
    }
    Some(Shortcut::new(Some(mods), code))
}

fn key_code(key: &str) -> Option<Code> {
    let code = match key {
        "space" | "пробел" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backquote" | "`" => Code::Backquote,
        "comma" | "," => Code::Comma,
        "period" | "." => Code::Period,
        "slash" | "/" => Code::Slash,
        "semicolon" | ";" => Code::Semicolon,
        "quote" | "'" => Code::Quote,
        "minus" | "-" => Code::Minus,
        "equal" | "=" => Code::Equal,
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC, "d" => Code::KeyD,
        "e" => Code::KeyE, "f" => Code::KeyF, "g" => Code::KeyG, "h" => Code::KeyH,
        "i" => Code::KeyI, "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO, "p" => Code::KeyP,
        "q" => Code::KeyQ, "r" => Code::KeyR, "s" => Code::KeyS, "t" => Code::KeyT,
        "u" => Code::KeyU, "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2,
        "3" => Code::Digit3, "4" => Code::Digit4, "5" => Code::Digit5,
        "6" => Code::Digit6, "7" => Code::Digit7, "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3, "f4" => Code::F4,
        "f5" => Code::F5, "f6" => Code::F6, "f7" => Code::F7, "f8" => Code::F8,
        "f9" => Code::F9, "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        _ => return None,
    };
    Some(code)
}

/// Подпись сочетания для интерфейса. На Mac это значки клавиш, на Windows
/// значков нет — там их пишут словами.
pub fn label(app: &tauri::AppHandle, text: &str) -> String {
    let mac = cfg!(target_os = "macos");
    text.split('+')
        .map(|p| match p.trim().to_lowercase().as_str() {
            "cmd" | "meta" | "command" | "super" => {
                if mac { "⌘" } else { "Win" }.to_string()
            }
            "alt" | "option" | "opt" => if mac { "⌥" } else { "Alt" }.to_string(),
            "shift" => if mac { "⇧" } else { "Shift" }.to_string(),
            "ctrl" | "control" => if mac { "⌃" } else { "Ctrl" }.to_string(),
            "space" => crate::lang::t(app, "Пробел"),
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(if mac { " " } else { " + " })
}
