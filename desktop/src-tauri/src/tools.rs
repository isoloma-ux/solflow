//! Managed media helpers. Updates are staged and verified before replacing
//! the working executable. Browser cookies are never read during installation.
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static BIN_DIR: OnceLock<PathBuf> = OnceLock::new();
static INSTALL: Mutex<()> = Mutex::new(());
pub fn init(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = BIN_DIR.set(dir.join("bin"));
    }
}
fn bin_dir() -> PathBuf {
    BIN_DIR.get().cloned().unwrap_or_else(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        home.join("Library/Application Support/ru.ivansolomin.solflow/bin")
    })
}
fn executable(name: &str) -> PathBuf {
    bin_dir().join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    })
}
fn managed(name: &str) -> Option<PathBuf> {
    let p = executable(name);
    p.is_file().then_some(p)
}
pub fn ytdlp() -> Option<PathBuf> {
    if let Some(p) = managed("yt-dlp") {
        return Some(p);
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut paths = vec![
            PathBuf::from("/opt/homebrew/bin/yt-dlp"),
            PathBuf::from("/usr/local/bin/yt-dlp"),
            PathBuf::from(format!("{home}/.local/bin/yt-dlp")),
        ];
        if let Ok(dirs) = std::fs::read_dir(format!("{home}/Library/Python")) {
            for d in dirs.flatten() {
                paths.push(d.path().join("bin/yt-dlp"));
            }
        }
        return paths.into_iter().find(|p| p.is_file());
    }
    #[cfg(not(target_os = "macos"))]
    None
}
pub fn ffmpeg() -> Option<PathBuf> {
    managed("ffmpeg")
}
pub fn deno() -> Option<PathBuf> {
    managed("deno")
}
pub fn converter_ready() -> bool {
    cfg!(target_os = "macos") || ffmpeg().is_some()
}
pub fn ready() -> bool {
    managed("yt-dlp").is_some() && ffmpeg().is_some() && deno().is_some()
}

fn verify(path: &Path, expected: &str) -> Result<()> {
    let sum = format!("{:x}", Sha256::digest(std::fs::read(path)?));
    if sum != expected.trim_start_matches("sha256:") {
        return Err(anyhow!(
            "Не совпала контрольная сумма загрузчика. Повторите обновление."
        ));
    }
    Ok(())
}
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
fn activate(staged: &Path, target: &Path, version_arg: &str) -> Result<()> {
    make_executable(staged)?;
    if !crate::sys::command(staged)
        .arg(version_arg)
        .output()?
        .status
        .success()
    {
        return Err(anyhow!(
            "Компонент загрузки не запускается. Повторите обновление."
        ));
    }
    // Windows cannot rename over an existing executable. Keep a rollback copy.
    let backup = target.with_extension("previous");
    let _ = std::fs::remove_file(&backup);
    if target.exists() {
        std::fs::rename(target, &backup)?;
    }
    if let Err(e) = std::fs::rename(staged, target) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, target);
        }
        return Err(e.into());
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}
fn release_download(
    repo: &str,
    name: &str,
    target: &Path,
    report: &dyn Fn(u64, u64),
) -> Result<()> {
    let release = crate::net::get_json(&format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))?;
    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(name)))
        .ok_or_else(|| anyhow!("Компонент загрузки не найден на сервере."))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow!("Нет адреса компонента"))?;
    if !url.starts_with(&format!("https://github.com/{repo}/releases/download/")) {
        return Err(anyhow!("Недопустимый адрес компонента"));
    }
    let digest = asset["digest"]
        .as_str()
        .filter(|s| s.starts_with("sha256:"))
        .ok_or_else(|| anyhow!("Нет контрольной суммы компонента"))?;
    crate::net::download(url, target, report, &|| false)?;
    verify(target, digest)
}
fn unzip(archive: &Path, dir: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let out = crate::sys::command("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(archive)
        .arg(dir)
        .output()?;
    #[cfg(windows)]
    let out = crate::sys::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            archive.display().to_string().replace('\'', "''"),
            dir.display().to_string().replace('\'', "''")
        ))
        .output()?;
    if !out.status.success() {
        return Err(anyhow!("Не удалось распаковать компонент загрузки."));
    }
    Ok(())
}
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().map(|f| f == name).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}
fn ffmpeg_install(report: &dyn Fn(u8)) -> Result<()> {
    if ffmpeg().is_some() {
        return Ok(());
    }
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    let staged = dir.join(if cfg!(windows) {
        "ffmpeg-new.exe"
    } else {
        "ffmpeg-new"
    });
    #[cfg(target_os = "macos")]
    {
        // ImageIO's standalone arm64 FFmpeg 7.1; source and license in docs/media-import.md.
        let url="https://raw.githubusercontent.com/imageio/imageio-binaries/f8f64710ea88e7e4a352c0f7d8c0deac9f5fd685/ffmpeg/ffmpeg-macos-aarch64-v7.1";
        crate::net::download(
            url,
            &staged,
            &|d, t| {
                report(if t > 0 {
                    (d * 100 / t).min(99) as u8
                } else {
                    0
                })
            },
            &|| false,
        )?;
        verify(
            &staged,
            "6d175a4743ca50256e89a8cdd731100f9cee33bd79aeea46894d209410dc6617",
        )?;
    }
    #[cfg(windows)]
    {
        let archive = dir.join("ffmpeg.zip");
        let unpacked = dir.join("ffmpeg-unpacked");
        let _ = std::fs::remove_dir_all(&unpacked);
        release_download(
            "yt-dlp/FFmpeg-Builds",
            "ffmpeg-master-latest-win64-gpl.zip",
            &archive,
            &|d, t| {
                report(if t > 0 {
                    (d * 100 / t).min(99) as u8
                } else {
                    0
                })
            },
        )?;
        unzip(&archive, &unpacked)?;
        let found =
            find_file(&unpacked, "ffmpeg.exe").ok_or_else(|| anyhow!("В архиве нет ffmpeg"))?;
        std::fs::copy(found, &staged)?;
        let _ = std::fs::remove_dir_all(unpacked);
        let _ = std::fs::remove_file(archive);
    }
    activate(&staged, &executable("ffmpeg"), "-version")?;
    report(100);
    Ok(())
}
pub fn ensure_ffmpeg(report: &dyn Fn(u8)) -> Result<()> {
    let _guard = INSTALL
        .lock()
        .map_err(|_| anyhow!("Не удалось запустить установку"))?;
    ffmpeg_install(report)
}
pub fn install(report: &dyn Fn(u8)) -> Result<()> {
    let _guard = INSTALL
        .try_lock()
        .map_err(|_| anyhow!("Дождитесь завершения установки загрузчика."))?;
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    ffmpeg_install(&|p| report(p / 2))?;
    #[cfg(target_os = "macos")]
    let (yt, deno_zip) = ("yt-dlp_macos", "deno-aarch64-apple-darwin.zip");
    #[cfg(windows)]
    let (yt, deno_zip) = ("yt-dlp.exe", "deno-x86_64-pc-windows-msvc.zip");
    let archive = dir.join("deno.zip");
    let unpacked = dir.join("deno-unpacked");
    let _ = std::fs::remove_dir_all(&unpacked);
    release_download("denoland/deno", deno_zip, &archive, &|d, t| {
        report(50 + if t > 0 { (d * 25 / t).min(25) as u8 } else { 0 })
    })?;
    unzip(&archive, &unpacked)?;
    let name = if cfg!(windows) { "deno.exe" } else { "deno" };
    let found = find_file(&unpacked, name).ok_or_else(|| anyhow!("В архиве нет Deno"))?;
    activate(&found, &executable("deno"), "--version")?;
    let _ = std::fs::remove_dir_all(unpacked);
    let _ = std::fs::remove_file(archive);
    let staged = dir.join(if cfg!(windows) {
        "yt-dlp-new.exe"
    } else {
        "yt-dlp-new"
    });
    release_download("yt-dlp/yt-dlp", yt, &staged, &|d, t| {
        report(75 + if t > 0 { (d * 24 / t).min(24) as u8 } else { 0 })
    })?;
    activate(&staged, &executable("yt-dlp"), "--version")?;
    report(100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incorrect_download_never_passes_checksum() {
        let p = std::env::temp_dir().join(format!("sf-checksum-{}", std::process::id()));
        std::fs::write(&p, b"test").unwrap();
        assert!(verify(&p, "bad").is_err());
        assert!(verify(
            &p,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        )
        .is_ok());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn failed_helper_keeps_existing_version() {
        let d = std::env::temp_dir().join(format!("sf-activate-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let staged = d.join("new");
        let old = d.join("installed");
        std::fs::write(&old, b"old working version").unwrap();
        std::fs::write(&staged, b"#!/bin/sh\nexit 1\n").unwrap();
        assert!(activate(&staged, &old, "--version").is_err());
        assert_eq!(std::fs::read(&old).unwrap(), b"old working version");
        std::fs::remove_dir_all(d).unwrap();
    }
}
