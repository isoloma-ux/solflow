//! Пилюля-оверлей внизу экрана на время записи. NSPanel, а не окно:
//! панель не забирает фокус у приложения, в которое пользователь диктует, —
//! приём из десктопного Handy (nonactivating panel, уровень Status).

use tauri::{AppHandle, Emitter, Manager, WebviewUrl};
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

const WIDTH: f64 = 260.0;
const HEIGHT: f64 = 52.0;
const BOTTOM_OFFSET: f64 = 14.0;
/// Сверху пилюля не должна лезть под меню-бар.
const TOP_OFFSET: f64 = 34.0;

tauri_panel! {
    panel!(HudPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

pub fn create(app: &AppHandle) {
    let (x, y) = match position(app) {
        Some(p) => p,
        None => return,
    };
    let result = PanelBuilder::<_, HudPanel>::new(app, "hud")
        .url(WebviewUrl::App("hud.html".into()))
        .title("Sol Flow")
        .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
        .level(PanelLevel::Status)
        .size(tauri::Size::Logical(tauri::LogicalSize {
            width: WIDTH,
            height: HEIGHT,
        }))
        .has_shadow(false)
        .transparent(true)
        .no_activate(true)
        .corner_radius(0.0)
        .style_mask(StyleMask::empty().borderless().nonactivating_panel())
        .with_window(|w| w.decorations(false).transparent(true).focusable(false))
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary(),
        )
        .build();

    match result {
        Ok(panel) => panel.hide(),
        Err(e) => log::error!("оверлей не создался: {e}"),
    }
}

/// По центру основного монитора — снизу или сверху, как выбрано в
/// настройках. Меню-бар сверху занимает свои пункты, поэтому отступ там
/// заметно больше.
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
        if let Ok(panel) = handle.get_webview_panel("hud") {
            // Экран мог смениться — позиция пересчитывается на каждый показ.
            if let Some((x, y)) = position(&handle) {
                if let Some(window) = handle.get_webview_window("hud") {
                    let _ = window.set_position(tauri::Position::Logical(
                        tauri::LogicalPosition { x, y },
                    ));
                }
            }
            panel.show();
            // Панель уже на экране и прозрачна — пилюля раскрывается внутри.
            let _ = handle.emit("solflow-hud-style", style.clone());
            let _ = handle.emit("solflow-hud-open", ());
        }
    });
}

/// Сначала просим пилюлю свернуться, панель убираем после анимации: иначе
/// она исчезает рывком вместе с окном.
pub fn hide(app: &AppHandle) {
    let _ = app.emit("solflow-hud-close", ());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(CLOSE_MS));
        let inner = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Ok(panel) = inner.get_webview_panel("hud") {
                panel.hide();
            }
        });
    });
}

use tauri_nspanel::ManagerExt;
