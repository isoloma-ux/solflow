//! Пилюля-оверлей на время записи — версия для систем без NSPanel.
//! Обычное окно без рамки: поверх всех, мимо панели задач, прозрачное и
//! не берущее мышь. Задача та же, что у панели на macOS, — показаться, не
//! отобрав фокус у приложения, в которое человек диктует.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WIDTH: f64 = 260.0;
const HEIGHT: f64 = 52.0;
const BOTTOM_OFFSET: f64 = 14.0;
/// Сверху нет меню-бара, поэтому отступ такой же, как снизу.
const TOP_OFFSET: f64 = 14.0;

pub fn create(app: &AppHandle) {
    let (x, y) = match position(app) {
        Some(p) => p,
        None => return,
    };
    let window = WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
        .title("Sol Flow")
        .inner_size(WIDTH, HEIGHT)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build();

    match window {
        // Клики сквозь пилюлю: окно висит поверх чужих и не должно
        // перехватывать мышь на своей полоске экрана.
        Ok(window) => {
            let _ = window.set_ignore_cursor_events(true);
        }
        Err(e) => log::error!("оверлей не создался: {e}"),
    }
}

/// По центру основного монитора — снизу или сверху, как выбрано в
/// настройках.
fn position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);

    let top = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .overlay_position
        == "top";

    let y = if top {
        origin.y + TOP_OFFSET
    } else {
        origin.y + size.height - HEIGHT - BOTTOM_OFFSET
    };
    Some((origin.x + (size.width - WIDTH) / 2.0, y))
}

/// Сколько ждать, пока пилюля свернётся: столько же, сколько длится
/// переход в hud.html, плюс кадр на всякий случай.
const CLOSE_MS: u64 = 300;

pub fn show(app: &AppHandle) {
    // Оверлей можно выключить совсем: кому-то он мешает поверх чужих окон.
    let style = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .overlay_style
        .clone();
    if style == "none" {
        return;
    }

    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let window = match handle.get_webview_window("hud") {
            Some(w) => w,
            None => return,
        };
        // Экран мог смениться — позиция пересчитывается на каждый показ.
        if let Some((x, y)) = position(&handle) {
            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                x,
                y,
            }));
        }
        let _ = window.show();
        // Окно уже на экране и прозрачно — пилюля раскрывается внутри.
        let _ = handle.emit("solflow-hud-style", style.clone());
        let _ = handle.emit("solflow-hud-open", ());
    });
}

/// Сначала просим пилюлю свернуться, окно убираем после анимации: иначе
/// оно исчезает рывком.
pub fn hide(app: &AppHandle) {
    let _ = app.emit("solflow-hud-close", ());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(CLOSE_MS));
        let inner = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(window) = inner.get_webview_window("hud") {
                let _ = window.hide();
            }
        });
    });
}
