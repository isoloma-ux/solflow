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
        ".mp3", ".m4a", ".wav", ".aac", ".aiff", ".aif", ".caf", ".mp4", ".mov", ".m4v", ".mkv",
        ".webm", ".ogg", ".opus", ".flac", ".wma", ".avi", ".ts", ".mts", ".m2ts",
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
        .filter(|e| e.file_name().to_string_lossy().starts_with("download"))
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn clean_downloads(dir: &Path) {
    for entry in std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
    {
        if entry.file_name().to_string_lossy().starts_with("download") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Скачивает звук по ссылке в [dir]. Возвращает путь к файлу и название,
/// которое станет именем встречи.
pub fn fetch(
    url: &str,
    dir: &Path,
    progress: &Progress,
    browser: Option<&str>,
) -> Result<(PathBuf, String)> {
    let url = url.trim();
    validate_browser(url, browser)?;
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
        download_to(url, &target, progress).map_err(|e| {
            if rutube_connection_failure(url, &e.to_string()) {
                anyhow!(RUTUBE_CONNECTION_HINT)
            } else {
                e
            }
        })?;
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
    let tool = crate::tools::ytdlp()
        .ok_or_else(|| anyhow!("для этой ссылки нужен загрузчик — поставьте его в настройках"))?;

    download_page(&tool, url, dir, progress, browser)
}

/// Session access is opt-in, per request, and limited to YouTube hosts.
pub fn validate_browser(url: &str, browser: Option<&str>) -> Result<()> {
    let Some(browser) = browser else {
        return Ok(());
    };
    let host = url.split('/').nth(2).unwrap_or("").to_ascii_lowercase();
    if ![
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "music.youtube.com",
        "youtu.be",
        "www.youtu.be",
    ]
    .contains(&host.as_str())
    {
        return Err(anyhow!(
            "Сессию браузера можно использовать только для ссылки YouTube."
        ));
    }
    if !["chrome", "edge", "firefox", "safari"].contains(&browser)
        || (cfg!(windows) && browser == "safari")
    {
        return Err(anyhow!("Выберите поддерживаемый браузер."));
    }
    Ok(())
}

pub const RUTUBE_CONNECTION_HINT: &str = "Не удалось подключиться к Rutube. Если включён VPN или прокси, попробуйте отключить его и повторить загрузку.";

fn network_failure(s: &str) -> bool {
    [
        "timed out",
        "timeout",
        "resolve",
        "network",
        "connection",
        "nodename nor servname",
        "name or service not known",
        "name resolution",
        "failed to lookup address",
        "getaddrinfo failed",
    ]
    .iter()
    .any(|part| s.contains(part))
}

fn rutube_connection_failure(url: &str, error: &str) -> bool {
    let host = url
        .split('/')
        .nth(2)
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let s = error.to_lowercase();
    (host == "rutube.ru" || host.ends_with(".rutube.ru"))
        && (network_failure(&s) || s.contains("http error 403") || s.contains("http status: 403"))
}

pub fn download_error(url: &str, stderr: &str) -> &'static str {
    let s = stderr.to_lowercase();
    if s.contains("not a bot") || s.contains("confirm you're") || s.contains("confirm you’re") {
        "YouTube просит подтвердить вход. Откройте ролик в браузере, пройдите проверку и повторите с выбранным браузером. Если не помогло — обновите загрузчик или попробуйте другую сеть."
    } else if s.contains("decrypt")
        || s.contains("cookie database")
        || s.contains("could not copy")
        || s.contains("keyring")
        || s.contains("cookies database")
        || s.contains("could not find") && s.contains("cookies")
    {
        "Не удалось прочитать сессию браузера. Закройте браузер и повторите или выберите другой браузер, в котором открывается ролик."
    } else if s.contains("sign in")
        || s.contains("login required")
        || s.contains("private video")
        || s.contains("members-only")
        || s.contains("age-restricted")
    {
        "Видео требует входа или ограничено владельцем. Проверьте доступ в браузере; для YouTube можно выбрать этот браузер перед загрузкой."
    } else if s.contains("429") || s.contains("too many requests") {
        "Видеосервис временно ограничил запросы. Подождите и повторите позже."
    } else if s.contains("unsupported url") {
        "Эта ссылка не поддерживается загрузчиком. Импортируйте аудио или видеофайл."
    } else if rutube_connection_failure(url, stderr) {
        RUTUBE_CONNECTION_HINT
    } else if network_failure(&s) {
        "Не удалось подключиться к видеосервису. Проверьте интернет и доступ к ролику в браузере."
    } else {
        "Не удалось скачать видео. Обновите загрузчик в настройках и проверьте доступ к ролику в браузере."
    }
}

fn terminate_download(child: &mut std::process::Child) {
    // Standalone yt-dlp can spawn a bootloader child or FFmpeg. Stop the whole
    // owned process group so cancellation also closes inherited stdout pipes.
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        unsafe {
            kill(-(child.id() as i32), 9);
        }
    }
    #[cfg(windows)]
    {
        let _ = crate::sys::command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn download_page(
    tool: &Path,
    url: &str,
    dir: &Path,
    progress: &Progress,
    browser: Option<&str>,
) -> Result<(PathBuf, String)> {
    use std::io::BufRead;
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};
    validate_browser(url, browser)?;
    if (progress.cancelled)() {
        return Err(anyhow!("отменено"));
    }
    clean_downloads(dir);
    let mut command = crate::sys::command(tool);
    command
        .env("PYTHONIOENCODING", "utf-8")
        .args([
            "--ignore-config",
            "--encoding",
            "utf-8",
            "--no-playlist",
            "--no-simulate",
            "--newline",
            "--progress",
            "--socket-timeout",
            "20",
            "--retries",
            "2",
            "--fragment-retries",
            "2",
            "--abort-on-unavailable-fragments",
            "--print",
            "before_dl:solflow-title %(title)j",
            "--print",
            "after_move:solflow-file %(filepath)j",
            "--progress-template",
            PROGRESS_TEMPLATE,
            "-f",
            "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio/worst[ext=mp4]/worst",
            "-o",
        ])
        .arg(dir.join("download.%(ext)s"));
    if let Some(ffmpeg) = crate::tools::ffmpeg() {
        command.arg("--ffmpeg-location").arg(ffmpeg);
    }
    if let Some(deno) = crate::tools::deno() {
        command
            .arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(browser) = browser {
        command.args(["--cookies-from-browser", browser]);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .arg("--")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let seen = Arc::new(Mutex::new((0u64, 0u64, String::new(), String::new())));
    let reader_seen = seen.clone();
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let mut state = reader_seen.lock().unwrap();
            if let Some((d, t)) = parse_progress(&line) {
                state.0 = d;
                state.1 = t;
            } else if let Some(v) = line.strip_prefix("solflow-title ") {
                state.2 = serde_json::from_str::<String>(v).unwrap_or_default();
            } else if let Some(v) = line.strip_prefix("solflow-file ") {
                state.3 = serde_json::from_str::<String>(v).unwrap_or_default();
            }
        }
    });
    // Drain stderr while the process runs: a full pipe used to block downloads.
    let stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut tail = std::collections::VecDeque::new();
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            if tail.len() == 16 {
                tail.pop_front();
            }
            tail.push_back(line.chars().take(2048).collect::<String>());
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });
    let mut last_progress = None;
    let status = loop {
        if (progress.cancelled)() {
            terminate_download(&mut child);
            let _ = reader.join();
            let _ = errors.join();
            clean_downloads(dir);
            return Err(anyhow!("отменено"));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                terminate_download(&mut child);
                let _ = reader.join();
                let _ = errors.join();
                clean_downloads(dir);
                return Err(e.into());
            }
        }
        // Release the mutex before callbacks/sleep so the pipe reader can drain.
        let (d, t) = {
            let state = seen.lock().unwrap();
            (state.0, state.1)
        };
        let current = (if d > 0 { d } else { downloaded_bytes(dir) }, t);
        if last_progress != Some(current) {
            (progress.report)(current.0, current.1);
            last_progress = Some(current);
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    };
    let _ = reader.join();
    let stderr = errors.join().unwrap_or_default();
    if !status.success() {
        clean_downloads(dir);
        return Err(anyhow!(download_error(url, &stderr)));
    }
    let state = seen.lock().unwrap();
    let file = PathBuf::from(&state.3);
    // A .part/.ytdl file is never a successfully downloaded recording.
    let valid = file.file_stem().map(|s| s == "download").unwrap_or(false)
        && file
            .extension()
            .map(|s| !["part", "ytdl", "json"].iter().any(|e| s == *e))
            .unwrap_or(false)
        && file
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            == dir.canonicalize().ok()
        && file.metadata().map(|m| m.len() > 0).unwrap_or(false);
    if !valid {
        clean_downloads(dir);
        return Err(anyhow!(
            "Загрузчик не создал готовый медиафайл. Обновите загрузчик и повторите."
        ));
    }
    (progress.report)(file.metadata()?.len(), file.metadata()?.len());
    Ok((
        file,
        if state.2.trim().is_empty() {
            "Запись по ссылке".into()
        } else {
            state.2.clone()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_estimate_and_invalid_values() {
        assert_eq!(parse_progress("solflow 1024 NA 2048"), Some((1024, 2048)));
        assert_eq!(parse_progress("solflow NaN -1 inf"), Some((0, 0)));
        assert_eq!(parse_progress("something else"), None);
    }
    #[test]
    fn transport_stream_links_are_direct_media() {
        assert!(looks_like_media("https://example.test/movie.TS?key=value"));
        assert!(looks_like_media("https://example.test/movie.m2ts"));
        assert!(!looks_like_media("https://example.test/movie.ts/page"));
    }
    #[test]
    fn session_is_never_allowed_for_other_hosts_or_arguments() {
        assert!(validate_browser("https://www.youtube.com/watch?v=abc", Some("chrome")).is_ok());
        assert!(validate_browser("https://youtu.be/abc", None).is_ok());
        for host in [
            "youtube.com.evil.test",
            "youtube.com@evil.test",
            "rutube.ru",
        ] {
            assert!(validate_browser(&format!("https://{host}/video"), Some("chrome")).is_err());
        }
        assert!(validate_browser(
            "https://youtube.com/watch?v=abc",
            Some("chrome:/private/profile")
        )
        .is_err());
    }
    #[test]
    fn readable_errors_do_not_include_raw_output() {
        let e = download_error(
            "https://youtube.com/watch?v=x",
            "ERROR: [youtube] x: Sign in to confirm you’re not a bot. --cookies-from-browser",
        );
        assert!(e.contains("YouTube"));
        assert!(!e.contains("--cookies"));
        assert!(download_error(
            "https://youtube.com/",
            "ERROR: Could not copy Chrome cookie database"
        )
        .contains("сессию"));
        assert!(
            download_error("https://youtube.com/", "HTTP Error 429: Too Many Requests")
                .contains("ограничил")
        );
    }
    #[test]
    fn rutube_hint_only_for_connection_or_access_failures() {
        for host in ["rutube.ru", "www.rutube.ru", "RUTUBE.RU:443"] {
            for error in ["Connection timed out", "HTTP Error 403: Forbidden",
                "io: failed to lookup address information: nodename nor servname provided, or not known",
                "Temporary failure in name resolution", "getaddrinfo failed"] {
                assert_eq!(download_error(&format!("https://{host}/video/test/"), error), RUTUBE_CONNECTION_HINT);
            }
        }
        for host in ["youtube.com", "rutube.ru.evil.test", "rutube.ru@evil.test"] {
            assert_ne!(
                download_error(&format!("https://{host}/"), "Connection refused"),
                RUTUBE_CONNECTION_HINT
            );
        }
        for error in [
            "HTTP Error 404: Not Found",
            "Video has been deleted",
            "Unsupported URL",
            "HTTP Error 429: Too Many Requests",
            "Login required",
            "Cancelled",
        ] {
            assert_ne!(
                download_error("https://rutube.ru/video/test/", error),
                RUTUBE_CONNECTION_HINT
            );
        }
    }
    #[cfg(unix)]
    fn scenario(body: &str, cancel: bool) -> Result<(PathBuf, String)> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "sf-fetch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("fake.py");
        std::fs::write(&tool,format!("#!/usr/bin/python3\nimport sys,pathlib,json,time\np=pathlib.Path(sys.argv[sys.argv.index('-o')+1].replace('%(ext)s','mp4'))\nassert '--ignore-config' in sys.argv\nassert '--cookies-from-browser' not in sys.argv\nassert '--skip-download' not in sys.argv\n{body}\n")).unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = std::time::Instant::now();
        let result = download_page(
            &tool,
            "https://rutube.ru/video/test",
            &dir,
            &Progress {
                report: &|_, _| {},
                cancelled: &|| cancel && start.elapsed() > std::time::Duration::from_millis(350),
            },
            None,
        );
        if cancel {
            assert!(start.elapsed() < std::time::Duration::from_secs(4));
        }
        if result.is_err() {
            assert!(!dir.join("download.mp4.part").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
        result
    }
    #[test]
    #[cfg(unix)]
    fn drains_stderr_and_uses_completed_file() {
        let r=scenario("for i in range(5000): print('warning '*60,file=sys.stderr)\np.write_bytes(b'media')\nprint('solflow-title '+json.dumps('Название'))\nprint('solflow-file '+json.dumps(str(p)))",false).unwrap();
        assert_eq!(r.1, "Название");
    }
    #[test]
    #[cfg(unix)]
    fn partial_file_is_not_a_success() {
        assert!(scenario("p.with_suffix('.mp4.part').write_bytes(b'partial')\nprint('ERROR: not a bot',file=sys.stderr)\nsys.exit(1)",false).is_err());
        assert!(scenario("p.with_suffix('.mp4.part').write_bytes(b'partial')", false).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn progress_pipe_keeps_draining() {
        let started = std::time::Instant::now();
        scenario("for i in range(10000): print('solflow %d 10000 NA'%i)\np.write_bytes(b'media')\nprint('solflow-file '+json.dumps(str(p)))",false).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
    #[test]
    #[cfg(unix)]
    fn cancellation_kills_children_holding_pipes() {
        assert!(scenario("import subprocess\nsubprocess.Popen([sys.executable,'-c','import time; time.sleep(20)'])\ntime.sleep(20)", true).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn metadata_wait_can_be_cancelled() {
        assert!(scenario("time.sleep(20)", true)
            .unwrap_err()
            .to_string()
            .contains("отменено"));
    }
}
