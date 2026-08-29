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

/// Формат строки прогресса. Своя метка в начале — чтобы не спутать со
/// всем остальным, что загрузчик печатает.
const PROGRESS_TEMPLATE: &str =
    "solflow %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s";

/// «solflow 1048576 NA 734003200» → (скачано, всего). Неизвестные поля
/// загрузчик печатает как NA — тогда ноль, и окно показывает мегабайты без
/// процентов.
fn parse_progress(line: &str) -> Option<(u64, u64)> {
    let rest = line.trim().strip_prefix("solflow ")?;
    let mut parts = rest.split_whitespace();
    let number = |value: Option<&str>| -> u64 {
        value
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(0.0) as u64
    };
    let done = number(parts.next());
    let exact = number(parts.next());
    let estimate = number(parts.next());
    Some((done, if exact > 0 { exact } else { estimate }))
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
            title = crate::sys::command(&tool)
                // Вывод идёт в кодировке консоли, а на русской Windows это
                // не UTF-8: названия приезжали ромбиками. Переменной среды
                // мало — у загрузчика свой ключ, он важнее.
                .env("PYTHONIOENCODING", "utf-8")
                .args(["--encoding", "utf-8"])
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
        let total = crate::sys::command(&tool)
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

        let mut child = crate::sys::command(&tool)
            .args([
                "--no-warnings",
                "--no-playlist",
                // Прогресс спрашиваем у самого загрузчика: считать по файлам
                // на диске нечестно — он пишет во временные куски, и полоска
                // стояла на нуле до самого конца.
                "--newline",
                "--progress-template",
                PROGRESS_TEMPLATE,
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio/best[ext=mp4]/best",
                "-o",
            ])
            .arg(&template)
            .args(extra)
            .arg(url)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Строки прогресса читает отдельный поток: основной должен успевать
        // проверять отмену.
        let seen = std::sync::Arc::new(std::sync::Mutex::new((0u64, 0u64)));
        let stdout = child.stdout.take();
        let reader_seen = seen.clone();
        let reader = std::thread::spawn(move || {
            use std::io::BufRead;
            let Some(stdout) = stdout else { return };
            for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(values) = parse_progress(&line) {
                    *reader_seen.lock().unwrap() = values;
                }
            }
        });

        let status = loop {
            if (progress.cancelled)() {
                let _ = child.kill();
                let _ = reader.join();
                clean_downloads(dir);
                return Err(anyhow!("отменено"));
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            // Пока загрузчик молчит, показываем то, что уже легло на диск.
            let (done, said_total) = *seen.lock().unwrap();
            let done = if done > 0 { done } else { downloaded_bytes(dir) };
            (progress.report)(done, if said_total > 0 { said_total } else { total });
            std::thread::sleep(std::time::Duration::from_millis(400));
        };
        let _ = reader.join();

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
