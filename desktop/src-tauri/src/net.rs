//! Сеть одним местом. Раньше каждая загрузка запускала /usr/bin/curl и
//! следила за тем, как растёт файл на диске: на Windows такого curl нет, да
//! и приём хрупкий — прогресс врал, отмена убивала процесс. Теперь запросы
//! делает ureq, прогресс считается по мере чтения тела.
//!
//! TLS берётся системный (Security.framework на macOS, schannel на
//! Windows): rustls потянул бы за собой сборку ассемблера, а системному
//! стеку не нужно ни nasm, ни OpenSSL.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Result};
use ureq::tls::{TlsConfig, TlsProvider};
use ureq::Agent;

/// Некоторым API (GitHub) без внятного User-Agent отвечать не положено.
const USER_AGENT: &str = "SolFlow";

/// Только на соединение: тело качается сколько нужно, встреча на два часа
/// весит сотни мегабайт.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Сколько байт скачано между отчётами о прогрессе.
const REPORT_STEP: u64 = 256 * 1024;

fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let config = Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .build(),
            )
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build();
        Agent::new_with_config(config)
    })
}

/// Тело ответа текстом.
pub fn get_text(url: &str) -> Result<String> {
    let mut response = agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| anyhow!("{e}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| anyhow!("{e}"))
}

/// Тело ответа разобранным JSON.
pub fn get_json(url: &str) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(&get_text(url)?)?)
}

/// Тип содержимого по HEAD-запросу — для ссылок без расширения.
pub fn content_type(url: &str) -> String {
    header(url, "content-type").unwrap_or_default().to_lowercase()
}

fn header(url: &str, name: &str) -> Option<String> {
    let response = agent()
        .head(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .ok()?;
    Some(response.headers().get(name)?.to_str().ok()?.to_string())
}

/// Качает ссылку в файл. `on_progress` получает «скачано» и «всего» (ноль,
/// если размер неизвестен), `cancelled` спрашивается между кусками —
/// недокачанный файл при отмене убирается.
pub fn download(
    url: &str,
    target: &Path,
    on_progress: &dyn Fn(u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let mut response = agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| anyhow!("{e}"))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let mut file = File::create(target)?;
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;
    let mut reported: u64 = 0;

    loop {
        if cancelled() {
            drop(file);
            let _ = std::fs::remove_file(target);
            return Err(anyhow!("отменено"));
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(target);
                return Err(anyhow!("обрыв загрузки: {e}"));
            }
        };
        file.write_all(&buffer[..read])?;
        done += read as u64;
        if done - reported >= REPORT_STEP {
            reported = done;
            on_progress(done, total);
        }
    }

    file.flush()?;
    on_progress(done, total.max(done));
    Ok(())
}
