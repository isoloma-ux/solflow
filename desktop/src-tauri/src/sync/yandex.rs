//! Яндекс: вход по коду устройства и REST Диска.
//!
//! Вход — «как на телевизоре»: приложение получает короткий код, человек
//! вводит его на oauth.yandex.ru/device в любом браузере, приложение тем
//! временем опрашивает Яндекс и забирает токен. Один и тот же путь на Mac,
//! Windows и Android: ни локального веб-сервера, ни перехвата ссылок.
//!
//! Диск — папка приложения (`app:/`, на Диске это «Приложения/Sol Flow»):
//! приложение видит только её, остальные файлы человека ему недоступны.

use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::Agent;

/// Ключи приложения в Яндекс OAuth — один файл на все три платформы.
const CREDENTIALS: &str = include_str!("../../../../yandex-oauth.json");

const OAUTH: &str = "https://oauth.yandex.ru";
const DISK: &str = "https://cloud-api.yandex.net/v1/disk";

// Список разрешений в запросе не передаётся: Яндекс тогда берёт те, что
// заданы при регистрации приложения (папка приложения), а любое
// расхождение между кодом и консолью отвечало бы «invalid_scope».

#[derive(Deserialize)]
struct Credentials {
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    client_secret: String,
}

fn credentials() -> &'static Credentials {
    static CREDS: OnceLock<Credentials> = OnceLock::new();
    CREDS.get_or_init(|| {
        serde_json::from_str(CREDENTIALS).unwrap_or(Credentials {
            client_id: String::new(),
            client_secret: String::new(),
        })
    })
}

/// Ключи заданы — без них вход невозможен, и интерфейс говорит об этом
/// словами, а не падает на первом запросе.
pub fn configured() -> bool {
    let c = credentials();
    !c.client_id.is_empty() && !c.client_secret.is_empty()
}

/// Свой агент, а не общий из net.rs: тому нужны ошибки на 4xx, чтобы не
/// записать страницу «404» в файл модели, а здесь коды ответов — часть
/// протокола (409 «папка уже есть», 404 «файла нет»).
fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let config = Agent::config_builder()
            .tls_config(
            // Корни — системные. По умолчанию ureq отключает встроенные корни
            // и подсовывает набор Mozilla, а schannel на Windows строит
            // цепочку Яндекса до корня, которого в том наборе нет, — и
            // отвечает «unable to find any user-specified roots».
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_connect(Some(Duration::from_secs(15)))
            .http_status_as_error(false)
            .build();
        Agent::new_with_config(config)
    })
}

/// Ответ как есть: код и тело. Тело читается вручную — у встроенного
/// read_to_vec потолок в 10 МБ, а расшифровка двухчасовой встречи бывает
/// больше.
struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::from_slice(&self.body)?)
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

fn read_all(response: &mut ureq::http::Response<ureq::Body>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut out)
        .map_err(|e| anyhow!("обрыв соединения: {e}"))?;
    Ok(out)
}

fn finish(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply> {
    let mut response = result.map_err(|e| anyhow!("{e}"))?;
    let status = response.status().as_u16();
    let body = read_all(&mut response)?;
    Ok(Reply { status, body })
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

fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Ошибка Яндекса человеческим текстом: в теле обычно есть `message` или
/// `error_description`, и они внятнее кода.
fn describe(reply: &Reply, what: &str) -> anyhow::Error {
    let detail = reply
        .json()
        .ok()
        .and_then(|v| {
            ["message", "error_description", "description", "error"]
                .iter()
                .find_map(|k| v.get(k)?.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| reply.text().chars().take(120).collect());
    anyhow!("{what}: {} ({})", detail.trim(), reply.status)
}

// --- OAuth --------------------------------------------------------------------

/// Код, который человек вводит на странице Яндекса.
#[derive(Clone, serde::Serialize)]
pub struct DeviceCode {
    #[serde(skip)]
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    /// Не чаще, чем раз в столько секунд, можно спрашивать про токен.
    pub interval: u64,
    /// Момент (millis), после которого код протухает.
    pub expires_at: i64,
}

/// Токены после успешного входа.
#[derive(Clone)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Момент (millis), когда access_token перестанет работать.
    pub expires_at: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Шаг 1: попросить у Яндекса код для этого устройства.
pub fn device_code(device_name: &str) -> Result<DeviceCode> {
    if !configured() {
        return Err(anyhow!("ключи Яндекс OAuth не заданы в этой сборке"));
    }
    let c = credentials();
    let body = form(&[
        ("client_id", &c.client_id),
        ("device_id", &device_id()),
        ("device_name", device_name),
    ]);
    let reply = finish(
        agent()
            .post(&format!("{OAUTH}/device/code"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes()),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, "Яндекс не дал код"));
    }
    let v = reply.json()?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
    let expires_in = v.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(300);
    Ok(DeviceCode {
        device_code: get("device_code").ok_or_else(|| anyhow!("в ответе нет device_code"))?,
        user_code: get("user_code").ok_or_else(|| anyhow!("в ответе нет user_code"))?,
        verification_url: get("verification_url")
            .unwrap_or_else(|| "https://oauth.yandex.ru/device".to_string()),
        interval: v.get("interval").and_then(|x| x.as_u64()).unwrap_or(5).max(2),
        expires_at: now_ms() + expires_in * 1000,
    })
}

/// Что ответил Яндекс на очередной опрос.
pub enum Poll {
    /// Человек ещё не ввёл код — спросить позже.
    Pending,
    Done(Tokens),
}

/// Шаг 2: спросить, ввёл ли человек код. `authorization_pending` — норма,
/// всё остальное — конец истории.
pub fn poll_token(code: &DeviceCode) -> Result<Poll> {
    let c = credentials();
    let body = form(&[
        ("grant_type", "device_code"),
        ("code", &code.device_code),
        ("client_id", &c.client_id),
        ("client_secret", &c.client_secret),
    ]);
    let reply = finish(
        agent()
            .post(&format!("{OAUTH}/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes()),
    )?;
    let v = reply.json().unwrap_or(serde_json::Value::Null);
    if !reply.ok() {
        let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
        return match error {
            "authorization_pending" | "slow_down" => Ok(Poll::Pending),
            "expired_token" => Err(anyhow!("код устарел — запросите новый")),
            "access_denied" => Err(anyhow!("вы отказали приложению в доступе")),
            _ => Err(describe(&reply, "вход не удался")),
        };
    }
    Ok(Poll::Done(parse_tokens(&v, None)?))
}

/// Продление токена. Refresh-токен Яндекс может выдать новый, а может
/// оставить старый — берём что дали, иначе прежний.
pub fn refresh(refresh_token: &str) -> Result<Tokens> {
    let c = credentials();
    let body = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &c.client_id),
        ("client_secret", &c.client_secret),
    ]);
    let reply = finish(
        agent()
            .post(&format!("{OAUTH}/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes()),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, "не удалось продлить вход"));
    }
    parse_tokens(&reply.json()?, Some(refresh_token))
}

fn parse_tokens(v: &serde_json::Value, old_refresh: Option<&str>) -> Result<Tokens> {
    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("в ответе нет токена"))?
        .to_string();
    let refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| old_refresh.map(|s| s.to_string()))
        .unwrap_or_default();
    let expires_in = v.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(365 * 86_400);
    Ok(Tokens {
        access_token: access,
        refresh_token: refresh,
        expires_at: now_ms() + expires_in * 1000,
    })
}

/// Отзыв токена при выходе — чтобы на странице аккаунта не висело
/// разрешение, которым никто не пользуется. Ошибка не страшна: локально
/// токен всё равно стирается.
pub fn revoke(access_token: &str) {
    let c = credentials();
    let body = form(&[
        ("access_token", access_token),
        ("client_id", &c.client_id),
        ("client_secret", &c.client_secret),
    ]);
    let _ = agent()
        .post(&format!("{OAUTH}/revoke_token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes());
}

/// Устойчивый идентификатор устройства для Яндекса: он ограничивает
/// число токенов на устройство, поэтому каждый вход с той же машины должен
/// приходить под одним именем. Хранится рядом с настройками.
fn device_id() -> String {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let path = crate::sync::data_dir().join("device-id");
        if let Ok(id) = std::fs::read_to_string(&path) {
            let id = id.trim().to_string();
            if !id.is_empty() {
                return id;
            }
        }
        // Случайности хватает: миллисекунды плюс адрес объекта в куче.
        let seed = format!("{}-{:p}", now_ms(), &path);
        let id = format!("{:x}", md5::compute(seed.as_bytes()));
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, &id);
        id
    })
    .clone()
}

// --- Диск -----------------------------------------------------------------------

/// Файл на Диске, как его видит листинг. Размер пока никому не нужен, но
/// в отчёте о состоянии Диска пригодится.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RemoteFile {
    pub name: String,
    pub md5: String,
    /// Момент загрузки на Диск (millis).
    pub modified: i64,
    pub size: u64,
}

/// Так синхронизация отличает «нужно войти заново» от сбоя сети.
pub fn is_unauthorized(e: &anyhow::Error) -> bool {
    e.to_string().contains("(401)")
}

fn auth(token: &str) -> String {
    format!("OAuth {token}")
}

/// Кто вошёл и сколько места: логин показывается в настройках, место —
/// задел на предупреждение «Диск почти полон» при передаче звука.
#[allow(dead_code)]
pub struct DiskInfo {
    pub login: String,
    pub total: u64,
    pub used: u64,
}

pub fn disk_info(token: &str) -> Result<DiskInfo> {
    let reply = finish(
        agent()
            .get(&format!("{DISK}/?fields=user,total_space,used_space"))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, "Диск не ответил"));
    }
    let v = reply.json()?;
    let user = v.get("user").cloned().unwrap_or(serde_json::Value::Null);
    let login = user
        .get("display_name")
        .or_else(|| user.get("login"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok(DiskInfo {
        login,
        total: v.get("total_space").and_then(|x| x.as_u64()).unwrap_or(0),
        used: v.get("used_space").and_then(|x| x.as_u64()).unwrap_or(0),
    })
}

/// Создать папку; «уже есть» — не ошибка.
pub fn mkdir(token: &str, path: &str) -> Result<()> {
    let reply = finish(
        agent()
            .put(&format!("{DISK}/resources?path={}", urlencode(path)))
            .header("Authorization", &auth(token))
            .send(&[][..]),
    )?;
    if reply.ok() || reply.status == 409 {
        Ok(())
    } else {
        Err(describe(&reply, &format!("не удалось создать папку {path}")))
    }
}

/// Все файлы папки (без вложенных папок). Постранично: Яндекс отдаёт не
/// больше нескольких сотен за раз.
pub fn list(token: &str, path: &str) -> Result<Vec<RemoteFile>> {
    const PAGE: usize = 500;
    let mut out = Vec::new();
    let mut offset = 0usize;
    loop {
        let url = format!(
            "{DISK}/resources?path={}&limit={PAGE}&offset={offset}&fields=_embedded.total,_embedded.items.name,_embedded.items.type,_embedded.items.md5,_embedded.items.modified,_embedded.items.size",
            urlencode(path)
        );
        let reply = finish(
            agent()
                .get(&url)
                .header("Authorization", &auth(token))
                .call(),
        )?;
        if reply.status == 404 {
            return Ok(out);
        }
        if !reply.ok() {
            return Err(describe(&reply, &format!("не удалось прочитать {path}")));
        }
        let v = reply.json()?;
        let embedded = v.get("_embedded").cloned().unwrap_or(serde_json::Value::Null);
        let total = embedded.get("total").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let items = embedded
            .get("items")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let got = items.len();
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let text = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            out.push(RemoteFile {
                name: text("name"),
                md5: text("md5"),
                modified: parse_iso8601(&text("modified")).unwrap_or(0),
                size: item.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
            });
        }
        offset += got;
        if got == 0 || offset >= total {
            break;
        }
    }
    Ok(out)
}

/// Загрузка байт в файл на Диске; существующий перезаписывается.
pub fn upload(token: &str, path: &str, data: &[u8]) -> Result<()> {
    let href = upload_href(token, path)?;
    let reply = finish(
        agent()
            .put(&href)
            .header("Content-Type", "application/octet-stream")
            .send(data),
    )?;
    if reply.ok() {
        Ok(())
    } else {
        Err(describe(&reply, &format!("не удалось загрузить {path}")))
    }
}

/// Загрузка большого файла потоком с диска: звук встречи в память не
/// поднимаем.
pub fn upload_file(token: &str, path: &str, file: &std::path::Path) -> Result<()> {
    let href = upload_href(token, path)?;
    let mut source = std::fs::File::open(file)?;
    let reply = finish(
        agent()
            .put(&href)
            .header("Content-Type", "application/octet-stream")
            .send(ureq::SendBody::from_reader(&mut source)),
    )?;
    if reply.ok() {
        Ok(())
    } else {
        Err(describe(&reply, &format!("не удалось загрузить {path}")))
    }
}

fn upload_href(token: &str, path: &str) -> Result<String> {
    let reply = finish(
        agent()
            .get(&format!(
                "{DISK}/resources/upload?path={}&overwrite=true",
                urlencode(path)
            ))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("Диск не принял {path}")));
    }
    reply
        .json()?
        .get("href")
        .and_then(|h| h.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Диск не дал адрес для загрузки"))
}

fn download_href(token: &str, path: &str) -> Result<String> {
    let reply = finish(
        agent()
            .get(&format!("{DISK}/resources/download?path={}", urlencode(path)))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("Диск не отдал {path}")));
    }
    reply
        .json()?
        .get("href")
        .and_then(|h| h.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Диск не дал адрес для скачивания"))
}

/// Файл с Диска целиком в память — для JSON.
pub fn download(token: &str, path: &str) -> Result<Vec<u8>> {
    let href = download_href(token, path)?;
    let reply = finish(agent().get(&href).call())?;
    if reply.ok() {
        Ok(reply.body)
    } else {
        Err(describe(&reply, &format!("не удалось скачать {path}")))
    }
}

/// Файл с Диска на диск — для звука. Пишется рядом и переименовывается
/// по завершении, чтобы оборванная загрузка не выглядела готовым файлом.
pub fn download_file(token: &str, path: &str, target: &std::path::Path) -> Result<()> {
    let href = download_href(token, path)?;
    let mut response = agent().get(&href).call().map_err(|e| anyhow!("{e}"))?;
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(anyhow!(
            "не удалось скачать {path} ({})",
            response.status().as_u16()
        ));
    }
    let part = target.with_extension("part");
    let mut file = std::fs::File::create(&part)?;
    let copied = std::io::copy(&mut response.body_mut().as_reader(), &mut file);
    drop(file);
    match copied {
        Ok(_) => {
            std::fs::rename(&part, target)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(anyhow!("обрыв загрузки {path}: {e}"))
        }
    }
}

/// Удаление без корзины; «уже нет» — не ошибка.
pub fn delete(token: &str, path: &str) -> Result<()> {
    let reply = finish(
        agent()
            .delete(&format!(
                "{DISK}/resources?path={}&permanently=true",
                urlencode(path)
            ))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    if reply.ok() || reply.status == 404 {
        Ok(())
    } else {
        Err(describe(&reply, &format!("не удалось удалить {path}")))
    }
}

/// «2024-05-01T12:34:56+00:00» → millis. Своими руками: ради одной даты
/// тянуть chrono незачем.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // Дробные секунды и смещение зоны — что осталось после секунд.
    let mut rest = &s[19..];
    if rest.starts_with('.') {
        let end = rest[1..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    let offset = match rest.chars().next() {
        Some('Z') | None => 0,
        Some(sign @ ('+' | '-')) => {
            let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            let oh = digits.get(0..2)?.parse::<i64>().ok()?;
            let om = digits.get(2..4).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0);
            let total = oh * 3600 + om * 60;
            if sign == '+' { total } else { -total }
        }
        _ => 0,
    };
    let days = days_from_civil(y, mo, d);
    Some(((days * 86_400 + h * 3600 + mi * 60 + sec) - offset) * 1000)
}

/// Дни от эпохи по календарной дате (алгоритм Говарда Хиннанта).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00+00:00"), Some(0));
        assert_eq!(parse_iso8601("2024-05-01T12:34:56+00:00"), Some(1_714_566_896_000));
        // Смещение зоны вычитается: 15:34 в +03:00 — это 12:34 UTC.
        assert_eq!(parse_iso8601("2024-05-01T15:34:56+03:00"), Some(1_714_566_896_000));
        assert_eq!(parse_iso8601("2024-05-01T12:34:56.123Z"), Some(1_714_566_896_000));
        assert_eq!(parse_iso8601("мусор"), None);
    }
}
