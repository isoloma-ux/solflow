//! Google Drive: вход по коду устройства и REST Drive v3. Раскладка та же,
//! что на Яндекс.Диске, только вместо путей — идентификаторы: папка
//! «Sol Flow» в корне Диска человека, внутри «meetings» и «audio». Папка
//! обычная, видна в Диске — можно заглянуть и скачать руками.
//!
//! Разрешение только drive.file: приложение видит лишь файлы, которые само
//! создало, остальной Диск человека ему недоступен. Для такого разрешения
//! Google не требует проверки приложения.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use serde::Deserialize;

use super::provider::{now_ms, DeviceCode, Folder, Poll, Provider, RemoteFile, Tokens};
use super::yandex::{agent, describe, finish, form, parse_iso8601, urlencode, Reply};

/// Ключи приложения в Google Cloud. Файл в репозиторий не попадает —
/// защита GitHub его не пускает, — build.rs подставляет пустой, если его
/// нет: тогда «не настроено», а не ошибка сборки.
const CREDENTIALS: &str = include_str!(concat!(env!("OUT_DIR"), "/google-oauth.json"));

const OAUTH: &str = "https://oauth2.googleapis.com";
const DRIVE: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const ROOT_NAME: &str = "Sol Flow";

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

pub fn configured() -> bool {
    let c = credentials();
    !c.client_id.is_empty() && !c.client_secret.is_empty()
}

fn auth(token: &str) -> String {
    format!("Bearer {token}")
}

// --- OAuth ------------------------------------------------------------------------

fn device_code(device_name: &str) -> Result<DeviceCode> {
    if !configured() {
        return Err(anyhow!("ключи Google OAuth не заданы в этой сборке"));
    }
    let _ = device_name; // Google имя устройства не принимает.
    let c = credentials();
    let body = form(&[("client_id", &c.client_id), ("scope", SCOPE)]);
    let reply = finish(
        agent()
            .post(&format!("{OAUTH}/device/code"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes()),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, "Google не дал код"));
    }
    let v = reply.json()?;
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
    let expires_in = v.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(1800);
    Ok(DeviceCode {
        device_code: get("device_code").ok_or_else(|| anyhow!("в ответе нет device_code"))?,
        user_code: get("user_code").ok_or_else(|| anyhow!("в ответе нет user_code"))?,
        verification_url: get("verification_url")
            .unwrap_or_else(|| "https://www.google.com/device".to_string()),
        interval: v.get("interval").and_then(|x| x.as_u64()).unwrap_or(5).max(2),
        expires_at: now_ms() + expires_in * 1000,
    })
}

fn poll_token(code: &DeviceCode) -> Result<Poll> {
    let c = credentials();
    let body = form(&[
        ("client_id", &c.client_id),
        ("client_secret", &c.client_secret),
        ("device_code", &code.device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
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
    parse_tokens(&v, None).map(Poll::Done)
}

fn refresh(refresh_token: &str) -> Result<Tokens> {
    let c = credentials();
    let body = form(&[
        ("client_id", &c.client_id),
        ("client_secret", &c.client_secret),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ]);
    let reply = finish(
        agent()
            .post(&format!("{OAUTH}/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes()),
    )?;
    if !reply.ok() {
        // Отозванный refresh-токен — это «войти заново», как 401.
        return Err(anyhow!("{} (401)", describe(&reply, "токен не продлился")));
    }
    parse_tokens(&reply.json()?, Some(refresh_token))
}

fn parse_tokens(v: &serde_json::Value, old_refresh: Option<&str>) -> Result<Tokens> {
    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("в ответе нет access_token"))?;
    let refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .or(old_refresh)
        .unwrap_or("");
    let expires_in = v.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(3600);
    Ok(Tokens {
        access_token: access.to_string(),
        refresh_token: refresh.to_string(),
        expires_at: now_ms() + expires_in * 1000,
    })
}

fn revoke(token: &str) {
    let _ = agent()
        .post(&format!("{OAUTH}/revoke?token={}", urlencode(token)))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(b"");
}

fn account(token: &str) -> Result<String> {
    let reply = finish(
        agent()
            .get(&format!("{DRIVE}/about?fields=user(emailAddress)"))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, "не удалось узнать аккаунт"));
    }
    let v = reply.json()?;
    Ok(v.pointer("/user/emailAddress")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string())
}

// --- папки и файлы --------------------------------------------------------------

/// Идентификаторы папок и файлов по именам — чтобы не искать по Диску
/// перед каждой операцией. Один аккаунт за раз, поэтому кэш один.
#[derive(Default, Clone)]
struct Ids {
    meetings: String,
    audio: String,
    /// Имя файла → id, по папкам; заполняется листингом и загрузками.
    files: HashMap<(Folder, String), String>,
}

static IDS: Mutex<Option<Ids>> = Mutex::new(None);

fn drive_get(token: &str, url: &str) -> Result<Reply> {
    finish(agent().get(url).header("Authorization", &auth(token)).call())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Первая папка с таким именем внутри родителя (не в корзине), если есть.
fn find_child(token: &str, parent: &str, name: &str, folder: bool) -> Result<Option<String>> {
    let mut q = format!(
        "name = '{}' and '{}' in parents and trashed = false",
        escape(name),
        escape(parent)
    );
    if folder {
        q.push_str(&format!(" and mimeType = '{FOLDER_MIME}'"));
    }
    let url = format!("{DRIVE}/files?q={}&fields=files(id)&pageSize=5", urlencode(&q));
    let reply = drive_get(token, &url)?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("не удалось найти «{name}»")));
    }
    Ok(reply
        .json()?
        .pointer("/files/0/id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string()))
}

fn create_folder(token: &str, parent: &str, name: &str) -> Result<String> {
    let body = serde_json::json!({ "name": name, "mimeType": FOLDER_MIME, "parents": [parent] });
    let reply = finish(
        agent()
            .post(&format!("{DRIVE}/files?fields=id"))
            .header("Authorization", &auth(token))
            .header("Content-Type", "application/json")
            .send(body.to_string().as_bytes()),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("не удалось создать папку «{name}»")));
    }
    reply
        .json()?
        .get("id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("в ответе нет id папки"))
}

fn ensure_folder(token: &str, parent: &str, name: &str) -> Result<String> {
    match find_child(token, parent, name, true)? {
        Some(id) => Ok(id),
        None => create_folder(token, parent, name),
    }
}

/// Папки на месте и их id в кэше.
fn ensure_ids(token: &str) -> Result<Ids> {
    if let Some(ids) = IDS.lock().unwrap().as_ref() {
        return Ok(ids.clone());
    }
    let root = ensure_folder(token, "root", ROOT_NAME)?;
    let ids = Ids {
        meetings: ensure_folder(token, &root, "meetings")?,
        audio: ensure_folder(token, &root, "audio")?,
        files: HashMap::new(),
    };
    *IDS.lock().unwrap() = Some(ids.clone());
    Ok(ids)
}

fn folder_id(ids: &Ids, folder: Folder) -> &str {
    match folder {
        Folder::Meetings => &ids.meetings,
        Folder::Audio => &ids.audio,
    }
}

fn remember(folder: Folder, name: &str, id: &str) {
    if let Some(ids) = IDS.lock().unwrap().as_mut() {
        ids.files.insert((folder, name.to_string()), id.to_string());
    }
}

fn forget(folder: Folder, name: &str) {
    if let Some(ids) = IDS.lock().unwrap().as_mut() {
        ids.files.remove(&(folder, name.to_string()));
    }
}

/// id файла по имени: из кэша, иначе поиском.
fn file_id(token: &str, folder: Folder, name: &str) -> Result<Option<String>> {
    let ids = ensure_ids(token)?;
    if let Some(id) = ids.files.get(&(folder, name.to_string())) {
        return Ok(Some(id.clone()));
    }
    let found = find_child(token, folder_id(&ids, folder), name, false)?;
    if let Some(id) = &found {
        remember(folder, name, id);
    }
    Ok(found)
}

fn list(token: &str, folder: Folder) -> Result<Vec<RemoteFile>> {
    let ids = ensure_ids(token)?;
    let parent = folder_id(&ids, folder).to_string();
    let q = format!("'{}' in parents and trashed = false", escape(&parent));
    let mut out = Vec::new();
    let mut page: Option<String> = None;
    loop {
        let mut url = format!(
            "{DRIVE}/files?q={}&fields=nextPageToken,files(id,name,md5Checksum,modifiedTime,size)&pageSize=1000",
            urlencode(&q)
        );
        if let Some(p) = &page {
            url.push_str(&format!("&pageToken={}", urlencode(p)));
        }
        let reply = drive_get(token, &url)?;
        if !reply.ok() {
            return Err(describe(&reply, "не удалось прочитать папку"));
        }
        let v = reply.json()?;
        for item in v.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let text = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let name = text("name");
            remember(folder, &name, &text("id"));
            out.push(RemoteFile {
                name,
                md5: text("md5Checksum"),
                modified: parse_iso8601(&text("modifiedTime")).unwrap_or(0),
                size: item
                    .get("size")
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            });
        }
        page = v.get("nextPageToken").and_then(|x| x.as_str()).map(|s| s.to_string());
        if page.is_none() {
            break;
        }
    }
    Ok(out)
}

/// Загрузка байт: существующий файл с тем же именем перезаписывается
/// (иначе Drive заводил бы второй с таким же именем), новый создаётся
/// одним multipart-запросом с метаданными.
fn upload(token: &str, folder: Folder, name: &str, data: &[u8]) -> Result<()> {
    if let Some(id) = file_id(token, folder, name)? {
        let reply = finish(
            agent()
                .patch(&format!("{UPLOAD}/{id}?uploadType=media"))
                .header("Authorization", &auth(token))
                .header("Content-Type", "application/octet-stream")
                .send(data),
        )?;
        if !reply.ok() {
            return Err(describe(&reply, &format!("не удалось загрузить {name}")));
        }
        return Ok(());
    }
    let ids = ensure_ids(token)?;
    let meta = serde_json::json!({ "name": name, "parents": [folder_id(&ids, folder)] });
    let boundary = "solflow-multipart-boundary";
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
    let reply = finish(
        agent()
            .post(&format!("{UPLOAD}?uploadType=multipart&fields=id"))
            .header("Authorization", &auth(token))
            .header("Content-Type", &format!("multipart/related; boundary={boundary}"))
            .send(&body),
    )?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("не удалось загрузить {name}")));
    }
    if let Some(id) = reply.json()?.get("id").and_then(|x| x.as_str()) {
        remember(folder, name, id);
    }
    Ok(())
}

fn download(token: &str, folder: Folder, name: &str) -> Result<Vec<u8>> {
    let id = file_id(token, folder, name)?.ok_or_else(|| anyhow!("{name}: файла нет (404)"))?;
    let reply = drive_get(token, &format!("{DRIVE}/{}?alt=media", format!("files/{id}")))?;
    if !reply.ok() {
        return Err(describe(&reply, &format!("не удалось скачать {name}")));
    }
    Ok(reply.body)
}

fn delete(token: &str, folder: Folder, name: &str) -> Result<()> {
    let Some(id) = file_id(token, folder, name)? else {
        return Ok(());
    };
    let reply = finish(
        agent()
            .delete(&format!("{DRIVE}/files/{id}"))
            .header("Authorization", &auth(token))
            .call(),
    )?;
    forget(folder, name);
    if !reply.ok() && reply.status != 404 {
        return Err(describe(&reply, &format!("не удалось удалить {name}")));
    }
    Ok(())
}

pub struct Google;

impl Provider for Google {
    fn id(&self) -> &'static str {
        "google"
    }
    fn title(&self) -> &'static str {
        "Google Drive"
    }
    fn configured(&self) -> bool {
        configured()
    }
    fn device_code(&self, device_name: &str) -> Result<DeviceCode> {
        device_code(device_name)
    }
    fn poll_token(&self, code: &DeviceCode) -> Result<Poll> {
        poll_token(code)
    }
    fn refresh(&self, refresh_token: &str) -> Result<Tokens> {
        refresh(refresh_token)
    }
    fn revoke(&self, access_token: &str) {
        revoke(access_token)
    }
    fn account(&self, token: &str) -> Result<String> {
        account(token)
    }
    fn prepare(&self, token: &str) -> Result<()> {
        // Новый аккаунт — кэш папок прежнего ни о чём.
        *IDS.lock().unwrap() = None;
        ensure_ids(token).map(|_| ())
    }
    fn list(&self, token: &str, folder: Folder) -> Result<Vec<RemoteFile>> {
        list(token, folder)
    }
    fn upload(&self, token: &str, folder: Folder, name: &str, data: &[u8]) -> Result<()> {
        upload(token, folder, name, data)
    }
    fn upload_file(&self, token: &str, folder: Folder, name: &str, file: &std::path::Path) -> Result<()> {
        // Звук — сотни мегабайт, но грузим целиком: возобновляемая загрузка
        // Drive — отдельный протокол, пока не нужен.
        let data = std::fs::read(file)?;
        upload(token, folder, name, &data)
    }
    fn download(&self, token: &str, folder: Folder, name: &str) -> Result<Vec<u8>> {
        download(token, folder, name)
    }
    fn download_file(&self, token: &str, folder: Folder, name: &str, target: &std::path::Path) -> Result<()> {
        let data = download(token, folder, name)?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(target, data)?;
        Ok(())
    }
    fn delete(&self, token: &str, folder: Folder, name: &str) -> Result<()> {
        delete(token, folder, name)
    }
}
