//! Внешние помощники — yt-dlp и ffmpeg. Ни того, ни другого в системе по
//! умолчанию нет, поэтому приложение умеет поставить их само, в свою папку
//! данных, и предлагает это в настройках.
//!
//! Что кому нужно, различается по системам. На macOS звук и видео приводит
//! к нужному виду встроенный afconvert (см. meetings.rs), а yt-dlp ставится
//! пользовательским pip — так было с первого дня и работает. На Windows
//! встроенного конвертера нет вовсе, поэтому нужен ffmpeg, а yt-dlp
//! качается готовым .exe: python в системе может и не оказаться.

use std::path::PathBuf;
#[cfg(windows)]
use std::sync::OnceLock;

use anyhow::Result;

/// Куда складываются скачанные помощники. Ставится один раз при запуске:
/// таскать AppHandle через всю загрузку по ссылке было бы шумно.
#[cfg(windows)]
static BIN_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(windows)]
pub fn init(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = BIN_DIR.set(dir.join("bin"));
    }
}

/// На macOS помощник ставится системным pip и лежит в своих местах —
/// запоминать нечего.
#[cfg(not(windows))]
pub fn init(_app: &tauri::AppHandle) {}

#[cfg(windows)]
fn bin_dir() -> PathBuf {
    BIN_DIR
        .get()
        .cloned()
        .unwrap_or_else(std::env::temp_dir)
}

/// Есть ли чем привести файл к нужному звуку. На macOS это делают
/// встроенные утилиты, ставить нечего.
pub fn converter_ready() -> bool {
    cfg!(target_os = "macos") || ffmpeg().is_some()
}

/// Всё ли на месте, чтобы качать встречи по ссылке.
pub fn ready() -> bool {
    ytdlp().is_some() && (cfg!(target_os = "macos") || ffmpeg().is_some())
}

// --- macOS -----------------------------------------------------------------

/// Где искать yt-dlp: pip кладёт его в пользовательский bin, homebrew — в
/// свой. PATH у приложения из Finder куцый, поэтому перебираем руками.
#[cfg(target_os = "macos")]
pub fn ytdlp() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/yt-dlp"),
        PathBuf::from("/usr/local/bin/yt-dlp"),
        PathBuf::from(format!("{home}/.local/bin/yt-dlp")),
    ];
    // ~/Library/Python/3.x/bin — сюда ставит системный pip.
    if let Ok(dirs) = std::fs::read_dir(format!("{home}/Library/Python")) {
        for dir in dirs.filter_map(|d| d.ok()) {
            candidates.push(dir.path().join("bin/yt-dlp"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

/// На macOS звук приводит к нужному виду afconvert — ffmpeg не нужен.
#[cfg(target_os = "macos")]
pub fn ffmpeg() -> Option<PathBuf> {
    None
}

/// Ставит yt-dlp пользовательским pip — без sudo и не трогая систему.
#[cfg(target_os = "macos")]
pub fn install(_on_progress: &dyn Fn(u8)) -> Result<()> {
    use anyhow::anyhow;

    let out = std::process::Command::new("/usr/bin/python3")
        .args(["-m", "pip", "install", "--user", "--upgrade", "yt-dlp"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "pip не справился: {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("неизвестная ошибка")
        ));
    }
    if ytdlp().is_none() {
        return Err(anyhow!("yt-dlp поставился, но не нашёлся"));
    }
    Ok(())
}

// --- Windows ---------------------------------------------------------------

#[cfg(windows)]
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

/// Сборки ffmpeg от самого проекта yt-dlp: они и обновляются вместе с ним,
/// и лежат на GitHub, куда мы и так ходим за моделями.
#[cfg(windows)]
const FFMPEG_URL: &str =
    "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip";

#[cfg(windows)]
pub fn ytdlp() -> Option<PathBuf> {
    let path = bin_dir().join("yt-dlp.exe");
    path.exists().then_some(path)
}

#[cfg(windows)]
pub fn ffmpeg() -> Option<PathBuf> {
    let path = bin_dir().join("ffmpeg.exe");
    path.exists().then_some(path)
}

/// Качает оба помощника в свою папку. Проценты — общие на обе загрузки:
/// ffmpeg весит на порядок больше, поэтому ему отданы почти все.
#[cfg(windows)]
pub fn install(on_progress: &dyn Fn(u8)) -> Result<()> {
    use anyhow::anyhow;

    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    let never = || false;

    if ytdlp().is_none() {
        let target = dir.join("yt-dlp.exe");
        crate::net::download(YTDLP_URL, &target, &|_, _| on_progress(5), &never)?;
    }
    on_progress(10);

    if ffmpeg().is_none() {
        let archive = dir.join("ffmpeg.zip");
        let unpacked = dir.join("ffmpeg-unpacked");
        let _ = std::fs::remove_dir_all(&unpacked);

        crate::net::download(
            FFMPEG_URL,
            &archive,
            &|done, total| {
                let share = if total > 0 { done * 85 / total } else { 0 };
                on_progress(10 + share.min(85) as u8);
            },
            &never,
        )?;

        // Распаковка средствами системы: свой распаковщик ради одного архива
        // тянуть незачем.
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive.display(),
                unpacked.display()
            ))
            .output()?;
        let _ = std::fs::remove_file(&archive);
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&unpacked);
            return Err(anyhow!("архив ffmpeg не распаковался"));
        }

        // Внутри архива ffmpeg.exe лежит в подпапке bin — ищем его там,
        // не полагаясь на имя папки с версией.
        let found = find_file(&unpacked, "ffmpeg.exe")
            .ok_or_else(|| anyhow!("в архиве не нашлось ffmpeg.exe"))?;
        std::fs::rename(&found, dir.join("ffmpeg.exe"))?;
        let _ = std::fs::remove_dir_all(&unpacked);
    }

    on_progress(100);
    if !ready() {
        return Err(anyhow!("помощники скачались, но не нашлись"));
    }
    Ok(())
}

#[cfg(windows)]
fn find_file(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().map(|f| f == name).unwrap_or(false) {
            return Some(path);
        }
    }
    None
}

/// Докачивает ffmpeg, если его ещё нет: импорт файла не должен упираться в
/// поход в настройки — на macOS он просто работает, и на Windows должен
/// вести себя так же.
#[cfg(windows)]
pub fn ensure_ffmpeg(on_progress: &dyn Fn(u8)) -> Result<()> {
    if ffmpeg().is_some() {
        return Ok(());
    }
    install(on_progress)
}

