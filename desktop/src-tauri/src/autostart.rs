//! Запуск вместе с системой.
//!
//! На macOS — LaunchAgent, plist в ~/Library/LaunchAgents: это работает для
//! приложения, лежащего где угодно, и не требует ни установки в
//! /Applications, ни разрешений. Запускаем сам бандл через `open -a`, а не
//! бинарь напрямую: иначе macOS считает процесс безымянным и не отдаёт ему
//! права, выданные приложению.
//!
//! На Windows — ключ в HKCU\...\CurrentVersion\Run: ярлык в папке
//! «Автозагрузка» пришлось бы собирать через COM, а реестр правится обычной
//! командой и живёт в профиле пользователя, без прав администратора.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Result};

const LABEL: &str = "ru.ivansolomin.solflow";

#[cfg(target_os = "macos")]
fn plist_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

/// Путь к .app, внутри которого лежит текущий бинарь.
#[cfg(target_os = "macos")]
fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // .../Sol Flow.app/Contents/MacOS/solflow → .../Sol Flow.app
    let bundle = exe.parent()?.parent()?.parent()?;
    if bundle.extension().map(|e| e == "app").unwrap_or(false) {
        Some(bundle.to_path_buf())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
pub fn enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn set(enabled: bool) -> Result<()> {
    let path = plist_path().ok_or_else(|| anyhow!("нет домашней папки"))?;

    if !enabled {
        // Сначала снимаем с учёта, потом убираем файл: иначе агент
        // остаётся зарегистрированным до перезагрузки.
        let _ = Command::new("/bin/launchctl")
            .args(["bootout", &format!("gui/{}/{LABEL}", uid())])
            .output();
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }

    let bundle = bundle_path()
        .ok_or_else(|| anyhow!("приложение запущено не из бандла — автозапуск не выйдет"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>-a</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        bundle.display()
    );
    std::fs::write(&path, plist)?;

    // bootout на всякий случай: повторная регистрация того же ярлыка
    // ругается, если он уже на учёте.
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{}/{LABEL}", uid())])
        .output();
    let out = Command::new("/bin/launchctl")
        .args(["bootstrap", &format!("gui/{}", uid())])
        .arg(&path)
        .output()?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(anyhow!(
            "система не приняла автозапуск: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn uid() -> String {
    Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "501".to_string())
}

// --- Windows ---------------------------------------------------------------

/// Ветка реестра, откуда Windows запускает программы при входе в систему.
#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Под каким именем мы там записаны.
#[cfg(windows)]
const RUN_NAME: &str = "Sol Flow";

/// Ответ реестра держим в памяти: окно перечитывает настройки после каждого
/// переключателя, а каждый запуск `reg` — это отдельный процесс, и на них
/// заметно подтормаживал весь экран настроек.
#[cfg(windows)]
fn cached() -> &'static std::sync::atomic::AtomicBool {
    use std::sync::atomic::AtomicBool;
    use std::sync::OnceLock;

    static CACHE: OnceLock<AtomicBool> = OnceLock::new();
    CACHE.get_or_init(|| AtomicBool::new(ask_registry()))
}

#[cfg(windows)]
fn ask_registry() -> bool {
    Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
pub fn enabled() -> bool {
    cached().load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<()> {
    if !enabled {
        let _ = Command::new("reg")
            .args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"])
            .output();
        cached().store(false, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    // Кавычки — часть значения: без них Windows спотыкается о пробел в
    // «Sol Flow.exe» и ищет программу «Sol».
    let value = format!("\"{}\"", exe.display());
    let out = Command::new("reg")
        .args(["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &value, "/f"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "система не приняла автозапуск: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    cached().store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
