//! Вставка распознанного текста в активное приложение: текст кладётся в
//! буфер обмена, отправляется Cmd+V (Ctrl+V на Windows), прежний текстовый
//! буфер возвращается.
//! Приём из десктопного Handy. Требует разрешения «Универсальный доступ».
//!
//! Enigo создаётся один раз на главном потоке и живёт в state: его
//! инициализация дергает HIToolbox, который вне main queue роняет процесс
//! (dispatch_assert_queue в TSMGetInputSourceProperty — проверено крэшем).
//! Cmd+V шлётся виртуальным кодом клавиши, а не символом: перевод символа
//! в код тоже ходит в раскладку через HIToolbox.

use std::sync::Mutex;

use anyhow::{anyhow, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Виртуальный код клавиши V — не зависит от раскладки: kVK_ANSI_V на
/// macOS, VK_V на Windows.
#[cfg(target_os = "macos")]
const VK_V: u32 = 9;
#[cfg(not(target_os = "macos"))]
const VK_V: u32 = 0x56;

/// Модификатор «вставить»: ⌘ на macOS, Ctrl везде ещё.
#[cfg(target_os = "macos")]
const PASTE_MOD: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const PASTE_MOD: Key = Key::Control;

/// Что писать, когда автовставка не вышла и текст остался в буфере.
#[cfg(target_os = "macos")]
const PASTE_YOURSELF: &str =
    "Текст в буфере обмена — нажмите ⌘V. Для автовставки включите Универсальный доступ";
#[cfg(not(target_os = "macos"))]
const PASTE_YOURSELF: &str = "Текст в буфере обмена — нажмите Ctrl+V";

pub struct EnigoState(pub Mutex<Option<Enigo>>);

impl EnigoState {
    /// Пустая: сама «нажималка» создаётся при первой попытке вставить.
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// Создаёт «нажималку», если её ещё нет.
///
/// Раньше она создавалась один раз при запуске, и это было ловушкой: без
/// «Универсального доступа» создание падает, а разрешение человек выдаёт
/// уже после запуска — вставка молчала до перезапуска приложения. Теперь
/// попытка повторяется, и первая же вставка после выдачи права работает.
///
/// Создавать можно только на главном потоке: инициализация дёргает
/// HIToolbox, который вне главной очереди роняет процесс.
fn ensure_enigo(app: &AppHandle) -> Result<()> {
    let state = app
        .try_state::<EnigoState>()
        .ok_or_else(|| anyhow!("нажималка не заведена"))?;
    if state.0.lock().map(|e| e.is_some()).unwrap_or(false) {
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    handle
        .clone()
        .run_on_main_thread(move || {
            let made = Enigo::new(&Settings::default()).map_err(|e| format!("{e}"));
            match made {
                Ok(enigo) => {
                    if let Some(state) = handle.try_state::<EnigoState>() {
                        *state.0.lock().unwrap() = Some(enigo);
                    }
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        })
        .map_err(|e| anyhow!("{e}"))?;

    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| anyhow!("нажималка не ответила"))?
        .map_err(|e| anyhow!("{e}"))
}

/// Разрешение «Универсальный доступ» выдано? Без него Cmd+V не долетит.
#[cfg(target_os = "macos")]
pub fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// На Windows отдельного разрешения на ввод нет: нажатия уходят сразу.
#[cfg(not(target_os = "macos"))]
pub fn accessibility_granted() -> bool {
    true
}

/// Как обойтись с буфером и нажимать ли ввод — решают настройки.
pub struct PasteOptions {
    /// Вернуть прежнее содержимое буфера после вставки.
    pub restore_clipboard: bool,
    /// Нажать ввод после вставки: None — не нажимать.
    pub submit: Option<SubmitKey>,
}

#[derive(Clone, Copy)]
pub enum SubmitKey {
    Enter,
    CtrlEnter,
    CmdEnter,
}

impl SubmitKey {
    pub fn parse(name: &str) -> Self {
        match name {
            "ctrl_enter" => SubmitKey::CtrlEnter,
            "cmd_enter" => SubmitKey::CmdEnter,
            _ => SubmitKey::Enter,
        }
    }
}

pub fn paste_text(app: &AppHandle, text: &str, options: &PasteOptions) -> Result<()> {
    // Без «Универсального доступа» Cmd+V молча не долетает. Деградируем как
    // на Android: текст остаётся в буфере, пользователь вставит его сам, —
    // и прежний буфер в этом случае НЕ восстанавливаем, иначе результат
    // затирается через треть секунды.
    if !accessibility_granted() {
        app.clipboard()
            .write_text(text.to_string())
            .map_err(|e| anyhow!("буфер обмена: {e}"))?;
        return Err(anyhow!(crate::lang::t(app, PASTE_YOURSELF)));
    }

    // Прежний буфер возвращаем на место, если так велят настройки:
    // пользователь не должен терять то, что копировал до диктовки. Но
    // кому-то удобнее, чтобы надиктованное оставалось под ⌘V.
    let previous = if options.restore_clipboard {
        app.clipboard().read_text().ok()
    } else {
        None
    };

    app.clipboard()
        .write_text(text.to_string())
        .map_err(|e| anyhow!("буфер обмена: {e}"))?;

    {
        ensure_enigo(app)?;
        let state = app
            .try_state::<EnigoState>()
            .ok_or_else(|| anyhow!("нажималка не заведена"))?;
        let mut guard = state.0.lock().map_err(|_| anyhow!("нажималка занята"))?;
        let enigo = guard
            .as_mut()
            .ok_or_else(|| anyhow!("нажималка не создалась"))?;
        std::thread::sleep(std::time::Duration::from_millis(80));
        enigo.key(PASTE_MOD, Direction::Press).map_err(|e| anyhow!("{e}"))?;
        enigo.key(Key::Other(VK_V), Direction::Click).map_err(|e| anyhow!("{e}"))?;
        enigo.key(PASTE_MOD, Direction::Release).map_err(|e| anyhow!("{e}"))?;

        // Ввод — сразу после вставки, пока фокус там же: так диктовка в
        // мессенджер сама отправляет сообщение.
        if let Some(key) = options.submit {
            std::thread::sleep(std::time::Duration::from_millis(60));
            match key {
                SubmitKey::Enter => {
                    enigo.key(Key::Return, Direction::Click).map_err(|e| anyhow!("{e}"))?;
                }
                SubmitKey::CtrlEnter => {
                    enigo.key(Key::Control, Direction::Press).map_err(|e| anyhow!("{e}"))?;
                    enigo.key(Key::Return, Direction::Click).map_err(|e| anyhow!("{e}"))?;
                    enigo.key(Key::Control, Direction::Release).map_err(|e| anyhow!("{e}"))?;
                }
                SubmitKey::CmdEnter => {
                    enigo.key(PASTE_MOD, Direction::Press).map_err(|e| anyhow!("{e}"))?;
                    enigo.key(Key::Return, Direction::Click).map_err(|e| anyhow!("{e}"))?;
                    enigo.key(PASTE_MOD, Direction::Release).map_err(|e| anyhow!("{e}"))?;
                }
            }
        }
    }

    // Вставка забирает буфер асинхронно — вернуть прежний можно только
    // после того, как нажатие долетело.
    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Some(previous) = previous {
        let _ = app.clipboard().write_text(previous);
    }
    Ok(())
}
