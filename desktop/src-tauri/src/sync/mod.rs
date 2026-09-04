//! Синхронизация встреч и проектов через облако пользователя — Яндекс.Диск
//! или Google Drive (см. provider.rs; раскладка и слияние одни).
//!
//! Никакого своего сервера: каждое устройство ходит в папку приложения на
//! Диске того человека, который вошёл, и чужие данные через нас не идут.
//! Главный сценарий — телефон записал и расшифровал, компьютер посчитал
//! саммери и название, всё вернулось на телефон.
//!
//! Раскладка на Диске плоская, чтобы одно чтение папки давало всю картину:
//!
//! ```text
//! app:/meetings/<id>.meta.json         мета встречи
//! app:/meetings/<id>.transcript.json   расшифровка
//! app:/meetings/<id>.deleted           надгробие: встречу удалили
//! app:/meetings/projects.json          проекты
//! app:/audio/<id>.wav                  звук — только если включено
//! ```
//!
//! Что менялось, определяется по md5: у каждого файла помним, каким его
//! видели на Диске и каким — у себя. Разошлось с одной стороны — копируем,
//! с обеих — сливаем (см. merge.rs). Удаление помечается надгробием, иначе
//! второе устройство воскресило бы стёртое из своей копии.

pub mod google;
pub mod merge;
pub mod provider;
pub mod yandex;

use provider::{now_ms, Folder, Provider};

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::meetings::{self, Meta, Project, STATE_TRANSCRIBING};
use crate::AppState;

/// Сколько ждать после изменения, прежде чем идти на Диск: серия правок
/// (имена говорящих одно за другим) должна уехать одним разом.
const DEBOUNCE: Duration = Duration::from_secs(20);
/// Первая синхронизация после запуска: даём сети и окну подняться.
const FIRST_DELAY: Duration = Duration::from_secs(15);
/// За сколько до конца срока токен продлевается заранее.
const REFRESH_AHEAD_MS: i64 = 7 * 24 * 3600 * 1000;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn data_dir() -> PathBuf {
    DATA_DIR.get().cloned().unwrap_or_default()
}

// --- состояние на диске ------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Clone)]
struct FileState {
    /// md5 файла, каким его в последний раз видели на Диске.
    remote: String,
    /// md5 местного файла после последней синхронизации.
    local: String,
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    #[serde(default)]
    files: HashMap<String, FileState>,
    /// Удалённые здесь встречи, о которых Диск ещё не знает.
    #[serde(default)]
    pending_deletes: Vec<i64>,
    /// Проекты после прошлой синхронизации — база трёхстороннего слияния.
    #[serde(default)]
    projects_snapshot: Vec<Project>,
    #[serde(default)]
    last_sync: i64,
    #[serde(default)]
    folders_ready: bool,
}

fn state_path() -> PathBuf {
    data_dir().join("sync.json")
}

impl State {
    fn load() -> State {
        std::fs::read_to_string(state_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        let _ = std::fs::write(state_path(), serde_json::to_string_pretty(self).unwrap());
    }
}

// --- состояние в памяти ------------------------------------------------------

/// Вход по коду, пока человек его вводит.
struct Flow {
    provider: &'static dyn Provider,
    code: provider::DeviceCode,
    cancel: AtomicBool,
}

pub struct SyncRuntime {
    running: AtomicBool,
    /// Когда что-то поменялось локально; None — всё уехало.
    dirty_at: Mutex<Option<Instant>>,
    last_run: Mutex<Option<Instant>>,
    flow: Mutex<Option<std::sync::Arc<Flow>>>,
    /// Последняя ошибка — окно показывает её под кнопкой.
    message: Mutex<Option<String>>,
    /// Что делается прямо сейчас: «Отправляю звук: Встреча 2 сентября».
    progress: Mutex<Option<String>>,
    /// Синхронизация идёт одна: вторая просьба во время первой ждёт.
    gate: Mutex<()>,
}

impl SyncRuntime {
    fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            dirty_at: Mutex::new(None),
            last_run: Mutex::new(None),
            flow: Mutex::new(None),
            message: Mutex::new(None),
            progress: Mutex::new(None),
            gate: Mutex::new(()),
        }
    }
}

/// Что окно показывает в настройках.
#[derive(Serialize, Clone)]
pub struct Status {
    /// Какие облака доступны в этой сборке (по ключам).
    pub configured_yandex: bool,
    pub configured_google: bool,
    /// Подключённое облако: "yandex" / "google", и его название.
    pub provider: String,
    pub provider_title: String,
    pub connected: bool,
    pub login: String,
    pub running: bool,
    pub last_sync: i64,
    pub message: Option<String>,
    pub progress: Option<String>,
    pub code: Option<provider::DeviceCode>,
    pub sync_audio: bool,
    pub sync_auto_summary: bool,
    pub sync_interval: String,
}

pub fn init(app: &AppHandle) {
    let _ = DATA_DIR.set(app.path().app_data_dir().unwrap_or_default());
    app.manage(SyncRuntime::new());
    spawn_watch(app.clone());
}

pub fn status(app: &AppHandle) -> Status {
    let rt = app.state::<SyncRuntime>();
    let app_state = app.state::<AppState>();
    let s = app_state.settings.lock().unwrap().clone();
    let state = State::load();
    let message = rt.message.lock().unwrap().clone();
    let progress = rt.progress.lock().unwrap().clone();
    let flow = rt.flow.lock().unwrap();
    let code = flow.as_ref().map(|f| f.code.clone());
    // Пока идёт вход — название того облака, куда входят.
    let current = flow
        .as_ref()
        .map(|f| f.provider)
        .unwrap_or_else(|| provider_of(&s));
    drop(flow);
    Status {
        configured_yandex: yandex::Yandex.configured(),
        configured_google: google::Google.configured(),
        provider: current.id().to_string(),
        provider_title: current.title().to_string(),
        connected: s.sync_token.is_some(),
        login: s.sync_login.clone(),
        running: rt.running.load(Ordering::Relaxed),
        last_sync: state.last_sync,
        message,
        progress,
        code,
        sync_audio: s.sync_audio,
        sync_auto_summary: s.sync_auto_summary,
        sync_interval: s.sync_interval.clone(),
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit("solflow-sync", status(app));
}

fn set_message(app: &AppHandle, message: Option<String>) {
    *app.state::<SyncRuntime>().message.lock().unwrap() = message;
    emit(app);
}

fn set_progress(app: &AppHandle, progress: Option<String>) {
    *app.state::<SyncRuntime>().progress.lock().unwrap() = progress;
    emit(app);
}

// --- вход и выход ------------------------------------------------------------

/// Облако из настроек; пустое при токене — Яндекс (настройки до Google).
fn provider_of(s: &crate::settings::Settings) -> &'static dyn Provider {
    provider::by_id(if s.sync_provider.is_empty() { "yandex" } else { &s.sync_provider })
}

fn current_provider(app: &AppHandle) -> &'static dyn Provider {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap();
    provider_of(&s)
}

/// Начало входа в выбранное облако: получить код и в фоне ждать, пока
/// человек его введёт.
pub fn connect_start(app: &AppHandle, provider_id: &str) -> Result<provider::DeviceCode> {
    let rt = app.state::<SyncRuntime>();
    let provider = provider::by_id(provider_id);
    if let Some(flow) = rt.flow.lock().unwrap().as_ref() {
        if flow.provider.id() == provider.id() && flow.code.expires_at > now_ms() {
            return Ok(flow.code.clone());
        }
    }
    let code = provider.device_code(&device_name())?;
    let flow = std::sync::Arc::new(Flow {
        provider,
        code: code.clone(),
        cancel: AtomicBool::new(false),
    });
    *rt.flow.lock().unwrap() = Some(flow.clone());
    set_message(app, None);

    let app = app.clone();
    std::thread::spawn(move || {
        let result = loop {
            std::thread::sleep(Duration::from_secs(flow.code.interval));
            if flow.cancel.load(Ordering::Relaxed) {
                break Ok(None);
            }
            if now_ms() > flow.code.expires_at {
                break Err(anyhow!("код устарел — запросите новый"));
            }
            match flow.provider.poll_token(&flow.code) {
                Ok(provider::Poll::Pending) => continue,
                Ok(provider::Poll::Done(tokens)) => break Ok(Some(tokens)),
                Err(e) => break Err(e),
            }
        };
        *app.state::<SyncRuntime>().flow.lock().unwrap() = None;
        match result {
            Ok(Some(tokens)) => {
                let login = flow.provider.account(&tokens.access_token).unwrap_or_default();
                {
                    let state = app.state::<AppState>();
                    let mut s = state.settings.lock().unwrap();
                    s.sync_provider = flow.provider.id().to_string();
                    s.sync_token = Some(tokens.access_token);
                    s.sync_refresh = Some(tokens.refresh_token).filter(|r| !r.is_empty());
                    s.sync_expires_at = tokens.expires_at;
                    s.sync_login = login;
                    crate::settings::save(&app, &s);
                }
                // Новый аккаунт — старые отметки о файлах ни о чём.
                let _ = std::fs::remove_file(state_path());
                set_message(&app, None);
                sync_now(&app);
            }
            Ok(None) => emit(&app),
            Err(e) => set_message(&app, Some(format!("{e}"))),
        }
    });
    Ok(code)
}

pub fn connect_cancel(app: &AppHandle) {
    if let Some(flow) = app.state::<SyncRuntime>().flow.lock().unwrap().take() {
        flow.cancel.store(true, Ordering::Relaxed);
    }
    emit(app);
}

/// Выход: токен отзывается у облака и стирается здесь. Сами встречи
/// остаются — и на этом устройстве, и в облаке.
pub fn disconnect(app: &AppHandle) {
    let (provider, token) = {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().unwrap();
        let provider = provider_of(&s);
        let token = s.sync_token.take();
        s.sync_refresh = None;
        s.sync_expires_at = 0;
        s.sync_login.clear();
        crate::settings::save(app, &s);
        (provider, token)
    };
    if let Some(token) = token {
        std::thread::spawn(move || provider.revoke(&token));
    }
    let _ = std::fs::remove_file(state_path());
    set_message(app, None);
}

fn device_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "компьютер".to_string());
    let host = host.trim_end_matches(".local");
    format!("Sol Flow · {host}")
}

// --- триггеры -------------------------------------------------------------------

/// Локально что-то поменялось — уедет с задержкой, когда правки утихнут.
pub fn touch(app: &AppHandle) {
    if let Some(rt) = app.try_state::<SyncRuntime>() {
        *rt.dirty_at.lock().unwrap() = Some(Instant::now());
    }
}

/// Встречу удалили: Диск узнает об этом надгробием при следующем заходе.
/// Записывается сразу, а не в памяти, — приложение могут закрыть раньше.
pub fn note_deleted(app: &AppHandle, id: i64) {
    if DATA_DIR.get().is_none() {
        return;
    }
    let connected = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .sync_token
        .is_some();
    if !connected {
        return;
    }
    let mut state = State::load();
    if !state.pending_deletes.contains(&id) {
        state.pending_deletes.push(id);
        state.save();
    }
    touch(app);
}

/// Синхронизация сейчас, в фоне. Идущая — не прерывается, просьба ждёт её.
pub fn sync_now(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || run_guarded(&app));
}

fn spawn_watch(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_DELAY);
        run_guarded(&app);
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let rt = app.state::<SyncRuntime>();
            let dirty = rt
                .dirty_at
                .lock()
                .unwrap()
                .map(|t| t.elapsed() >= DEBOUNCE)
                .unwrap_or(false);
            // Интервал читается каждый раз: настройку меняют на ходу.
            let period = app.state::<AppState>().settings.lock().unwrap().sync_period();
            let stale = match period {
                Some(period) => rt
                    .last_run
                    .lock()
                    .unwrap()
                    .map(|t| t.elapsed() >= period)
                    .unwrap_or(true),
                None => false,
            };
            if dirty || stale {
                run_guarded(&app);
            }
        }
    });
}

fn run_guarded(app: &AppHandle) {
    let connected = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .sync_token
        .is_some();
    let rt = app.state::<SyncRuntime>();
    if !connected {
        *rt.last_run.lock().unwrap() = Some(Instant::now());
        return;
    }
    let _turn = rt.gate.lock().unwrap();
    *rt.dirty_at.lock().unwrap() = None;
    rt.running.store(true, Ordering::Relaxed);
    emit(app);

    let result = match run(app) {
        Err(e) if provider::is_unauthorized(&e) => match refresh_tokens(app) {
            Ok(()) => run(app),
            Err(re) => Err(anyhow!("нужно войти заново: {re}")),
        },
        other => other,
    };

    let rt = app.state::<SyncRuntime>();
    *rt.last_run.lock().unwrap() = Some(Instant::now());
    rt.running.store(false, Ordering::Relaxed);
    *rt.progress.lock().unwrap() = None;
    match result {
        Ok(()) => set_message(app, None),
        Err(e) => {
            log::warn!("синхронизация: {e}");
            set_message(app, Some(format!("{e}")));
        }
    }
}

// --- токены -----------------------------------------------------------------------

fn token(app: &AppHandle) -> Result<String> {
    let (token, expires_at, has_refresh) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (
            s.sync_token.clone(),
            s.sync_expires_at,
            s.sync_refresh.is_some(),
        )
    };
    let token = token.ok_or_else(|| anyhow!("облако не подключено"))?;
    if has_refresh && expires_at > 0 && expires_at - now_ms() < REFRESH_AHEAD_MS {
        if let Err(e) = refresh_tokens(app) {
            log::warn!("продление токена не удалось: {e}");
        }
        return Ok(app
            .state::<AppState>()
            .settings
            .lock()
            .unwrap()
            .sync_token
            .clone()
            .unwrap_or(token));
    }
    Ok(token)
}

fn refresh_tokens(app: &AppHandle) -> Result<()> {
    let (provider, refresh) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (provider_of(&s), s.sync_refresh.clone())
    };
    let refresh = refresh.ok_or_else(|| anyhow!("нет refresh-токена"))?;
    let tokens = provider.refresh(&refresh)?;
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.sync_token = Some(tokens.access_token);
    if !tokens.refresh_token.is_empty() {
        s.sync_refresh = Some(tokens.refresh_token);
    }
    s.sync_expires_at = tokens.expires_at;
    crate::settings::save(app, &s);
    Ok(())
}

// --- сама синхронизация -------------------------------------------------------

fn md5_hex(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

fn file_mtime_ms(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Что делать с одним файлом, судя по тому, где он изменился.
#[derive(Debug, PartialEq)]
enum Plan {
    Nothing,
    Upload,
    Download,
    Conflict,
    Forget,
}

fn plan(
    local: Option<&str>,
    remote: Option<&str>,
    seen: Option<&FileState>,
) -> Plan {
    match (local, remote) {
        (None, None) => Plan::Forget,
        (Some(_), None) => Plan::Upload,
        (None, Some(_)) => Plan::Download,
        (Some(l), Some(r)) => {
            let local_changed = seen.map(|s| s.local != l).unwrap_or(true);
            let remote_changed = seen.map(|s| s.remote != r).unwrap_or(true);
            match (local_changed, remote_changed) {
                (false, false) => Plan::Nothing,
                (true, false) => Plan::Upload,
                (false, true) => Plan::Download,
                (true, true) => Plan::Conflict,
            }
        }
    }
}

struct Run<'a> {
    app: &'a AppHandle,
    cloud: &'static dyn Provider,
    token: String,
    state: State,
    remote: HashMap<String, provider::RemoteFile>,
    /// Встречи, у которых что-то приехало, — кандидаты на авто-саммери.
    arrived: HashSet<i64>,
    /// Хоть что-то поменялось локально — окну надо перечитать список.
    changed_local: bool,
}

impl Run<'_> {
    fn remote_md5(&self, name: &str) -> Option<&str> {
        self.remote.get(name).map(|f| f.md5.as_str())
    }

    fn upload(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.cloud.upload(&self.token, Folder::Meetings, name, bytes)?;
        let md5 = md5_hex(bytes);
        self.state.files.insert(
            name.to_string(),
            FileState {
                remote: md5.clone(),
                local: md5,
            },
        );
        Ok(())
    }

    fn download(&mut self, name: &str) -> Result<Vec<u8>> {
        let bytes = self.cloud.download(&self.token, Folder::Meetings, name)?;
        Ok(bytes)
    }

    fn mark_downloaded(&mut self, name: &str, bytes: &[u8]) {
        let md5 = md5_hex(bytes);
        self.state.files.insert(
            name.to_string(),
            FileState {
                remote: self.remote_md5(name).unwrap_or(&md5).to_string(),
                local: md5,
            },
        );
    }

    fn write_local(&mut self, id: i64, file: &str, bytes: &[u8]) -> Result<()> {
        let dir = meetings::dir(self.app, id);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(file), bytes)?;
        self.changed_local = true;
        self.arrived.insert(id);
        Ok(())
    }

    fn remove_remote(&mut self, name: &str) -> Result<()> {
        self.cloud.delete(&self.token, Folder::Meetings, name)?;
        self.state.files.remove(name);
        Ok(())
    }

    // --- встреча ---

    fn tombstone(&mut self, id: i64) -> Result<()> {
        self.upload(&format!("{id}.deleted"), b"{}")?;
        for name in [format!("{id}.meta.json"), format!("{id}.transcript.json")] {
            if self.remote.contains_key(&name) {
                self.remove_remote(&name)?;
            }
        }
        let _ = self.cloud.delete(&self.token, Folder::Audio, &format!("{id}.wav"));
        self.state.pending_deletes.retain(|d| *d != id);
        Ok(())
    }

    fn apply_tombstone(&mut self, id: i64) {
        let dir = meetings::dir(self.app, id);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
            self.changed_local = true;
        }
        self.state.files.remove(&format!("{id}.meta.json"));
        self.state.files.remove(&format!("{id}.transcript.json"));
        self.state.pending_deletes.retain(|d| *d != id);
    }

    fn sync_meta(&mut self, id: i64) -> Result<()> {
        let name = format!("{id}.meta.json");
        let path = meetings::dir(self.app, id).join("meta.json");
        let local_bytes = std::fs::read(&path).ok();
        let local_md5 = local_bytes.as_deref().map(md5_hex);
        let seen = self.state.files.get(&name).cloned();
        match plan(local_md5.as_deref(), self.remote_md5(&name), seen.as_ref()) {
            Plan::Nothing => {}
            Plan::Forget => {
                self.state.files.remove(&name);
            }
            Plan::Upload => {
                let bytes = local_bytes.unwrap_or_default();
                // Встречу посреди расшифровки не отправляем: второе устройство
                // показывало бы «расшифровываю» без прогресса и конца.
                let meta: Option<Meta> = serde_json::from_slice(&bytes).ok();
                if meta.map(|m| m.state == STATE_TRANSCRIBING).unwrap_or(true) {
                    return Ok(());
                }
                self.upload(&name, &bytes)?;
            }
            Plan::Download => {
                let bytes = self.download(&name)?;
                let meta: Meta = serde_json::from_slice(&bytes)
                    .map_err(|e| anyhow!("мета {id} на Диске нечитаема: {e}"))?;
                if meta.state == STATE_TRANSCRIBING {
                    return Ok(());
                }
                self.write_local(id, "meta.json", &bytes)?;
                self.mark_downloaded(&name, &bytes);
            }
            Plan::Conflict => {
                let local: Meta = serde_json::from_slice(local_bytes.as_deref().unwrap_or_default())
                    .unwrap_or_default();
                let remote_bytes = self.download(&name)?;
                let remote: Meta = serde_json::from_slice(&remote_bytes).unwrap_or_default();
                let merged = merge::merge_meta(&local, &remote);
                let bytes = serde_json::to_vec_pretty(&merged)?;
                self.write_local(id, "meta.json", &bytes)?;
                self.upload(&name, &bytes)?;
            }
        }
        Ok(())
    }

    fn sync_transcript(&mut self, id: i64) -> Result<()> {
        let name = format!("{id}.transcript.json");
        let path = meetings::dir(self.app, id).join("transcript.json");
        // Расшифровка без меты — осколок: подождём, пока приедет мета.
        if !meetings::dir(self.app, id).join("meta.json").exists() {
            return Ok(());
        }
        let local_bytes = std::fs::read(&path).ok();
        let local_md5 = local_bytes.as_deref().map(md5_hex);
        let seen = self.state.files.get(&name).cloned();
        match plan(local_md5.as_deref(), self.remote_md5(&name), seen.as_ref()) {
            Plan::Nothing => {}
            Plan::Forget => {
                self.state.files.remove(&name);
            }
            Plan::Upload => {
                let bytes = local_bytes.unwrap_or_default();
                self.upload(&name, &bytes)?;
            }
            Plan::Download => {
                let bytes = self.download(&name)?;
                self.write_local(id, "transcript.json", &bytes)?;
                self.mark_downloaded(&name, &bytes);
            }
            Plan::Conflict => {
                // Две разных расшифровки одной записи — берём более свежую.
                let remote_modified = self.remote.get(&name).map(|f| f.modified).unwrap_or(0);
                if remote_modified > file_mtime_ms(&path) {
                    let bytes = self.download(&name)?;
                    self.write_local(id, "transcript.json", &bytes)?;
                    self.mark_downloaded(&name, &bytes);
                } else {
                    let bytes = local_bytes.unwrap_or_default();
                    self.upload(&name, &bytes)?;
                }
            }
        }
        Ok(())
    }

    // --- проекты ---

    fn sync_projects(&mut self) -> Result<()> {
        let name = "projects.json";
        let local = meetings::projects(self.app);
        let local_bytes = serde_json::to_vec_pretty(&local)?;
        let local_md5 = md5_hex(&local_bytes);
        let seen = self.state.files.get(name).cloned();
        let remote_md5 = self.remote_md5(name).map(|s| s.to_string());

        let local_changed = seen.as_ref().map(|s| s.local != local_md5).unwrap_or(true);
        let remote_changed = seen.as_ref().map(|s| Some(s.remote.as_str()) != remote_md5.as_deref())
            .unwrap_or(remote_md5.is_some());
        if !local_changed && !remote_changed {
            return Ok(());
        }

        let remote: Vec<Project> = match &remote_md5 {
            Some(_) => serde_json::from_slice(&self.download(name)?).unwrap_or_default(),
            None => Vec::new(),
        };
        let merged = if remote_md5.is_none() && self.state.projects_snapshot.is_empty() {
            local.clone()
        } else {
            merge::merge_projects(&local, &remote, &self.state.projects_snapshot)
        };

        if !merge::same_projects(&merged, &local) {
            meetings::replace_projects(self.app, &merged);
            self.changed_local = true;
        }
        let bytes = serde_json::to_vec_pretty(&merged)?;
        if !merge::same_projects(&merged, &remote) || remote_md5.is_none() {
            self.upload(name, &bytes)?;
        } else {
            let md5 = md5_hex(&bytes);
            self.state.files.insert(
                name.to_string(),
                FileState {
                    remote: remote_md5.unwrap_or_else(|| md5.clone()),
                    local: md5,
                },
            );
        }
        self.state.projects_snapshot = merged;
        Ok(())
    }

    // --- звук ---

    fn sync_audio(&mut self, ids: &BTreeSet<i64>, busy: &HashSet<i64>) -> Result<()> {
        let remote_audio: HashSet<String> = self.cloud.list(&self.token, Folder::Audio)?
            .into_iter()
            .map(|f| f.name)
            .collect();
        for &id in ids {
            if busy.contains(&id) || self.state.pending_deletes.contains(&id) {
                continue;
            }
            let name = format!("{id}.wav");
            let local = meetings::audio_file(self.app, id);
            let has_local = local.exists();
            let has_remote = remote_audio.contains(&name);
            let has_meta_remote = self.remote.contains_key(&format!("{id}.meta.json"));
            let title = meetings::load_meta(self.app, id)
                .map(|m| meetings::display_title(&m))
                .unwrap_or_else(|| id.to_string());
            if has_local && !has_remote && has_meta_remote {
                set_progress(self.app, Some(format!("Отправляю звук: {title}")));
                self.cloud.upload_file(&self.token, Folder::Audio, &name, &local)?;
            } else if !has_local && has_remote && meetings::dir(self.app, id).join("meta.json").exists() {
                set_progress(self.app, Some(format!("Скачиваю звук: {title}")));
                self.cloud.download_file(&self.token, Folder::Audio, &name, &local)?;
                self.changed_local = true;
            }
        }
        set_progress(self.app, None);
        Ok(())
    }
}

fn parse_id(name: &str) -> Option<i64> {
    let stem = name.split('.').next()?;
    stem.parse().ok()
}

fn run(app: &AppHandle) -> Result<()> {
    let token = token(app)?;
    let cloud = current_provider(app);
    let mut state = State::load();

    if !state.folders_ready {
        cloud.prepare(&token)?;
        state.folders_ready = true;
        state.save();
    }

    let remote: HashMap<String, provider::RemoteFile> = cloud.list(&token, Folder::Meetings)?
        .into_iter()
        .map(|f| (f.name.clone(), f))
        .collect();

    let busy = meetings::busy_ids(app);
    let mut ids: BTreeSet<i64> = meetings::local_ids(app).into_iter().collect();
    ids.extend(remote.keys().filter_map(|n| parse_id(n)));
    ids.extend(state.pending_deletes.iter().copied());

    let (sync_audio, auto_summary) = {
        let app_state = app.state::<AppState>();
        let s = app_state.settings.lock().unwrap();
        (s.sync_audio, s.sync_auto_summary)
    };

    let mut run = Run {
        app,
        cloud,
        token,
        state,
        remote,
        arrived: HashSet::new(),
        changed_local: false,
    };

    let mut first_error: Option<anyhow::Error> = None;
    for &id in &ids {
        if busy.contains(&id) {
            continue;
        }
        let result = (|| -> Result<()> {
            if run.remote.contains_key(&format!("{id}.deleted")) {
                run.apply_tombstone(id);
                return Ok(());
            }
            if run.state.pending_deletes.contains(&id) {
                return run.tombstone(id);
            }
            run.sync_meta(id)?;
            run.sync_transcript(id)
        })();
        if let Err(e) = result {
            // Одна неудачная встреча не должна останавливать остальные;
            // но о первой ошибке скажем — и на 401 прервёмся сразу.
            if provider::is_unauthorized(&e) {
                run.state.save();
                return Err(e);
            }
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
        // Состояние пишется по ходу: обрыв посреди длинного списка не
        // заставит начинать с нуля.
        run.state.save();
    }

    if let Err(e) = run.sync_projects() {
        if provider::is_unauthorized(&e) {
            run.state.save();
            return Err(e);
        }
        first_error.get_or_insert(e);
    }

    if sync_audio {
        if let Err(e) = run.sync_audio(&ids, &busy) {
            if provider::is_unauthorized(&e) {
                run.state.save();
                return Err(e);
            }
            first_error.get_or_insert(e);
        }
    }

    if first_error.is_none() {
        run.state.last_sync = now_ms();
    }
    run.state.save();

    if run.changed_local {
        meetings::notify(app);
    }

    // Приехавшие готовые встречи без саммери: компьютер считает за
    // телефон — на телефоне модели нет. Только если модель уже здесь:
    // качать гигабайты молча не станем.
    if auto_summary && crate::summary::model_ready(app) {
        for id in run.arrived.iter().copied().collect::<Vec<_>>() {
            if let Some(meta) = meetings::load_meta(app, id) {
                if meta.state == meetings::STATE_DONE
                    && meta.summary.is_empty()
                    && !meetings::load_transcript(app, id).is_empty()
                {
                    meetings::summarize_and_title(app, id);
                }
            }
        }
    }

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(local: &str, remote: &str) -> FileState {
        FileState {
            local: local.into(),
            remote: remote.into(),
        }
    }

    #[test]
    fn plans() {
        assert_eq!(plan(None, None, None), Plan::Forget);
        assert_eq!(plan(Some("a"), None, None), Plan::Upload);
        assert_eq!(plan(None, Some("a"), None), Plan::Download);
        // Первая встреча файла на обеих сторонах — конфликт, сливаем.
        assert_eq!(plan(Some("a"), Some("b"), None), Plan::Conflict);
        let s = seen("a", "b");
        assert_eq!(plan(Some("a"), Some("b"), Some(&s)), Plan::Nothing);
        assert_eq!(plan(Some("x"), Some("b"), Some(&s)), Plan::Upload);
        assert_eq!(plan(Some("a"), Some("y"), Some(&s)), Plan::Download);
        assert_eq!(plan(Some("x"), Some("y"), Some(&s)), Plan::Conflict);
    }

    #[test]
    fn ids_from_names() {
        assert_eq!(parse_id("1725000000000.meta.json"), Some(1725000000000));
        assert_eq!(parse_id("1725000000000.deleted"), Some(1725000000000));
        assert_eq!(parse_id("projects.json"), None);
    }
}
