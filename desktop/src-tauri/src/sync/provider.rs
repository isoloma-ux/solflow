//! Облако как набор из восьми операций: вход по коду, продление и отзыв
//! токена, папки, список, загрузка, скачивание, удаление. Логика
//! синхронизации в mod.rs ходит только через этот интерфейс, поэтому
//! Яндекс.Диск и Google Drive для неё неотличимы: раскладка файлов и
//! слияние одни и те же.

use anyhow::Result;

/// Код, который человек вводит на странице облака.
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

/// Что ответило облако на очередной опрос.
pub enum Poll {
    /// Человек ещё не ввёл код — спросить позже.
    Pending,
    Done(Tokens),
}

/// Файл в облаке, как его видит листинг.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RemoteFile {
    pub name: String,
    pub md5: String,
    /// Момент загрузки (millis).
    pub modified: i64,
    pub size: u64,
}

/// Две папки приложения в облаке.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Folder {
    Meetings,
    Audio,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Так синхронизация отличает «нужно войти заново» от сбоя сети: оба
/// клиента вписывают код ответа в текст ошибки.
pub fn is_unauthorized(e: &anyhow::Error) -> bool {
    e.to_string().contains("(401)")
}

pub trait Provider: Send + Sync {
    /// "yandex" или "google" — так провайдер записан в настройках.
    fn id(&self) -> &'static str;
    /// Название для окна.
    fn title(&self) -> &'static str;
    /// Ключи заданы в сборке — без них вход невозможен.
    fn configured(&self) -> bool;

    fn device_code(&self, device_name: &str) -> Result<DeviceCode>;
    fn poll_token(&self, code: &DeviceCode) -> Result<Poll>;
    fn refresh(&self, refresh_token: &str) -> Result<Tokens>;
    fn revoke(&self, access_token: &str);
    /// Кто вошёл — логин или почта для настроек.
    fn account(&self, token: &str) -> Result<String>;

    /// Папки приложения на месте (создаются, если нет). Вызывается перед
    /// первым проходом и после смены аккаунта.
    fn prepare(&self, token: &str) -> Result<()>;
    fn list(&self, token: &str, folder: Folder) -> Result<Vec<RemoteFile>>;
    fn upload(&self, token: &str, folder: Folder, name: &str, data: &[u8]) -> Result<()>;
    fn upload_file(&self, token: &str, folder: Folder, name: &str, file: &std::path::Path) -> Result<()>;
    fn download(&self, token: &str, folder: Folder, name: &str) -> Result<Vec<u8>>;
    fn download_file(&self, token: &str, folder: Folder, name: &str, target: &std::path::Path) -> Result<()>;
    fn delete(&self, token: &str, folder: Folder, name: &str) -> Result<()>;
}

/// Все провайдеры — для окна и для выбора по настройке.
pub fn all() -> [&'static dyn Provider; 2] {
    [&super::yandex::Yandex, &super::google::Google]
}

pub fn by_id(id: &str) -> &'static dyn Provider {
    all()
        .into_iter()
        .find(|p| p.id() == id)
        .unwrap_or(&super::yandex::Yandex)
}
