//! Загрузка встречи по ссылке. Три случая, от простого к сложному:
//!
//! 1. Публичная ссылка Яндекс.Диска — у него есть открытый API, который
//!    отдаёт прямой адрес файла без ключей и авторизации.
//! 2. Прямая ссылка на медиафайл — качаем как есть.
//! 3. Страница с видео (YouTube, VK и прочие) — нужен yt-dlp. Приложение
//!    умеет поставить его само (см. tools), а не просто разводить руками.
//!
//! Скачиваем только звуковую дорожку: видео весит в разы больше, а
//! расшифровке нужен один моно-канал.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};

/// Загрузка прямой ссылки с отчётом о прогрессе.
fn download_to(url: &str, target: &Path, progress: &Progress) -> Result<()> {
    crate::net::download(url, target, progress.report, progress.cancelled)?;
    if target.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::remove_file(target);
        return Err(anyhow!("файл по ссылке не скачался"));
    }
    Ok(())
}

/// Как сообщать о ходе загрузки и как узнать про отмену.
pub struct Progress<'a> {
    /// Скачано и всего байт; ноль во втором — размер неизвестен.
    pub report: &'a dyn Fn(u64, u64),
    pub cancelled: &'a dyn Fn() -> bool,
}

/// Публичная ссылка Яндекс.Диска → прямой адрес файла и его имя.
fn yandex_direct(url: &str) -> Option<(String, String)> {
    let api = format!(
        "https://cloud-api.yandex.net/v1/disk/public/resources/download?public_key={}",
        urlencode(url)
    );
    let body = crate::net::get_json(&api).ok()?;
    let href = body.get("href")?.as_str()?.to_string();

    // Имя файла берём из соседнего вызова: в ссылке на скачивание его нет.
    let meta_api = format!(
        "https://cloud-api.yandex.net/v1/disk/public/resources?public_key={}",
        urlencode(url)
    );
    let name = crate::net::get_json(&meta_api)
        .ok()
        .and_then(|v| v.get("name")?.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Запись с Яндекс.Диска".to_string());
    Some((href, name))
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Похоже ли на прямую ссылку на медиа — по расширению в пути.
fn looks_like_media(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
    [
        ".mp3", ".m4a", ".wav", ".aac", ".aiff", ".aif", ".caf", ".mp4", ".mov", ".m4v",
        ".mkv", ".webm", ".ogg", ".opus", ".flac", ".wma", ".avi",
    ]
    .iter()
    .any(|ext| path.ends_with(ext))
}

/// Тип содержимого по HEAD-запросу — для ссылок без расширения.
fn content_type(url: &str) -> String {
    crate::net::content_type(url)
}

/// Сколько уже на диске: загрузчик пишет во временные .part и .ytdl.
fn downloaded_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("download")
        })
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn clean_downloads(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().filter_map(|e| e.ok()) {
        if entry.file_name().to_string_lossy().starts_with("download") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Скачивает звук по ссылке в [dir]. Возвращает путь к файлу и название,
/// которое станет именем встречи.
pub fn fetch(url: &str, dir: &Path, progress: &Progress) -> Result<(PathBuf, String)> {
    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("нужна ссылка, начинающаяся с http"));
    }

    // 1. Яндекс.Диск отдаёт прямой адрес по своему открытому API.
    let host = url.split('/').nth(2).unwrap_or("").to_lowercase();
    if host.contains("disk.yandex") || host.contains("yadi.sk") {
        if let Some((direct, name)) = yandex_direct(url) {
            let target = dir.join("download");
            download_to(&direct, &target, progress)?;
            let title = Path::new(&name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(name);
            return Ok((target, title));
        }
    }

    // 2. Прямая ссылка на файл.
    let media_type = if looks_like_media(url) {
        true
    } else {
        let ct = content_type(url);
        ct.starts_with("audio/") || ct.starts_with("video/")
    };
    if media_type {
        let target = dir.join("download");
        download_to(url, &target, progress)?;
        let name = url
            .split(['?', '#'])
            .next()
            .unwrap_or(url)
            .rsplit('/')
            .next()
            .unwrap_or("Запись по ссылке");
        let title = Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Запись по ссылке".to_string());
        return Ok((target, title));
    }

    // 3. Страница с видео — работа для yt-dlp.
    let tool = crate::tools::ytdlp().ok_or_else(|| {
        anyhow!("для этой ссылки нужен загрузчик — поставьте его в настройках")
    })?;

    // YouTube по-разному отвечает разным клиентам: обычный веб-запрос
    // часто упирается в «страницу нужно перезагрузить», а мобильный
    // проходит. Перебираем, пока какой-нибудь не отдаст файл.
    let clients: [&[&str]; 4] = [
        &[],
        &["--extractor-args", "youtube:player_client=android"],
        &["--extractor-args", "youtube:player_client=ios"],
        &["--extractor-args", "youtube:player_client=tv"],
    ];

    // Просим готовую дорожку одним файлом: слияние потоков потребовало бы
    // ffmpeg, которого в системе нет.
    let template = dir.join("download.%(ext)s");
    let mut last_error = String::from("ссылка не поддерживается");
    let mut title = String::new();

    for extra in clients {
        if title.is_empty() {
            title = Command::new(&tool)
                .args(["--no-warnings", "--skip-download", "--print", "%(title)s"])
                .args(extra)
                .arg(url)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_default();
        }

        // Общий размер спрашиваем заранее: сам загрузчик о нём молчит,
        // пока не закончит, а ждать вслепую неприятно.
        let total = Command::new(&tool)
            .args(["--no-warnings", "--skip-download", "--print", "%(filesize_approx)s"])
            .args(extra)
            .arg(url)
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v as u64)
            })
            .unwrap_or(0);

        let mut child = Command::new(&tool)
            .args([
                "--no-warnings",
                "--no-playlist",
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio/best[ext=mp4]/best",
                "-o",
            ])
            .arg(&template)
            .args(extra)
            .arg(url)
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let status = loop {
            if (progress.cancelled)() {
                let _ = child.kill();
                clean_downloads(dir);
                return Err(anyhow!("отменено"));
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            (progress.report)(downloaded_bytes(dir), total);
            std::thread::sleep(std::time::Duration::from_millis(400));
        };

        if status.success() {
            (progress.report)(downloaded_bytes(dir), total);
            break;
        }
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            use std::io::Read;
            let _ = pipe.read_to_string(&mut stderr);
        }
        last_error = stderr
            .lines()
            .filter(|l| l.contains("ERROR"))
            .last()
            .unwrap_or("ссылка не поддерживается")
            .trim()
            .to_string();
    }

    if title.is_empty() {
        title = "Запись по ссылке".to_string();
    }

    // Имя файла заранее неизвестно — расширение выбрал сам загрузчик.
    let file = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_stem().map(|s| s == "download").unwrap_or(false))
        .ok_or_else(|| anyhow!("загрузчик не смог: {last_error}"))?;
    Ok((file, title))
}
