//! Desktop integration. The MCP process only sees the managed export.
use serde_json::{json, Value};
use solflow_mcp_core::{access::{Policy, FILE}, collect, Grant, Store};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct State {
    gate: Mutex<()>,
    dirty: AtomicBool,
}
fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("mcp-export"))
        .map_err(|e| e.to_string())
}
fn policy_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(failure)?.join(FILE))
}
// Caller holds State.gate, shared with local permission edits.
fn policy_locked(app: &AppHandle) -> Result<Policy, String> {
    let path = policy_path(app)?;
    let mut policy = Policy::load(&path).map_err(failure)?;
    if !path.exists() {
        let export = root(app)?;
        if export.join("export.sqlite3").exists() {
            for (id, grant) in Store::reader(&export).and_then(|s| s.grants()).map_err(failure)? {
                policy.set(&id, grant).map_err(failure)?;
            }
        }
        policy.save(&path).map_err(failure)?;
    }
    Ok(policy)
}
fn apply_locked(app: &AppHandle, policy: &Policy) -> Result<(), String> {
    if policy.projects.is_empty() { return Ok(()) }
    let mut store = Store::create(&root(app)?).map_err(failure)?;
    let existing = store.grants().map_err(failure)?;
    let alive = crate::meetings::projects(app);
    for (id, entry) in &policy.projects {
        let grant = if alive.iter().any(|p| p.id == *id) { entry.grant.clone() } else { Grant::default() };
        if existing.get(id) != Some(&grant) { store.set_grant(id, &grant).map_err(failure)?; }
    }
    Ok(())
}
pub fn merge_synced_access(app: &AppHandle, remote: &Policy) -> Result<Policy, String> {
    let state = app.state::<State>();
    let _lock = state.gate.lock().map_err(failure)?;
    let local = policy_locked(app)?;
    let merged = local.merge(remote);
    merged.save(&policy_path(app)?).map_err(failure)?;
    apply_locked(app, &merged)?;
    state.dirty.store(true, Ordering::SeqCst);
    Ok(merged)
}
fn failure(e: impl std::fmt::Display) -> String {
    log::error!("MCP export: {e}");
    "Не удалось обновить доступ MCP. Повторите попытку.".into()
}
fn show_source_window(app: &AppHandle, urls: &[String]) {
    if urls.iter().any(|u| u.starts_with("solflow://recording/")) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

pub fn init(app: &AppHandle) {
    app.manage(State::default());
    use tauri_plugin_deep_link::DeepLinkExt;
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        let urls: Vec<_> = event.urls().iter().map(ToString::to_string).collect();
        show_source_window(&handle, &urls);
    });
    // No export directory or record reads until someone explicitly grants access.
    invalidate(app);
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(750));
        let state = app.state::<State>();
        if !state.dirty.swap(false, Ordering::SeqCst) {
            continue;
        }
        if let Err(e) = refresh(&app) {
            log::warn!("MCP refresh: {e}");
            let _ = app.emit("solflow-mcp", json!({"error":true}));
        }
    });
}

pub fn invalidate(app: &AppHandle) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let Ok(root) = root(app) else { return };
    if !root.join("export.sqlite3").exists() {
        return;
    }
    let Ok(_lock) = state.gate.lock() else { return };
    match Store::create(&root).and_then(|mut s| s.invalidate()) {
        Ok(()) => state.dirty.store(true, Ordering::SeqCst),
        Err(e) => log::error!("MCP invalidation failed: {e}"),
    }
}
fn refresh(app: &AppHandle) -> Result<Value, String> {
    let state = app.state::<State>();
    let _lock = state.gate.lock().map_err(failure)?;
    let policy = policy_locked(app)?;
    apply_locked(app, &policy)?;
    let mut store = Store::create(&root(app)?).map_err(failure)?;
    store.invalidate().map_err(failure)?;
    let source = app.path().app_data_dir().map_err(failure)?;
    let snapshot = collect(&source, &store.grants().map_err(failure)?).map_err(failure)?;
    store.publish(&snapshot).map_err(failure)?;
    let status = store.status().map_err(failure)?;
    let _ = app.emit("solflow-mcp", &status);
    Ok(status)
}
#[tauri::command]
pub fn mcp_status(app: AppHandle) -> Result<Value, String> {
    let path = root(&app)?;
    if !path.join("export.sqlite3").exists() {
        return Ok(json!({"grants":{},"documents":0,"updated_at":0,"stale":true}));
    }
    Store::reader(&path)
        .and_then(|s| s.status())
        .map_err(failure)
}
#[tauri::command]
pub async fn mcp_set_access(
    app: AppHandle,
    project_id: String,
    grant: Grant,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !crate::meetings::projects(&app)
            .iter()
            .any(|p| p.id == project_id)
        {
            return Err("Проект недоступен".into());
        }
        {
            let state = app.state::<State>();
            let _lock = state.gate.lock().map_err(failure)?;
            let mut policy = policy_locked(&app)?;
            policy.set(&project_id, grant).map_err(failure)?;
            policy.save(&policy_path(&app)?).map_err(failure)?;
            apply_locked(&app, &policy)?;
        }
        crate::sync::touch(&app);
        // Permissions have committed. Report their actual state even if another
        // project's source is broken; never display a successful revoke as failed.
        match refresh(&app) {
            Ok(status) => Ok(status),
            Err(_) => mcp_status(app),
        }
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub async fn mcp_refresh(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || refresh(&app))
        .await
        .map_err(failure)?
}
#[tauri::command]
pub async fn mcp_text_copy(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<State>();
        let _lock = state.gate.lock().map_err(failure)?;
        Store::reader(&root(&app)?)
            .and_then(|s| s.text_copy())
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(failure)
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub fn mcp_claude_config(app: AppHandle) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(failure)?;
    let name = if cfg!(windows) {
        "solflow-mcp.exe"
    } else {
        "solflow-mcp"
    };
    let mut binary = exe
        .parent()
        .ok_or_else(|| failure("missing executable directory"))?
        .join(name);
    if cfg!(debug_assertions) && !binary.is_file() {
        binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../mcp-server/target/debug")
            .join(name);
    }
    if !binary.is_file() {
        return Err("Компонент MCP еще не собран для этой установки.".into());
    }
    let binary = binary.canonicalize().map_err(failure)?;
    serde_json::to_string_pretty(
        &json!({"mcpServers":{"solflow":{"command":binary,"args":["--export-dir",root(&app)?]}}}),
    )
    .map_err(failure)
}

#[tauri::command]
pub fn mcp_start_links(app: AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_deep_link::DeepLinkExt;
    let urls: Vec<String> = app
        .deep_link()
        .get_current()
        .map(|urls| {
            urls.unwrap_or_default()
                .into_iter()
                .map(|u| u.to_string())
                .collect()
        })
        .map_err(failure)?;
    show_source_window(&app, &urls);
    Ok(urls)
}
