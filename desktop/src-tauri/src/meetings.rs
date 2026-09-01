//! Раздел встреч: запись длинных встреч в WAV на диске, расшифровка в два
//! прохода с таймлайном, проекты-папки, импорт чужих аудио и видео, экспорт.
//! Порт MeetingService/MeetingStore с Android, поверх — проекты, которых на
//! телефоне нет.
//!
//! По каталогу на встречу в `Application Support/.../meetings/<id>/`:
//! внутри `audio.wav`, `meta.json` и, после расшифровки, `transcript.json`.
//! Аудио остаётся и после расшифровки — можно расшифровать заново другой
//! моделью. Список проектов — `projects.json` рядом с настройками.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::wav::{WavReader, WavWriter, SAMPLE_RATE};
use crate::{cleanup, segmenter};

// --- данные ----------------------------------------------------------------

pub const STATE_RECORDED: &str = "recorded";
pub const STATE_TRANSCRIBING: &str = "transcribing";
pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Meta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub seconds: f32,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub imported: bool,
    /// Id проекта из projects.json; None — «без проекта».
    #[serde(default)]
    pub project: Option<String>,
    /// Сколько говорящих нашла диаризация; 0 — ещё не запускалась.
    #[serde(default)]
    pub speakers: u32,
    /// Имена, которые пользователь дал говорящим, по их номерам.
    #[serde(default)]
    pub names: HashMap<String, String>,
    /// Саммери от локальной языковой модели; пустая строка — не делали.
    #[serde(default)]
    pub summary: String,
    /// Почему не вышло, если не вышло: строку показывает список встреч.
    /// Молчаливая неудача — худшее, что может случиться с импортом.
    #[serde(default)]
    pub error: Option<String>,
}

/// Одна реплика таймлайна: границы в секундах от начала записи.
#[derive(Serialize, Deserialize, Clone)]
pub struct Segment {
    pub s: f32,
    pub e: f32,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spk: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// Строка списка встреч для интерфейса.
#[derive(Serialize)]
pub struct MeetingRow {
    pub id: i64,
    pub title: String,
    pub at: i64,
    pub seconds: f32,
    pub state: String,
    pub imported: bool,
    pub project: Option<String>,
    pub speakers: u32,
    /// Имена говорящих по номерам — их показывает и экспортирует окно.
    pub names: HashMap<String, String>,
    /// Почему не вышло, если не вышло.
    pub error: Option<String>,
    /// Проценты идущей работы; None — работа не идёт или без процентов.
    pub progress: Option<u8>,
    /// Скачано и всего байт, пока идёт загрузка по ссылке.
    pub fetched: Option<(u64, u64)>,
    /// Что за работа идёт: "transcribing" или "importing".
    pub phase: Option<String>,
    /// Саммери локальной модели; пустая строка — его ещё не делали.
    pub summary: String,
}

// --- состояние -------------------------------------------------------------

pub struct MeetingState {
    progress: Mutex<HashMap<i64, u8>>,
    phase: Mutex<HashMap<i64, &'static str>>,
    cancel: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// Скачано и всего байт по ссылке — окно показывает мегабайты и скорость.
    fetched: Mutex<HashMap<i64, (u64, u64)>>,
    recording_id: Mutex<Option<i64>>,
    rec_stop: Mutex<Option<Arc<AtomicBool>>>,
    rec_samples: Arc<AtomicU64>,
    rec_level: Arc<AtomicU32>,
    /// Запись на паузе: звук читается, но мимо файла.
    rec_paused: Arc<AtomicBool>,
    /// Движок один, и две расшифровки одновременно только мешают друг другу:
    /// по очереди суммарно быстрее. Ждущие висят «в очереди» — как на Android.
    engine_gate: Arc<Mutex<()>>,
}

impl MeetingState {
    pub fn new() -> Self {
        Self {
            progress: Mutex::new(HashMap::new()),
            phase: Mutex::new(HashMap::new()),
            cancel: Mutex::new(HashMap::new()),
            fetched: Mutex::new(HashMap::new()),
            recording_id: Mutex::new(None),
            rec_stop: Mutex::new(None),
            rec_samples: Arc::new(AtomicU64::new(0)),
            rec_level: Arc::new(AtomicU32::new(0)),
            rec_paused: Arc::new(AtomicBool::new(false)),
            engine_gate: Arc::new(Mutex::new(())),
        }
    }
}

fn notify(app: &AppHandle) {
    let _ = app.emit("solflow-meetings", ());
}

// --- каталоги и файлы ------------------------------------------------------

fn root(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("meetings"))
        .unwrap_or_default()
}

fn dir(app: &AppHandle, id: i64) -> PathBuf {
    root(app).join(id.to_string())
}

fn audio_file(app: &AppHandle, id: i64) -> PathBuf {
    dir(app, id).join("audio.wav")
}

fn save_meta(app: &AppHandle, id: i64, meta: &Meta) {
    let path = dir(app, id).join("meta.json");
    if path.parent().map(|p| p.exists()).unwrap_or(false) {
        let _ = std::fs::write(path, serde_json::to_string_pretty(meta).unwrap());
    }
}

fn load_meta(app: &AppHandle, id: i64) -> Option<Meta> {
    let raw = std::fs::read_to_string(dir(app, id).join("meta.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_transcript(app: &AppHandle, id: i64, segments: &[Segment]) {
    let path = dir(app, id).join("transcript.json");
    if path.parent().map(|p| p.exists()).unwrap_or(false) {
        let _ = std::fs::write(path, serde_json::to_string(segments).unwrap());
    }
}

pub fn load_transcript(app: &AppHandle, id: i64) -> Vec<Segment> {
    std::fs::read_to_string(dir(app, id).join("transcript.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn create(app: &AppHandle, imported: bool) -> Result<(i64, Meta)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let meta = Meta {
        title: String::new(),
        at: now,
        seconds: 0.0,
        state: STATE_RECORDED.to_string(),
        imported,
        project: None,
        speakers: 0,
        names: HashMap::new(),
        summary: String::new(),
        error: None,
    };
    std::fs::create_dir_all(dir(app, now))?;
    save_meta(app, now, &meta);
    Ok((now, meta))
}

/// Все встречи, новые сверху. Каталоги без meta.json — мусор, пропускаем.
pub fn rows(app: &AppHandle) -> Vec<MeetingRow> {
    let state = app.state::<MeetingState>();
    let progress = state.progress.lock().unwrap();
    let phase = state.phase.lock().unwrap();
    let fetched = state.fetched.lock().unwrap();
    let recording = *state.recording_id.lock().unwrap();

    let mut list: Vec<MeetingRow> = std::fs::read_dir(root(app))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().parse::<i64>().ok())
        .filter_map(|id| {
            let mut m = load_meta(app, id)?;
            // Приложение погибло посреди записи: звук на диске есть, а в
            // meta.json длительность так и осталась нулевой. Считаем её по
            // файлу — иначе встреча выглядит пустой и её нечем расшифровать.
            if m.seconds == 0.0 && recording.map(|r| r != id).unwrap_or(true) {
                if let Ok(wav) = WavReader::open(&audio_file(app, id)) {
                    if wav.total_samples > 0 {
                        m.seconds = wav.total_samples as f32 / SAMPLE_RATE as f32;
                        save_meta(app, id, &m);
                    }
                }
            }
            Some(MeetingRow {
                id,
                title: m.title,
                at: m.at,
                seconds: m.seconds,
                state: m.state,
                imported: m.imported,
                project: m.project,
                speakers: m.speakers,
                names: m.names,
                error: m.error,
                progress: progress.get(&id).copied(),
                fetched: fetched.get(&id).copied(),
                phase: phase.get(&id).map(|p| p.to_string()),
                summary: m.summary,
            })
        })
        .collect();
    list.sort_by_key(|m| std::cmp::Reverse(m.at));
    list
}

pub fn rename(app: &AppHandle, id: i64, title: String) {
    if let Some(mut m) = load_meta(app, id) {
        m.title = title.trim().to_string();
        save_meta(app, id, &m);
        notify(app);
    }
}

/// Имя говорящего: пустое возвращает подпись «Говорящий N».
pub fn rename_speaker(app: &AppHandle, id: i64, speaker: u32, name: String) {
    if let Some(mut m) = load_meta(app, id) {
        let name = name.trim().to_string();
        if name.is_empty() {
            m.names.remove(&speaker.to_string());
        } else {
            m.names.insert(speaker.to_string(), name);
        }
        save_meta(app, id, &m);
        notify(app);
    }
}

pub fn set_project(app: &AppHandle, id: i64, project: Option<String>) {
    if let Some(mut m) = load_meta(app, id) {
        m.project = project;
        save_meta(app, id, &m);
        notify(app);
    }
}

pub fn delete(app: &AppHandle, id: i64) {
    let state = app.state::<MeetingState>();
    if let Some(flag) = state.cancel.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = std::fs::remove_dir_all(dir(app, id));
    notify(app);
}

/// Что нашлось внутри одной записи: сколько совпадений и первые из них
/// с временем и куском текста вокруг слова.
#[derive(Serialize)]
pub struct Hit {
    pub id: i64,
    pub count: usize,
    /// До трёх фрагментов: время реплики и текст с найденным словом.
    pub quotes: Vec<(f32, String)>,
}

/// Ищет по названию и тексту расшифровки, регистр не важен. Возвращает не
/// просто список записей, а где именно нашлось: по одному совпадению
/// пролистывать всю расшифровку невозможно.
pub fn search(app: &AppHandle, query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    rows(app)
        .into_iter()
        .filter_map(|m| {
            let in_title = m.title.to_lowercase().contains(&needle);
            let segments = load_transcript(app, m.id);
            let matches: Vec<&Segment> = segments
                .iter()
                .filter(|s| s.text.to_lowercase().contains(&needle))
                .collect();
            if !in_title && matches.is_empty() {
                return None;
            }
            Some(Hit {
                id: m.id,
                count: matches.len(),
                quotes: matches
                    .iter()
                    .take(3)
                    .map(|s| (s.s, quote_around(&s.text, &needle)))
                    .collect(),
            })
        })
        .collect()
}

/// Кусок реплики вокруг найденного слова: целую реплику в списке не
/// показать, а без контекста непонятно, то ли это.
fn quote_around(text: &str, needle: &str) -> String {
    const AROUND: usize = 60;
    let lower = text.to_lowercase();
    let Some(at) = lower.find(needle) else {
        return text.chars().take(AROUND * 2).collect();
    };

    // Границы режем по символам, а не байтам: кириллица двухбайтовая.
    let before: usize = text[..at].chars().count();
    let start = before.saturating_sub(AROUND);
    let end = (before + needle.chars().count() + AROUND).min(text.chars().count());

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(text.chars().skip(start).take(end - start));
    if end < text.chars().count() {
        out.push('…');
    }
    out
}

// --- проекты ---------------------------------------------------------------

fn projects_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("projects.json"))
        .unwrap_or_default()
}

pub fn projects(app: &AppHandle) -> Vec<Project> {
    std::fs::read_to_string(projects_file(app))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_projects(app: &AppHandle, list: &[Project]) {
    if let Some(parent) = projects_file(app).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        projects_file(app),
        serde_json::to_string_pretty(list).unwrap(),
    );
}

pub fn create_project(app: &AppHandle, name: String) -> Option<Project> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let mut list = projects(app);
    let project = Project {
        id: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string(),
        name,
    };
    list.push(project.clone());
    save_projects(app, &list);
    notify(app);
    Some(project)
}

pub fn rename_project(app: &AppHandle, id: &str, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let mut list = projects(app);
    if let Some(p) = list.iter_mut().find(|p| p.id == id) {
        p.name = name;
        save_projects(app, &list);
        notify(app);
    }
}

/// Удаляет проект; его встречи становятся «без проекта», записи остаются.
pub fn delete_project(app: &AppHandle, id: &str) {
    let list: Vec<Project> = projects(app).into_iter().filter(|p| p.id != id).collect();
    save_projects(app, &list);
    for m in rows(app) {
        if m.project.as_deref() == Some(id) {
            set_project(app, m.id, None);
        }
    }
    notify(app);
}

// --- запись ----------------------------------------------------------------

#[derive(Serialize, Clone)]
struct RecEvent {
    active: bool,
    seconds: u64,
    level: f32,
    paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn emit_rec(app: &AppHandle, active: bool, seconds: u64, level: f32, error: Option<String>) {
    let paused = app
        .state::<MeetingState>()
        .rec_paused
        .load(Ordering::Relaxed);
    let _ = app.emit(
        "solflow-meetrec",
        RecEvent {
            active,
            seconds,
            // На паузе волна лежит: уровень из колбэка микрофона живой,
            // но показывать его — врать, что звук пишется.
            level: if paused { 0.0 } else { level },
            paused,
            error,
        },
    );
}

pub fn record_start(app: &AppHandle) -> Result<()> {
    let state = app.state::<MeetingState>();
    if state.recording_id.lock().unwrap().is_some() {
        return Ok(());
    }

    let (id, meta) = create(app, false)?;
    let stop = Arc::new(AtomicBool::new(false));
    *state.recording_id.lock().unwrap() = Some(id);
    *state.rec_stop.lock().unwrap() = Some(stop.clone());
    state.rec_samples.store(0, Ordering::Relaxed);
    state.rec_level.store(0f32.to_bits(), Ordering::Relaxed);
    state.rec_paused.store(false, Ordering::Relaxed);

    let handle = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = record_loop(&handle, id, stop) {
            log::error!("запись встречи: {e}");
            let state = handle.state::<MeetingState>();
            *state.recording_id.lock().unwrap() = None;
            *state.rec_stop.lock().unwrap() = None;
            // Встреча без звука в списке бессмысленна — убираем след.
            let _ = std::fs::remove_dir_all(dir(&handle, id));
            emit_rec(&handle, false, 0, 0.0, Some(format!("{e}")));
            notify(&handle);
            return;
        }
        let state = handle.state::<MeetingState>();
        *state.recording_id.lock().unwrap() = None;
        *state.rec_stop.lock().unwrap() = None;
        emit_rec(&handle, false, 0, 0.0, None);

        let seconds = state.rec_samples.load(Ordering::Relaxed) as f32 / SAMPLE_RATE as f32;
        let mut meta = load_meta(&handle, id).unwrap_or(meta);
        meta.seconds = seconds;
        save_meta(&handle, id, &meta);
        notify(&handle);
        // Расшифровка стартует сама: пользователь просил результат, а не
        // промежуточное состояние «записано, нажмите ещё раз».
        transcribe(&handle, id);
    });

    // Пульс записи для интерфейса: таймер и волна.
    let pulse = app.clone();
    std::thread::spawn(move || loop {
        let state = pulse.state::<MeetingState>();
        if state.recording_id.lock().unwrap().is_none() {
            break;
        }
        let seconds = state.rec_samples.load(Ordering::Relaxed) / SAMPLE_RATE as u64;
        let level = f32::from_bits(state.rec_level.load(Ordering::Relaxed));
        emit_rec(&pulse, true, seconds, level, None);
        std::thread::sleep(std::time::Duration::from_millis(200));
    });

    notify(app);
    Ok(())
}

pub fn record_stop(app: &AppHandle) {
    let state = app.state::<MeetingState>();
    let guard = state.rec_stop.lock().unwrap();
    if let Some(stop) = guard.as_ref() {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Пауза записи: микрофон продолжает читаться, но мимо файла — таймер и
/// волна замирают сами, а в WAV не остаётся ни стыка, ни тишины.
pub fn record_pause(app: &AppHandle) {
    let state = app.state::<MeetingState>();
    if state.recording_id.lock().unwrap().is_none() {
        return;
    }
    let paused = !state.rec_paused.load(Ordering::Relaxed);
    state.rec_paused.store(paused, Ordering::Relaxed);
    let seconds = state.rec_samples.load(Ordering::Relaxed) / SAMPLE_RATE as u64;
    emit_rec(app, true, seconds, 0.0, None);
}

/// Пишет с микрофона на диск, пока не попросят остановиться. Живёт в своём
/// треде: cpal-поток не Send, а диктовка держит собственный поток — записи
/// друг другу не мешают.
fn record_loop(app: &AppHandle, id: i64, stop: Arc<AtomicBool>) -> Result<()> {
    let state = app.state::<MeetingState>();
    let samples = state.rec_samples.clone();
    let level = state.rec_level.clone();

    let wanted = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .input_device
        .clone();
    let device = crate::audio::pick_device(&wanted).ok_or_else(|| anyhow!("микрофон не найден"))?;
    let config = device
        .default_input_config()
        .map_err(|e| anyhow!("микрофон не открылся: {e}"))?;
    let src_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;

    let (tx, rx) = channel::<Vec<f32>>();
    let cb_level = level.clone();
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mut sum = 0.0f64;
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                for frame in data.chunks_exact(channels.max(1)) {
                    let v = frame.iter().sum::<f32>() / channels.max(1) as f32;
                    mono.push(v);
                    sum += (v as f64) * (v as f64);
                }
                if !mono.is_empty() {
                    let rms = (sum / mono.len() as f64).sqrt();
                    let shown = ((rms.sqrt() * 2.2) as f32).clamp(0.0, 1.0);
                    cb_level.store(shown.to_bits(), Ordering::Relaxed);
                }
                let _ = tx.send(mono);
            },
            |e| log::error!("ошибка аудиопотока встречи: {e}"),
            None,
        )
        .map_err(|e| anyhow!("не удалось открыть поток: {e}"))?;
    stream.play().map_err(|e| anyhow!("поток не стартовал: {e}"))?;

    let mut wav = WavWriter::create(&audio_file(app, id))?;
    let mut resampler = Downsampler::new(src_rate, SAMPLE_RATE);
    let mut out = Vec::new();

    let paused = state.rec_paused.clone();
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            // На паузе куски выбрасываются: буфер не копится, файл не растёт.
            Ok(_) if paused.load(Ordering::Relaxed) => {}
            Ok(chunk) => {
                out.clear();
                resampler.feed(&chunk, &mut out);
                wav.write(&out)?;
                samples.store(wav.samples_written(), Ordering::Relaxed);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
    }

    // Гибель посреди записи не теряет звук: он уже на диске, а заголовок WAV
    // чинится по длине файла при чтении. Здесь — штатный хвост.
    drop(stream);
    while let Ok(chunk) = rx.try_recv() {
        if paused.load(Ordering::Relaxed) {
            continue;
        }
        out.clear();
        resampler.feed(&chunk, &mut out);
        wav.write(&out)?;
    }
    let written = wav.finish()?;
    samples.store(written, Ordering::Relaxed);
    level.store(0f32.to_bits(), Ordering::Relaxed);
    Ok(())
}

/// Потоковое приведение частоты к 16 кГц: кусок за куском, с переносом
/// хвоста между вызовами. Вниз — среднее по интервалу, вверх — линейная
/// интерполяция; та же математика, что в audio::resample, но позиция
/// считается дробью src/dst в целых числах. Часовая запись — это сотни
/// миллионов шагов, и накопленная ошибка f64 сдвигала бы границы окон,
/// а с ней и таймкоды таймлайна.
pub struct Downsampler {
    src: u64,
    dst: u64,
    /// Позиция во входных отсчётах, умноженная на dst.
    pos: u64,
    /// Сколько входных отсчётов уже выброшено из буфера.
    dropped: u64,
    buf: Vec<f32>,
}

impl Downsampler {
    pub fn new(src: usize, dst: usize) -> Self {
        Self {
            src: src as u64,
            dst: dst as u64,
            pos: 0,
            dropped: 0,
            buf: Vec::new(),
        }
    }

    pub fn feed(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.src == self.dst {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        let available = self.dropped + self.buf.len() as u64;

        if self.src > self.dst {
            // Вниз: среднее по интервалу — заодно фильтр от алиасинга.
            while (self.pos + self.src) / self.dst <= available {
                let from = (self.pos / self.dst - self.dropped) as usize;
                let to = (((self.pos + self.src) / self.dst - self.dropped) as usize)
                    .min(self.buf.len());
                let sum: f32 = self.buf[from..to].iter().sum();
                out.push(sum / (to - from).max(1) as f32);
                self.pos += self.src;
            }
        } else {
            // Вверх: линейная интерполяция между соседними отсчётами.
            while self.pos / self.dst + 1 < available {
                let index = self.pos / self.dst;
                let i = (index - self.dropped) as usize;
                let frac = (self.pos - index * self.dst) as f32 / self.dst as f32;
                out.push(self.buf[i] * (1.0 - frac) + self.buf[i + 1] * frac);
                self.pos += self.src;
            }
        }

        // Хвост, который ещё понадобится следующему окну, остаётся в буфере.
        let keep_from = self.pos / self.dst;
        let used = (keep_from - self.dropped) as usize;
        self.buf.drain(..used.min(self.buf.len()));
        self.dropped = keep_from;
    }
}

#[cfg(test)]
mod tests {
    use super::{Downsampler, SAMPLE_RATE};

    fn tone(src: usize, seconds: usize) -> Vec<f32> {
        (0..src * seconds)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / src as f32).sin())
            .collect()
    }

    /// Нарезка на куски не должна менять результат: если стык теряет или
    /// задваивает отсчёты, звук уезжает от таймкодов таймлайна.
    #[test]
    fn chunked_matches_single_feed() {
        let src = 44_100usize;
        let input = tone(src, 3);

        let mut whole = Vec::new();
        Downsampler::new(src, SAMPLE_RATE).feed(&input, &mut whole);

        for size in [1, 7, 1024, 4096] {
            let mut streamed = Vec::new();
            let mut down = Downsampler::new(src, SAMPLE_RATE);
            for chunk in input.chunks(size) {
                down.feed(chunk, &mut streamed);
            }
            assert_eq!(
                streamed.len(),
                whole.len(),
                "куски по {size}: длина разошлась"
            );
            for i in 0..whole.len() {
                assert!(
                    (streamed[i] - whole[i]).abs() < 1e-6,
                    "куски по {size}: отсчёт {i} разошёлся"
                );
            }
        }
    }

    /// Длина и темп совпадают с разовым ресемплером диктовки: расхождения
    /// допускаются только на краях окон усреднения, где точная позиция
    /// попадает ровно на целый отсчёт.
    #[test]
    fn keeps_pace_with_one_shot() {
        let src = 44_100usize;
        let input = tone(src, 3);

        let mut streamed = Vec::new();
        let mut down = Downsampler::new(src, SAMPLE_RATE);
        for chunk in input.chunks(1024) {
            down.feed(chunk, &mut streamed);
        }
        let once = crate::audio::resample_for_test(&input, src, SAMPLE_RATE);

        assert!(
            (streamed.len() as i64 - once.len() as i64).abs() <= 2,
            "длины разошлись: {} против {}",
            streamed.len(),
            once.len()
        );
        let n = streamed.len().min(once.len());
        let drifted = (0..n).filter(|&i| (streamed[i] - once[i]).abs() > 0.05).count();
        assert!(
            drifted * 200 < n,
            "разошлось {drifted} отсчётов из {n} — похоже на дрейф, а не на края окон"
        );
    }

    /// Повышение частоты — редкий случай (микрофон на 8 кГц), но код тот же.
    #[test]
    fn upsampling_survives_chunks() {
        let src = 8_000usize;
        let input = tone(src, 2);

        let mut whole = Vec::new();
        Downsampler::new(src, SAMPLE_RATE).feed(&input, &mut whole);

        let mut streamed = Vec::new();
        let mut down = Downsampler::new(src, SAMPLE_RATE);
        for chunk in input.chunks(333) {
            down.feed(chunk, &mut streamed);
        }
        assert_eq!(streamed.len(), whole.len());
        for i in 0..whole.len() {
            assert!((streamed[i] - whole[i]).abs() < 1e-6, "отсчёт {i} разошёлся");
        }
        // Вверх — значит выходных отсчётов должно стать вдвое больше.
        assert!(streamed.len() > input.len());
    }
}

// --- расшифровка -----------------------------------------------------------

/// Идёт ли сейчас работа хоть над одной встречей. Сторож, выгружающий
/// модель по простою, обязан это знать: расшифровка встречи для него
/// выглядела бездействием, и он вынимал модель прямо из-под неё — отсюда
/// «модель не загружена» посреди двухчасовой записи.
pub fn busy(app: &AppHandle) -> bool {
    !app.state::<MeetingState>().phase.lock().unwrap().is_empty()
}

pub fn transcribe(app: &AppHandle, id: i64) {
    let state = app.state::<MeetingState>();
    {
        let mut cancel = state.cancel.lock().unwrap();
        if cancel.contains_key(&id) {
            return;
        }
        cancel.insert(id, Arc::new(AtomicBool::new(false)));
    }
    state.progress.lock().unwrap().insert(id, 0);
    state.phase.lock().unwrap().insert(id, "queued");
    notify(app);

    let app = app.clone();
    std::thread::spawn(move || {
        // Очередь: движок отдаётся расшифровкам по одной. Пока чужая идёт,
        // эта висит с подписью «в очереди», и её можно отменить.
        let gate = app.state::<MeetingState>().engine_gate.clone();
        let _turn = gate.lock().unwrap();

        let state = app.state::<MeetingState>();
        let cancelled_in_queue = state
            .cancel
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(true);
        let result = if cancelled_in_queue {
            Ok(())
        } else {
            state.phase.lock().unwrap().insert(id, "transcribing");
            notify(&app);
            transcribe_job(&app, id)
        };
        let state = app.state::<MeetingState>();
        let cancelled = state
            .cancel
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);
        state.cancel.lock().unwrap().remove(&id);
        state.progress.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().remove(&id);

        if let Err(e) = result {
            log::error!("расшифровка не удалась: {e}");
            if !cancelled {
                if let Some(mut m) = load_meta(&app, id) {
                    m.state = STATE_FAILED.to_string();
                    // Причину храним рядом: «не удалось расшифровать» без
                    // объяснения — это тупик и для человека, и для разбора.
                    m.error = Some(e.to_string());
                    save_meta(&app, id, &m);
                }
            }
        }
        notify(&app);
    });
}

fn transcribe_job(app: &AppHandle, id: i64) -> Result<()> {
    let engine = app.state::<crate::AppState>().engine.clone();
    // Модель могла быть выгружена по таймеру простоя — расшифровка обязана
    // поднять её сама, а не ждать, пока это сделает диктовка (проверено
    // падением «модель не загружена» после долгой работы без диктовки).
    crate::ensure_model_loaded(app.clone());
    // Загрузка идёт в фоне — подождём её.
    for _ in 0..60 {
        if engine.is_loaded() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    if !engine.is_loaded() {
        return Err(anyhow!("модель не загружена"));
    }

    // Флаг отмены снимаем на входе: если прошлую встречу отменили ровно на
    // границе куска, поднятый флаг достался бы этой — и она упала бы на
    // первом же куске «сама собой».
    engine.clear_cancel();

    let mut meta = load_meta(app, id).ok_or_else(|| anyhow!("встреча пропала"))?;
    let mut wav = WavReader::open(&audio_file(app, id))?;
    let sr = SAMPLE_RATE as u64;
    let frame = segmenter::frame_samples(SAMPLE_RATE);
    let total = wav.total_samples;
    if total < sr / 2 {
        return Err(anyhow!("запись пустая"));
    }

    meta.state = STATE_TRANSCRIBING.to_string();
    save_meta(app, id, &meta);
    notify(app);

    let state = app.state::<MeetingState>();
    let cancelled = || {
        state
            .cancel
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(true)
    };

    // Первый проход: энергии кадров всего файла. Для двух часов это 360
    // тысяч float — копейки по сравнению с самим звуком.
    let frames = (total / frame as u64) as usize;
    let mut loud = Vec::with_capacity(frames);
    let block = frame * 500;
    let mut offset = 0u64;
    while loud.len() < frames {
        if cancelled() {
            return Ok(());
        }
        let pcm = wav.read(offset, block)?;
        if pcm.is_empty() {
            break;
        }
        let mut o = 0usize;
        while o + frame <= pcm.len() && loud.len() < frames {
            loud.push(segmenter::frame_energy(&pcm, o, frame));
            o += frame;
        }
        offset += o as u64;
    }

    let max_single = (segmenter::MAX_SEGMENT_SEC * SAMPLE_RATE as f32) as u64;
    let cuts = if total <= max_single {
        Vec::new()
    } else {
        segmenter::cut_frames(&loud)
    };
    let mut bounds = vec![0u64];
    bounds.extend(cuts.iter().map(|&c| c as u64 * frame as u64));
    bounds.push(frames as u64 * frame as u64);
    let ranges: Vec<(u64, u64)> = bounds
        .windows(2)
        .map(|w| (w[0], w[1].min(total)))
        .filter(|(a, b)| b.saturating_sub(*a) > (frame * 5) as u64)
        .collect();

    // Второй проход: куски читаются с диска и распознаются по одному.
    // Частичный результат сохраняется после каждого куска — обрыв на
    // середине не выбрасывает уже готовый текст.
    let mut segments: Vec<Segment> = Vec::new();
    for (index, (from, to)) in ranges.iter().enumerate() {
        if cancelled() {
            engine.clear_cancel();
            return Ok(());
        }
        let pcm = wav.read(*from, (*to - *from) as usize)?;
        // Брошенный по отмене кусок возвращает ошибку — это не поломка, а
        // ровно то, чего просил человек.
        let text = match engine.transcribe_segment(&pcm) {
            Ok(text) => cleanup::clean(&text),
            Err(e) => {
                engine.clear_cancel();
                if cancelled() {
                    return Ok(());
                }
                return Err(e);
            }
        };
        if !text.is_empty() {
            segments.push(Segment {
                s: *from as f32 / sr as f32,
                e: *to as f32 / sr as f32,
                text,
                spk: None,
            });
            save_transcript(app, id, &segments);
        }

        // Модель только что работала — сдвигаем счётчик простоя.
        *app.state::<crate::AppState>().last_used.lock().unwrap() = std::time::Instant::now();

        let pct = (((index + 1) * 100) / ranges.len().max(1)).min(100) as u8;
        let changed = state.progress.lock().unwrap().insert(id, pct) != Some(pct);
        if changed {
            notify(app);
        }
    }

    meta.state = STATE_DONE.to_string();
    meta.error = None;

    // Автоназвание: безымянной встрече — короткое имя от модели саммери,
    // если та уже скачана (качать гигабайты ради названия не станем).
    // Ошибка названия не должна валить готовую расшифровку — молча живём
    // с «Встречей 1 сентября».
    if meta.title.trim().is_empty() && crate::summary::model_ready(app) {
        let head: String = {
            let mut text = String::new();
            for s in &segments {
                text.push_str(&s.text);
                text.push(' ');
                if text.len() > 6000 {
                    break;
                }
            }
            text
        };
        match crate::summary::title(app, &head) {
            Ok(title) if !title.is_empty() => meta.title = title,
            Ok(_) => {}
            Err(e) => log::warn!("автоназвание не удалось: {e}"),
        }
    }

    save_meta(app, id, &meta);
    Ok(())
}

// --- саммери ----------------------------------------------------------------

/// Саммери готовой встречи локальной моделью. Если модель ещё не скачана,
/// сначала тянем её (интерфейс предупреждает о размере заранее). Очередь
/// та же, что у расшифровок: движки тяжёлые, пусть ходят по одному.
pub fn summarize(app: &AppHandle, id: i64) {
    let state = app.state::<MeetingState>();
    {
        let mut cancel = state.cancel.lock().unwrap();
        if cancel.contains_key(&id) {
            return;
        }
        cancel.insert(id, Arc::new(AtomicBool::new(false)));
    }
    state.progress.lock().unwrap().insert(id, 0);
    state.phase.lock().unwrap().insert(id, "queued");
    notify(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let gate = app.state::<MeetingState>().engine_gate.clone();
        let _turn = gate.lock().unwrap();

        let state = app.state::<MeetingState>();
        let flag = state
            .cancel
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));

        let result = if flag.load(Ordering::Relaxed) {
            Ok(())
        } else {
            summarize_job(&app, id, flag.clone())
        };

        let state = app.state::<MeetingState>();
        let cancelled = flag.load(Ordering::Relaxed);
        state.cancel.lock().unwrap().remove(&id);
        state.progress.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().remove(&id);

        if let Err(e) = result {
            log::error!("саммери не удалось: {e}");
            if !cancelled {
                use tauri::Emitter;
                let _ = app.emit("solflow-summary-error", format!("{e}"));
            }
        }
        notify(&app);
    });
}

fn summarize_job(app: &AppHandle, id: i64, flag: Arc<AtomicBool>) -> Result<()> {
    let state = app.state::<MeetingState>();

    if !crate::summary::model_ready(app) {
        state.phase.lock().unwrap().insert(id, "llm_downloading");
        notify(app);
        crate::summary::download(
            app,
            &|pct| {
                let state = app.state::<MeetingState>();
                state.progress.lock().unwrap().insert(id, pct);
                notify(app);
            },
            &|| flag.load(Ordering::Relaxed),
        )?;
    }

    state.phase.lock().unwrap().insert(id, "summarizing");
    state.progress.lock().unwrap().insert(id, 0);
    notify(app);

    let segments = load_transcript(app, id);
    if segments.is_empty() {
        return Err(anyhow!("расшифровки ещё нет"));
    }
    let text: String = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let progress_app = app.clone();
    let summary = crate::summary::summarize(
        app,
        &text,
        move |pct| {
            let state = progress_app.state::<MeetingState>();
            state.progress.lock().unwrap().insert(id, pct);
            notify(&progress_app);
        },
        flag,
    )?;

    let mut meta = load_meta(app, id).ok_or_else(|| anyhow!("встреча пропала"))?;
    meta.summary = summary;
    save_meta(app, id, &meta);
    Ok(())
}

// --- разделение говорящих --------------------------------------------------

/// Диаризация готовой встречи: если модель голосов ещё не скачана, сначала
/// тянем её (второй раз уже не понадобится), потом гоним разбор. Встреча всё
/// это время остаётся готовой — таймлайн и экспорт живут, прогресс рисуется
/// поверх.
pub fn diarize(app: &AppHandle, id: i64, num_speakers: i32) {
    let state = app.state::<MeetingState>();
    {
        let mut cancel = state.cancel.lock().unwrap();
        if cancel.contains_key(&id) {
            return;
        }
        cancel.insert(id, Arc::new(AtomicBool::new(false)));
    }
    state.progress.lock().unwrap().insert(id, 0);
    state.phase.lock().unwrap().insert(
        id,
        if crate::diarize::models_ready(app) {
            "diarizing"
        } else {
            "downloading"
        },
    );
    notify(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let result = diarize_job(&app, id, num_speakers);
        let state = app.state::<MeetingState>();
        state.cancel.lock().unwrap().remove(&id);
        state.progress.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().remove(&id);

        match result {
            Ok(speakers) => {
                if let Some(mut m) = load_meta(&app, id) {
                    m.speakers = speakers as u32;
                    save_meta(&app, id, &m);
                }
            }
            Err(e) => {
                log::error!("диаризация не удалась: {e}");
                let _ = app.emit("solflow-diarize-failed", format!("{e}"));
            }
        }
        notify(&app);
    });
}

fn diarize_job(app: &AppHandle, id: i64, num_speakers: i32) -> Result<usize> {
    let state = app.state::<MeetingState>();
    let cancelled = || {
        state
            .cancel
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(true)
    };
    let report = |pct: u8| {
        let changed = state.progress.lock().unwrap().insert(id, pct) != Some(pct);
        if changed {
            notify(app);
        }
    };

    if !crate::diarize::models_ready(app) {
        crate::diarize::download(app, &report, &cancelled)?;
        state.phase.lock().unwrap().insert(id, "diarizing");
        report(0);
        notify(app);
    }

    let segments = load_transcript(app, id);
    if segments.is_empty() {
        return Err(anyhow!("расшифровки нет"));
    }
    let bounds: Vec<(f32, f32)> = segments.iter().map(|s| (s.s, s.e)).collect();

    let speakers = crate::diarize::run(
        app,
        &audio_file(app, id),
        &bounds,
        num_speakers,
        &report,
        &cancelled,
    )?;

    let labelled: Vec<Segment> = segments
        .into_iter()
        .zip(speakers.iter())
        .map(|(s, &spk)| Segment {
            spk: Some(spk as u32),
            ..s
        })
        .collect();
    let count = speakers.iter().max().map(|m| m + 1).unwrap_or(0);
    save_transcript(app, id, &labelled);
    Ok(count)
}

// --- импорт ----------------------------------------------------------------

/// Диалог выбора файла — системный, через plugin-dialog: osascript есть
/// только на Mac. Вызывать с отдельного потока: blocking_* ждёт ответа
/// пользователя и на главном потоке заблокировал бы окно.
pub fn pick_import_file(app: &AppHandle) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_title("Аудио или видео встречи")
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
}

/// Импорт по ссылке. Скачивание идёт в каталоге встречи, чтобы файл не
/// пришлось никуда переносить, а название берётся из источника.
pub fn import_url(app: &AppHandle, url: String) -> Result<()> {
    let (id, mut meta) = create(app, true)?;
    let state = app.state::<MeetingState>();
    state.cancel.lock().unwrap().insert(id, Arc::new(AtomicBool::new(false)));
    state.phase.lock().unwrap().insert(id, "fetching");
    notify(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let cancelled = || {
            let state = app.state::<MeetingState>();
            let flag = state.cancel.lock().unwrap().get(&id).cloned();
            flag.map(|f| f.load(Ordering::Relaxed)).unwrap_or(true)
        };
        let report = |done: u64, total: u64| {
            let state = app.state::<MeetingState>();
            state.fetched.lock().unwrap().insert(id, (done, total));
            let pct = if total > 0 {
                ((done * 100 / total).min(99)) as u8
            } else {
                0
            };
            state.progress.lock().unwrap().insert(id, pct);
            notify(&app);
        };
        let progress = crate::fetch::Progress {
            report: &report,
            cancelled: &cancelled,
        };

        let result = crate::fetch::fetch(&url, &dir(&app, id), &progress).and_then(
            |(file, title)| {
                meta.title = clean_title(&title, &meta.title);
                save_meta(&app, id, &meta);
                let state = app.state::<MeetingState>();
                state.phase.lock().unwrap().insert(id, "importing");
                state.fetched.lock().unwrap().remove(&id);
                state.progress.lock().unwrap().remove(&id);
                notify(&app);

                let outcome = import_job(&app, id, &file, meta.clone());
                keep_or_drop_source(&app, &file, &meta.title);
                outcome
            },
        );

        let state = app.state::<MeetingState>();
        state.cancel.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().remove(&id);
        state.fetched.lock().unwrap().remove(&id);
        state.progress.lock().unwrap().remove(&id);
        match result {
            Ok(()) => transcribe(&app, id),
            Err(e) => {
                log::error!("ссылка не пошла: {e}");
                // Встречу не удаляем: строка с причиной — единственный
                // способ узнать, что пошло не так, не открывая логи.
                let _ = std::fs::remove_file(audio_file(&app, id));
                mark_failed(&app, id, &e.to_string());
                let _ = app.emit("solflow-import-failed", format!("{e}"));
            }
        }
        notify(&app);
    });
    Ok(())
}

/// Скачанный исходник либо переезжает в папку из настроек, либо удаляется:
/// для расшифровки он больше не нужен, а весит порой гигабайты.
fn keep_or_drop_source(app: &AppHandle, file: &Path, title: &str) {
    let keep = app
        .state::<crate::AppState>()
        .settings
        .lock()
        .unwrap()
        .downloads_dir
        .clone();
    let Some(dir) = keep.filter(|d| !d.is_empty()) else {
        let _ = std::fs::remove_file(file);
        return;
    };

    let dir = PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "media".to_string());
    let safe: String = title
        .chars()
        .map(|c| match c {
            ':' | '/' | '\\' => '.',
            c => c,
        })
        .collect();

    let mut target = dir.join(format!("{safe}.{ext}"));
    let mut counter = 2;
    while target.exists() {
        target = dir.join(format!("{safe} {counter}.{ext}"));
        counter += 1;
    }
    if std::fs::rename(file, &target).is_err() {
        // Через границу тома rename не работает — копируем и убираем.
        if std::fs::copy(file, &target).is_ok() {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Остановить работу над встречей: загрузку, импорт, расшифровку.
pub fn cancel(app: &AppHandle, id: i64) {
    let state = app.state::<MeetingState>();
    let phase = state.phase.lock().unwrap().get(&id).copied();
    if let Some(flag) = state.cancel.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    // Кусок в двадцать четыре секунды на медленной машине считается минуту, и
    // всё это время флага никто не видит. Движку говорим отдельно — он
    // бросает работу между шагами декодера.
    if phase == Some("transcribing") {
        app.state::<crate::AppState>().engine.request_cancel();
    }
    notify(app);
}

pub fn import(app: &AppHandle, source: PathBuf) -> Result<()> {
    let (id, mut meta) = create(app, true)?;
    // Имя файла становится названием встречи: по нему её и ищут потом,
    // «Импорт 26 августа» ни о чём не говорит.
    if let Some(name) = source.file_stem().map(|s| s.to_string_lossy().to_string()) {
        meta.title = clean_title(&name, &meta.title);
        save_meta(app, id, &meta);
    }
    let state = app.state::<MeetingState>();
    state.cancel.lock().unwrap().insert(id, Arc::new(AtomicBool::new(false)));
    state.phase.lock().unwrap().insert(id, "importing");
    notify(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let result = import_job(&app, id, &source, meta);
        let state = app.state::<MeetingState>();
        state.cancel.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().remove(&id);
        match result {
            // Расшифровка запускается здесь, а не в конце import_job:
            // пока импорт числится в работах, transcribe считает, что
            // работа уже идёт, и молча выходит.
            Ok(()) => transcribe(&app, id),
            Err(e) => {
                log::error!("импорт не удался: {e}");
                mark_failed(&app, id, &e.to_string());
                // Встреча без звука в списке бессмысленна — убираем след.
                let _ = std::fs::remove_dir_all(dir(&app, id));
                let _ = app.emit("solflow-import-failed", format!("{e}"));
            }
        }
        notify(&app);
    });
    Ok(())
}

fn import_job(app: &AppHandle, id: i64, source: &Path, mut meta: Meta) -> Result<()> {
    let target = audio_file(app, id);

    to_wav_16k(app, id, source, &target)?;

    let wav = WavReader::open(&target)?;
    if wav.total_samples < SAMPLE_RATE as u64 / 2 {
        return Err(anyhow!("в файле не нашлось звука"));
    }
    meta.seconds = wav.total_samples as f32 / SAMPLE_RATE as f32;
    save_meta(app, id, &meta);
    notify(app);
    Ok(())
}

/// Приводит любой звук или видео к 16 кГц моно WAV — тому виду, в котором
/// работает движок.
///
/// На macOS этим занимаются встроенные утилиты: afconvert читает всё, что
/// умеет CoreAudio (wav, m4a, mp3, aiff...), а видео он не берёт — тогда
/// avconvert вытаскивает дорожку в m4a, и afconvert дожимает её. Цепочка
/// проверена руками.
#[cfg(target_os = "macos")]
fn to_wav_16k(app: &AppHandle, id: i64, source: &Path, target: &Path) -> Result<()> {
    let direct = convert_ok(
        "/usr/bin/afconvert",
        &[
            "-f",
            "WAVE",
            "-d",
            "LEI16@16000",
            "-c",
            "1",
            &source.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    );
    if direct {
        return Ok(());
    }

    let m4a = dir(app, id).join("import.m4a");
    if !convert_ok(
        "/usr/bin/avconvert",
        &[
            "--preset",
            "PresetAppleM4A",
            "--source",
            &source.to_string_lossy(),
            "--output",
            &m4a.to_string_lossy(),
            "--replace",
        ],
    ) {
        return Err(anyhow!("файл не читается как аудио или видео"));
    }
    let ok = convert_ok(
        "/usr/bin/afconvert",
        &[
            "-f",
            "WAVE",
            "-d",
            "LEI16@16000",
            "-c",
            "1",
            &m4a.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    );
    let _ = std::fs::remove_file(&m4a);
    if !ok {
        return Err(anyhow!("не удалось привести звук к нужному формату"));
    }
    Ok(())
}

/// На Windows встроенного конвертера нет — работу делает ffmpeg, который
/// приложение качает себе в настройках. Одной командой: он и видео берёт,
/// и в нужный формат кладёт сразу.
#[cfg(windows)]
fn to_wav_16k(app: &AppHandle, id: i64, source: &Path, target: &Path) -> Result<()> {
    // Первый импорт докачивает ffmpeg — как первая диаризация докачивает
    // модель голосов. Проценты идут в строку встречи: молчащая строка на
    // восьмидесяти мегабайтах выглядит как зависшая.
    if !crate::tools::converter_ready() {
        let state = app.state::<MeetingState>();
        state.phase.lock().unwrap().insert(id, "helper");
        state.progress.lock().unwrap().insert(id, 0);
        notify(app);

        let report = |pct: u8| {
            let state = app.state::<MeetingState>();
            let changed = state.progress.lock().unwrap().insert(id, pct) != Some(pct);
            if changed {
                notify(app);
            }
        };
        let result = crate::tools::ensure_ffmpeg(&report);

        let state = app.state::<MeetingState>();
        state.progress.lock().unwrap().remove(&id);
        state.phase.lock().unwrap().insert(id, "importing");
        notify(app);
        result?;
    }

    let ffmpeg = crate::tools::ffmpeg()
        .ok_or_else(|| anyhow!("ffmpeg не нашёлся — поставьте его в настройках"))?;
    let ok = convert_ok(
        &ffmpeg.to_string_lossy(),
        &[
            "-y",
            "-i",
            &source.to_string_lossy(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            &target.to_string_lossy(),
        ],
    );
    if !ok {
        return Err(anyhow!("файл не читается как аудио или видео"));
    }
    Ok(())
}

/// Чистит название от следов чужой кодировки. Ромбик U+FFFD появляется,
/// когда чей-то вывод пришёл не в UTF-8; лучше короткое название без него,
/// чем строка из ромбиков.
fn clean_title(title: &str, fallback: &str) -> String {
    let cleaned = title.replace('\u{FFFD}', "").trim().to_string();
    let letters = cleaned.chars().filter(|c| c.is_alphanumeric()).count();
    if letters == 0 {
        fallback.to_string()
    } else {
        cleaned
    }
}

/// Помечает встречу неудавшейся и запоминает причину.
fn mark_failed(app: &AppHandle, id: i64, reason: &str) {
    if let Some(mut meta) = load_meta(app, id) {
        meta.state = "failed".to_string();
        meta.error = Some(reason.to_string());
        save_meta(app, id, &meta);
    }
    notify(app);
}

fn convert_ok(bin: &str, args: &[&str]) -> bool {
    crate::sys::command(bin)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// --- экспорт ---------------------------------------------------------------

/// «12:34» или «1:02:34» после часа — метка времени в таймлайне.
fn clock_label(seconds: f32) -> String {
    let total = seconds as u64;
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// «1 ч 42 мин» или «12 мин» — длительность для шапки файла.
fn duration_label(seconds: f32) -> String {
    let total = seconds as u64;
    let (h, m) = (total / 3600, total % 3600 / 60);
    match (h, m) {
        (0, 0) => format!("{total} с"),
        (0, m) => format!("{m} мин"),
        (h, m) => format!("{h} ч {m} мин"),
    }
}

/// Подпись говорящего на смене голоса, как в пьесе; None — голос тот же.
/// Если пользователь дал человеку имя, в файл идёт имя.
fn speaker_at(segments: &[Segment], index: usize, names: &HashMap<String, String>) -> Option<String> {
    let spk = segments[index].spk?;
    if index > 0 && segments[index - 1].spk == Some(spk) {
        return None;
    }
    Some(speaker_label(spk, names))
}

pub fn speaker_label(spk: u32, names: &HashMap<String, String>) -> String {
    names
        .get(&spk.to_string())
        .filter(|n| !n.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("Говорящий {}", spk + 1))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Строки саммери для экспорта: (это заголовок?, текст) без markdown-меток.
pub fn summary_lines(summary: &str) -> Vec<(bool, String)> {
    summary
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            if let Some(h) = l.strip_prefix("##") {
                Some((true, h.trim_start_matches('#').trim().to_string()))
            } else if let Some(b) = l.strip_prefix("- ").or_else(|| l.strip_prefix("• ")) {
                Some((false, format!("• {b}")))
            } else {
                Some((false, l.to_string()))
            }
        })
        .collect()
}

pub fn as_text(title: &str, duration: &str, summary: &str, segments: &[Segment], names: &HashMap<String, String>) -> String {
    let mut out = format!("{title}\n{duration}\n\n");
    for (head, line) in summary_lines(summary) {
        if head {
            out.push('\n');
        }
        out.push_str(&line);
        out.push('\n');
    }
    if !summary.is_empty() {
        out.push('\n');
    }
    for (i, s) in segments.iter().enumerate() {
        if let Some(name) = speaker_at(segments, i, names) {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&name);
            out.push('\n');
        }
        out.push_str(&format!("{}  {}\n", clock_label(s.s), s.text));
    }
    out
}

pub fn as_markdown(title: &str, duration: &str, summary: &str, segments: &[Segment], names: &HashMap<String, String>) -> String {
    let mut out = format!("# {title}\n\n*{duration}*\n\n");
    if !summary.is_empty() {
        out.push_str(summary.trim());
        out.push_str("\n\n---\n\n");
    }
    for (i, s) in segments.iter().enumerate() {
        if let Some(name) = speaker_at(segments, i, names) {
            out.push_str(&format!("## {name}\n\n"));
        }
        out.push_str(&format!("**{}** {}\n\n", clock_label(s.s), s.text));
    }
    out
}

/// HTML — промежуточный формат для docx: textutil переносит из него
/// размеры, начертания и цвета, так что документ выходит свёрстанным,
/// а не голым текстом.
pub fn as_html(title: &str, duration: &str, summary: &str, segments: &[Segment], names: &HashMap<String, String>) -> String {
    let mut out = String::from(
        "<html><head><meta charset=\"utf-8\"></head>\
         <body style=\"font-family:Helvetica,Arial,sans-serif\">",
    );
    out.push_str(&format!(
        "<h1 style=\"font-size:18pt;font-weight:500\">{}</h1>\
         <p style=\"color:#8a8a8a;font-size:10pt\">{}</p>",
        esc(title),
        esc(duration)
    ));
    for (head, line) in summary_lines(summary) {
        if head {
            out.push_str(&format!(
                "<p style=\"font-size:12pt;font-weight:500;margin-top:14pt\">{}</p>",
                esc(&line)
            ));
        } else {
            out.push_str(&format!("<p style=\"font-size:11pt\">{}</p>", esc(&line)));
        }
    }
    for (i, s) in segments.iter().enumerate() {
        if let Some(name) = speaker_at(segments, i, names) {
            out.push_str(&format!(
                "<p style=\"font-size:12pt;font-weight:500;margin-top:16pt\">{}</p>",
                esc(&name)
            ));
        }
        out.push_str(&format!(
            "<p style=\"font-size:11pt\">\
             <span style=\"color:#8a8a8a;font-size:9pt\">{}&nbsp;&nbsp;</span>{}</p>",
            esc(&clock_label(s.s)),
            esc(&s.text)
        ));
    }
    out.push_str("</body></html>");
    out
}

/// docx собирается своим генератором: textutil, который делал это раньше,
/// есть только на macOS.
pub fn as_docx(
    title: &str,
    duration: &str,
    summary: &str,
    segments: &[Segment],
    names: &HashMap<String, String>,
) -> Vec<u8> {
    crate::docx::build(
        title,
        duration,
        summary,
        segments,
        names,
        &|i| speaker_at(segments, i, names),
        &clock_label,
    )
}

/// PDF собирается своим генератором со встроенным Inter — вид тот же, что
/// в окне приложения и в экспорте на Android.
pub fn as_pdf(title: &str, duration: &str, summary: &str, segments: &[Segment], names: &HashMap<String, String>) -> Result<Vec<u8>> {
    use crate::pdf::{Block, Document, Face};

    let regular = crate::inter_regular();
    let medium = crate::inter_medium();
    let mut doc = Document::new(regular, medium);

    doc.add(&Block::new(title, Face::Medium, 18.0));
    doc.add(&Block::new(duration, Face::Regular, 10.0).gray().gap(2.0));
    for (head, line) in summary_lines(summary) {
        if head {
            doc.add(&Block::new(line, Face::Medium, 12.0).gap(14.0));
        } else {
            doc.add(&Block::new(line, Face::Regular, 11.0).gap(4.0));
        }
    }

    for (i, s) in segments.iter().enumerate() {
        if let Some(name) = speaker_at(segments, i, names) {
            doc.add(&Block::new(name, Face::Medium, 12.0).gap(18.0));
        }
        // Метка времени слева в своей колонке, текст — с отступом под неё.
        doc.add_row(&clock_label(s.s), &s.text, 11.5, 62.0);
    }
    Ok(doc.finish())
}

/// Экспорт в Загрузки; заголовок приходит из интерфейса — там же, где
/// собирается «Встреча 27 августа». Возвращает путь готового файла.
///
/// txt и md пишутся напрямую, docx собирает системный textutil из HTML,
/// pdf — свой генератор.
/// Название встречи, пригодное для имени файла: двоеточия и слэши в именах
/// не живут ни на одной из систем.
pub fn safe_file_name(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            ':' => '.',
            '\\' | '/' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c => c,
        })
        .collect()
}

/// Куда сохранять экспорт.
pub enum Target {
    /// Как выбрано в настройках: папка оттуда или «Загрузки».
    AsSettings,
    /// Конкретная папка — её выбрали в диалоге на эту выгрузку.
    Dir(PathBuf),
    /// Конкретный файл — его выбрали в диалоге вместе с именем.
    File(PathBuf),
}

/// Папка и свободное имя для файла экспорта — общее для одиночного и
/// склееного: папка из настроек или «Загрузки», «имя 2» при занятом имени.
/// Пропавшую папку (флешку вынули) молча заменяем «Загрузками», иначе
/// экспорт упал бы вместо того, чтобы сохраниться.
fn export_target_path(app: &AppHandle, target: Target, safe: &str, format: &str) -> Result<PathBuf> {
    let ext = match format {
        "md" | "docx" | "pdf" | "wav" => format,
        _ => "txt",
    };
    let chosen = match &target {
        Target::Dir(dir) => Some(dir.clone()),
        _ => app
            .state::<crate::AppState>()
            .settings
            .lock()
            .unwrap()
            .export_dir
            .clone()
            .map(PathBuf::from),
    }
    .filter(|dir| dir.is_dir());
    let downloads = match chosen {
        Some(dir) => dir,
        None => app
            .path()
            .download_dir()
            .map_err(|_| anyhow!("папка Загрузки не нашлась"))?,
    };

    // Выбранный в диалоге файл берём как есть: человек уже решил и про имя,
    // и про перезапись. В остальных случаях не перезаписываем чужое:
    // «имя 2», «имя 3»...
    let chosen_file = matches!(target, Target::File(_));
    let mut path = match target {
        Target::File(path) => path,
        _ => downloads.join(format!("{safe}.{ext}")),
    };
    if path.extension().is_none() {
        path.set_extension(ext);
    }
    if !chosen_file {
        let mut counter = 2;
        while path.exists() {
            path = downloads.join(format!("{safe} {counter}.{ext}"));
            counter += 1;
        }
    }
    Ok(path)
}

/// Несколько встреч одним файлом: каждая начинается со своего заголовка,
/// в PDF и Word — со своей страницы. Встречи без расшифровки пропускаются.
pub fn export_combined(
    app: &AppHandle,
    items: &[(i64, String)],
    format: &str,
    title: &str,
    target: Target,
) -> Result<String> {
    let mut parts: Vec<(String, String, String, Vec<Segment>, HashMap<String, String>)> =
        Vec::new();
    for (id, item_title) in items {
        let Some(meta) = load_meta(app, *id) else { continue };
        let segments = load_transcript(app, *id);
        if segments.is_empty() {
            continue;
        }
        parts.push((
            item_title.clone(),
            duration_label(meta.seconds),
            meta.summary,
            segments,
            meta.names,
        ));
    }
    if parts.is_empty() {
        return Err(anyhow!("расшифровки ещё нет"));
    }

    let path = export_target_path(app, target, &safe_file_name(title), format)?;

    let bytes: Vec<u8> = match format {
        "md" => {
            let mut out = String::new();
            for (n, (t, d, summary, segs, names)) in parts.iter().enumerate() {
                if n > 0 {
                    out.push_str("---\n\n");
                }
                out.push_str(&as_markdown(t, d, summary, segs, names));
            }
            out.into_bytes()
        }
        "docx" => {
            let closures: Vec<Box<dyn Fn(usize) -> Option<String>>> = parts
                .iter()
                .map(|(_, _, _, segs, names)| {
                    let segs = segs.clone();
                    let names = names.clone();
                    Box::new(move |i: usize| speaker_at(&segs, i, &names))
                        as Box<dyn Fn(usize) -> Option<String>>
                })
                .collect();
            let sections: Vec<crate::docx::Section> = parts
                .iter()
                .zip(&closures)
                .map(|((t, d, summary, segs, _), c)| crate::docx::Section {
                    title: t,
                    duration: d,
                    summary,
                    segments: segs,
                    speaker_at: c.as_ref(),
                })
                .collect();
            crate::docx::build_many(&sections, &clock_label)
        }
        "pdf" => {
            use crate::pdf::{Block, Document, Face};
            let regular = crate::inter_regular();
            let medium = crate::inter_medium();
            let mut doc = Document::new(regular, medium);
            for (n, (t, d, summary, segs, names)) in parts.iter().enumerate() {
                if n > 0 {
                    doc.page_break();
                }
                doc.add(&Block::new(t.as_str(), Face::Medium, 18.0));
                doc.add(&Block::new(d.as_str(), Face::Regular, 10.0).gray().gap(2.0));
                for (head, line) in summary_lines(summary) {
                    if head {
                        doc.add(&Block::new(line, Face::Medium, 12.0).gap(14.0));
                    } else {
                        doc.add(&Block::new(line, Face::Regular, 11.0).gap(4.0));
                    }
                }
                for (i, s) in segs.iter().enumerate() {
                    if let Some(name) = speaker_at(segs, i, names) {
                        doc.add(&Block::new(name, Face::Medium, 12.0).gap(18.0));
                    }
                    doc.add_row(&clock_label(s.s), &s.text, 11.5, 62.0);
                }
            }
            doc.finish()
        }
        _ => {
            let mut out = String::new();
            for (n, (t, d, summary, segs, names)) in parts.iter().enumerate() {
                if n > 0 {
                    out.push_str("\n————————\n\n");
                }
                out.push_str(&as_text(t, d, summary, segs, names));
            }
            out.into_bytes()
        }
    };
    std::fs::write(&path, bytes)?;
    crate::sys::reveal_file(&path);
    Ok(path.to_string_lossy().to_string())
}

/// `reveal` — показать ли файл в проводнике. При выгрузке пачкой его
/// выключают: иначе на каждую встречу открылось бы своё окно.
pub fn export(
    app: &AppHandle,
    id: i64,
    format: &str,
    title: &str,
    reveal: bool,
    target: Target,
) -> Result<String> {
    // Сам звук — отдельная ветка: он есть и у нерасшифрованной встречи,
    // и собирать ему нечего — WAV копируется как лежит.
    if format == "wav" {
        let source = audio_file(app, id);
        if !source.exists() {
            return Err(anyhow!("звука нет"));
        }
        let path = export_target_path(app, target, &safe_file_name(title), "wav")?;
        std::fs::copy(&source, &path)?;
        if reveal {
            crate::sys::reveal_file(&path);
        }
        return Ok(path.to_string_lossy().to_string());
    }

    let meta = load_meta(app, id).ok_or_else(|| anyhow!("встреча пропала"))?;
    let segments = load_transcript(app, id);
    if segments.is_empty() {
        return Err(anyhow!("расшифровки ещё нет"));
    }
    let duration = duration_label(meta.seconds);
    let path = export_target_path(app, target, &safe_file_name(title), format)?;

    match format {
        "md" => std::fs::write(
            &path,
            as_markdown(title, &duration, &meta.summary, &segments, &meta.names),
        )?,
        "docx" => std::fs::write(
            &path,
            as_docx(title, &duration, &meta.summary, &segments, &meta.names),
        )?,
        "pdf" => std::fs::write(
            &path,
            as_pdf(title, &duration, &meta.summary, &segments, &meta.names)?,
        )?,
        _ => std::fs::write(
            &path,
            as_text(title, &duration, &meta.summary, &segments, &meta.names),
        )?,
    }

    // Показать файл в Finder или проводнике — та же роль, что «Открыть» в
    // снекбаре Android.
    if reveal {
        crate::sys::reveal_file(&path);
    }
    Ok(path.to_string_lossy().to_string())
}
