//! Sol Flow для Mac: диктовка по глобальному сочетанию (по умолчанию
//! ⌥Пробел, настраивается). Два способа, как на телефоне: быстро нажал —
//! запись пошла, нажал ещё раз — текст вставился; зажал и держишь —
//! говоришь, отпустил — вставилось. Приложение живёт в меню-баре, во время
//! записи внизу экрана появляется пилюля-оверлей.

mod audio;
mod autostart;
mod cleanup;
/// Без фичи `diarize` подставляется заглушка: sherpa-onnx собран не везде.
#[cfg_attr(not(feature = "diarize"), path = "diarize_off.rs")]
mod diarize;
mod docx;
mod engine;
mod fetch;
mod history;
mod hotkey;
mod lang;
/// Оверлей на macOS — NSPanel, на остальных системах обычное окно.
#[cfg_attr(not(target_os = "macos"), path = "hud_win.rs")]
mod hud;
mod meetings;
mod models;
mod net;
mod paste;
mod pdf;
mod report;
mod segmenter;
mod sync;
#[cfg(has_summary)]
mod summary;
/// Без libsolflow_llama (llama-shim/build-macos.sh не гоняли) саммери
/// выключено, но всё остальное собирается и работает.
#[cfg(not(has_summary))]
mod summary {
    use anyhow::{anyhow, Result};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tauri::AppHandle;

    pub const MODEL_MB: u64 = 2440;

    pub fn model_path(_app: &AppHandle) -> PathBuf {
        PathBuf::new()
    }
    pub fn model_ready(_app: &AppHandle) -> bool {
        false
    }
    pub fn devices() -> String {
        String::new()
    }
    pub fn download(
        _app: &AppHandle,
        _on_progress: &dyn Fn(u8),
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        Err(anyhow!("сборка без модели саммери"))
    }
    pub fn summarize(
        _app: &AppHandle,
        _transcript: &str,
        _progress: impl Fn(u8) + Send + Sync + Clone + 'static,
        _cancelled: Arc<AtomicBool>,
    ) -> Result<String> {
        Err(anyhow!("сборка без модели саммери"))
    }
    pub fn title(_app: &AppHandle, _transcript_head: &str) -> Result<String> {
        Err(anyhow!("сборка без модели саммери"))
    }
}
mod ttf;
mod settings;
mod sys;
mod tools;
/// pub — им пользуется проверочный пример wav_check.
pub mod wav;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use audio::Recorder;
use engine::Engine;

/// Для тестовых примеров transcribe_file и meeting_check — внутренние
/// модули наружу не торчат.
pub fn segmenter_split(pcm: &[f32], sample_rate: usize) -> Vec<Vec<f32>> {
    segmenter::split(pcm, sample_rate)
}

pub fn cleanup_clean(text: &str) -> String {
    cleanup::clean(text)
}

pub use segmenter::{cut_frames, frame_energy, frame_samples, MAX_SEGMENT_SEC};

pub use meetings::Segment as MeetingSegment;

/// Inter из дизайн-системы сайта — тот же файл, что в Android-версии.
/// Разбирается один раз: на каждый экспорт заново парсить 400 КБ незачем.
pub fn inter_regular() -> &'static ttf::Font {
    static FONT: std::sync::OnceLock<ttf::Font> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        ttf::Font::parse(include_bytes!("../fonts/inter_regular.ttf")).expect("Inter Regular сломан")
    })
}

pub fn inter_medium() -> &'static ttf::Font {
    static FONT: std::sync::OnceLock<ttf::Font> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        ttf::Font::parse(include_bytes!("../fonts/inter_medium.ttf")).expect("Inter Medium сломан")
    })
}

/// Сборка PDF для примера export_check.
pub fn export_pdf(title: &str, duration: &str, segments: &[MeetingSegment]) -> Vec<u8> {
    meetings::as_pdf(title, duration, "", segments, &Default::default()).expect("pdf не собрался")
}

/// Сборка docx для примера export_check.
pub fn export_docx(title: &str, duration: &str, segments: &[MeetingSegment]) -> Vec<u8> {
    meetings::as_docx(title, duration, "", segments, &Default::default())
}

/// Сборка текста экспорта для примера export_check.
pub fn export_bodies(
    title: &str,
    duration: &str,
    segments: &[meetings::Segment],
) -> (String, String, String) {
    (
        meetings::as_text(title, duration, "", segments, &Default::default()),
        meetings::as_markdown(title, duration, "", segments, &Default::default()),
        meetings::as_html(title, duration, "", segments, &Default::default()),
    )
}

/// Разделение говорящих для примера diarize_check.
pub fn diarize_file(
    audio: &std::path::Path,
    seg_model: &std::path::Path,
    emb_model: &std::path::Path,
    num_speakers: i32,
) -> anyhow::Result<Vec<(f32, f32, usize)>> {
    diarize::turns_for_example(audio, seg_model, emb_model, num_speakers)
}

/// Загрузка по ссылке для примера fetch_check.
pub fn fetch_url(
    url: &str,
    dir: &std::path::Path,
) -> anyhow::Result<(std::path::PathBuf, String)> {
    let report = |done: u64, total: u64| {
        if total > 0 {
            println!("скачано {:.1} из {:.1} МБ", done as f64 / 1e6, total as f64 / 1e6);
        }
    };
    let cancelled = || false;
    fetch::fetch(url, dir, &fetch::Progress { report: &report, cancelled: &cancelled })
}

/// Похожесть голосов для примера diarize_check.
pub fn voice_similarity(
    a: &std::path::Path,
    b: &std::path::Path,
    emb_model: &std::path::Path,
) -> anyhow::Result<f32> {
    diarize::voice_similarity(a, b, emb_model)
}

/// Потоковый ресемплер для примера record_check.
pub fn downsampler_for_test(src: usize, dst: usize) -> meetings::Downsampler {
    meetings::Downsampler::new(src, dst)
}

/// Загруженный движок для примеров — в приложении он живёт в AppState.
pub fn engine_for_test(path: &std::path::PathBuf) -> Engine {
    let engine = Engine::new();
    engine.load(path, true).expect("модель не загрузилась");
    engine
}

/// Дольше этого — считаем «зажал и держит»: запись остановится по
/// отпусканию. Короче — режим двух нажатий.
const HOLD_MS: u128 = 400;

const PHASE_NO_MODEL: u8 = 0;
const PHASE_LOADING: u8 = 1;
const PHASE_READY: u8 = 2;
const PHASE_REC_UI: u8 = 3;
const PHASE_REC_HOTKEY: u8 = 4;
const PHASE_BUSY: u8 = 5;

struct AppState {
    engine: Arc<Engine>,
    recorder: Recorder,
    phase: AtomicU8,
    /// Момент нажатия, начавшего запись, — различает тап и удержание.
    press_started: Mutex<Option<Instant>>,
    settings: Mutex<settings::Settings>,
    /// Когда моделью пользовались в последний раз — по этому сторож
    /// решает, пора ли выгружать её из памяти.
    pub last_used: Mutex<Instant>,
}

/// Путь к активной модели по настройкам; None — файла нет.
fn model_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    let models = app.path().app_data_dir().ok()?.join("models");
    let chosen = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .active_model
        .clone()
        .map(|f| models.join(f))
        .filter(|p| p.exists());
    chosen.or_else(|| Engine::find_model(&models))
}

/// Поднимает модель, если её выгрузили по таймеру. Делает это в фоне:
/// GGUF грузится секунды, а человек в этот момент уже говорит.
pub(crate) fn ensure_model_loaded(app: AppHandle) {
    let state = app.state::<AppState>();
    *state.last_used.lock().unwrap() = Instant::now();
    if state.engine.is_loaded() {
        return;
    }
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        if state.engine.is_loaded() {
            return;
        }
        if let Some(path) = model_path(&app) {
            log::info!("модель поднимается обратно в память");
            let use_gpu = state.settings.lock().unwrap().use_gpu;
            if let Err(e) = state.engine.load(&path, use_gpu) {
                log::error!("модель не поднялась: {e}");
            }
            emit_state(&app, None);
        }
    });
}

/// Иконка в меню-баре появляется и исчезает по настройке. Без неё окно
/// возвращается повторным запуском приложения — об этом сказано в
/// настройках, иначе выключивший её человек остаётся без входа.
fn apply_tray(app: &AppHandle) {
    let want = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .show_tray_icon;
    let existing = app.tray_by_id("main");

    if !want {
        if existing.is_some() {
            let _ = app.remove_tray_by_id("main");
        }
        return;
    }
    if existing.is_some() {
        return;
    }

    let build = || -> tauri::Result<()> {
        let open_item = MenuItem::with_id(
            app,
            "open",
            lang::t(app, "Открыть Sol Flow"),
            true,
            None::<&str>,
        )?;
        let quit_item =
            MenuItem::with_id(app, "quit", lang::t(app, "Выйти"), true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        TrayIconBuilder::with_id("main")
            .icon(icon)
            .icon_as_template(true)
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(|app, event| match event.id.as_ref() {
                "open" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            })
            .build(app)?;
        Ok(())
    };
    if let Err(e) = build() {
        log::error!("иконка меню-бара не создалась: {e}");
    }
}

/// Сторож памяти: если моделью давно не пользовались, отпускает её.
/// Так приложение, висящее в меню-баре сутками, не держит гигабайт зря.
fn spawn_unload_watch(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(20));
        let state = app.state::<AppState>();

        // Во время записи и распознавания не трогаем — как и во время
        // работы над встречей: она идёт своим чередом и о фазе диктовки
        // ничего не знает.
        let phase = state.phase.load(Ordering::SeqCst);
        if phase != PHASE_READY || !state.engine.is_loaded() || meetings::busy(&app) {
            continue;
        }
        let Some(after) = state.settings.lock().unwrap().unload_after() else {
            continue;
        };
        let idle = state.last_used.lock().unwrap().elapsed().as_secs();
        if idle >= after {
            log::info!("модель выгружена после {idle} с простоя");
            state.engine.unload();
            emit_state(&app, None);
        }
    });
}

#[derive(Clone, Serialize)]
struct StateEvent {
    phase: String,
    model: Option<String>,
    detail: Option<String>,
    accessibility: bool,
    hotkey: String,
    hotkey_label: String,
    /// Чем считается модель: «видеокарта Intel Arc» или «процессор».
    device: Option<String>,
}

fn phase_name(phase: u8) -> &'static str {
    match phase {
        PHASE_NO_MODEL => "no_model",
        PHASE_LOADING => "loading",
        PHASE_READY => "ready",
        PHASE_REC_UI | PHASE_REC_HOTKEY => "recording",
        _ => "transcribing",
    }
}

fn emit_state(app: &AppHandle, detail: Option<String>) {
    let state = app.state::<AppState>();
    let hotkey_text = state.settings.lock().unwrap().hotkey.clone();
    let event = StateEvent {
        phase: phase_name(state.phase.load(Ordering::SeqCst)).to_string(),
        model: state.engine.model_name.lock().unwrap().clone(),
        detail,
        accessibility: paste::accessibility_granted(),
        hotkey_label: hotkey::label(app, &hotkey_text),
        hotkey: hotkey_text,
        device: state.engine.device_label(),
    };
    let _ = app.emit("solflow-state", event);
}

/// Глушение колонок на время записи: иначе музыка попадает в микрофон и
/// оседает в расшифровке. Возвращаем звук после остановки.
fn mute_system(app: &AppHandle) {
    let muted = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .mute_while_recording;
    if !muted {
        return;
    }
    sys::set_muted(true);
}

fn unmute_system(app: &AppHandle) {
    let muted = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .mute_while_recording;
    if !muted {
        return;
    }
    sys::set_muted(false);
}

fn start_recording(app: &AppHandle, from_hotkey: bool) {
    let state = app.state::<AppState>();
    let (device, sound) = {
        let settings = state.settings.lock().unwrap();
        (settings.input_device.clone(), settings.start_sound)
    };
    if sound {
        history::play_start_sound(app);
        // Глушение системного звука откладываем: сигнал начинает звучать не
        // мгновенно, и глушение успевало прихлопнуть его же — сигнал то
        // слышно, то нет. Если запись к этому моменту уже кончилась,
        // глушить нечего: иначе звук остался бы выключенным насовсем.
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(350));
            let phase = handle.state::<AppState>().phase.load(Ordering::SeqCst);
            if phase == PHASE_REC_UI || phase == PHASE_REC_HOTKEY {
                mute_system(&handle);
            }
        });
    } else {
        mute_system(app);
    }
    match state.recorder.start(device) {
        Ok(()) => {
            log::info!("запись пошла (hotkey={from_hotkey})");
            state.phase.store(
                if from_hotkey { PHASE_REC_HOTKEY } else { PHASE_REC_UI },
                Ordering::SeqCst,
            );
            *state.press_started.lock().unwrap() =
                if from_hotkey { Some(Instant::now()) } else { None };
            emit_state(app, None);
            if from_hotkey {
                hud::show(app);
            }
            spawn_level_loop(app.clone());
            // Модель могла уйти из памяти по таймеру — поднимаем её, пока
            // человек говорит, чтобы к остановке она уже была на месте.
            ensure_model_loaded(app.clone());
        }
        Err(e) => {
            log::error!("микрофон: {e}");
            emit_state(app, Some(format!("Микрофон: {e}")));
        }
    }
}

fn stop_and_process(app: &AppHandle) {
    let state = app.state::<AppState>();
    let phase = state.phase.load(Ordering::SeqCst);
    if phase != PHASE_REC_UI && phase != PHASE_REC_HOTKEY {
        return;
    }
    let paste_result = phase == PHASE_REC_HOTKEY;
    state.phase.store(PHASE_BUSY, Ordering::SeqCst);
    emit_state(app, None);

    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let pcm = state.recorder.stop();
        unmute_system(&app);

        let (drop_parasites, keep_audio) = {
            let settings = state.settings.lock().unwrap();
            (settings.remove_fillers, settings.keep_audio)
        };
        let recorded = pcm.as_ref().ok().cloned();
        let outcome = pcm.and_then(|pcm| {
            let seconds = pcm.len() as f32 / audio::TARGET_RATE as f32;
            if seconds < 0.3 {
                return Ok(String::new());
            }
            state.engine.transcribe_with(&pcm, drop_parasites)
        });

        state.phase.store(PHASE_READY, Ordering::SeqCst);
        match &outcome {
            Ok(t) => log::info!("распознано {} символов", t.len()),
            Err(e) => log::error!("распознавание: {e}"),
        }
        match outcome {
            Ok(text) => {
                let _ = app.emit("solflow-result", text.clone());
                history::add(&app, &text, recorded.filter(|_| keep_audio).as_deref());
                let mut detail = None;
                if paste_result && !text.is_empty() {
                    let options = {
                        let settings = state.settings.lock().unwrap();
                        paste::PasteOptions {
                            restore_clipboard: settings.clipboard_handling != "keep",
                            submit: settings
                                .auto_submit
                                .then(|| paste::SubmitKey::parse(&settings.auto_submit_key)),
                        }
                    };
                    if let Err(e) = paste::paste_text(&app, &text, &options) {
                        log::error!("вставка не удалась: {e}");
                        detail = Some(format!("{e}"));
                    }
                }
                emit_state(&app, detail);
            }
            Err(e) => emit_state(&app, Some(format!("{e}"))),
        }
        hud::hide(&app);
    });
}

/// Нажатие сочетания: свободен — начать запись; идёт запись — остановить.
fn on_hotkey_press(app: &AppHandle) {
    log::info!("хоткей: нажатие");
    let state = app.state::<AppState>();
    match state.phase.load(Ordering::SeqCst) {
        PHASE_READY => start_recording(app, true),
        PHASE_REC_UI | PHASE_REC_HOTKEY => stop_and_process(app),
        _ => {}
    }
}

/// Отпускание: если это было удержание — остановить запись. Быстрый тап
/// оставляет запись идти до второго нажатия.
fn on_hotkey_release(app: &AppHandle) {
    log::info!("хоткей: отпускание");
    let state = app.state::<AppState>();
    if state.phase.load(Ordering::SeqCst) != PHASE_REC_HOTKEY {
        return;
    }
    let held = state
        .press_started
        .lock()
        .unwrap()
        .map(|t| t.elapsed().as_millis() >= HOLD_MS)
        .unwrap_or(false);
    if held {
        stop_and_process(app);
    }
}

fn spawn_level_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        let state = app.state::<AppState>();
        let phase = state.phase.load(Ordering::SeqCst);
        if phase != PHASE_REC_UI && phase != PHASE_REC_HOTKEY {
            break;
        }
        let _ = app.emit("solflow-level", state.recorder.level());
        std::thread::sleep(std::time::Duration::from_millis(80));
    });
}

fn register_hotkey(app: &AppHandle, text: &str) -> Result<(), String> {
    let shortcut = hotkey::parse(text).ok_or_else(|| format!("не понимаю сочетание «{text}»"))?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("сочетание занято: {e}"))
}

// --- команды окна ---------------------------------------------------------

#[tauri::command]
fn ui_toggle(app: AppHandle) {
    let state = app.state::<AppState>();
    match state.phase.load(Ordering::SeqCst) {
        PHASE_READY => start_recording(&app, false),
        PHASE_REC_UI | PHASE_REC_HOTKEY => stop_and_process(&app),
        _ => {}
    }
}

/// Тап по пилюле — как второе нажатие сочетания: остановить и вставить.
#[tauri::command]
fn hud_stop(app: AppHandle) {
    stop_and_process(&app);
}

#[tauri::command]
fn ui_state(app: AppHandle) {
    emit_state(&app, None);
}

#[tauri::command]
fn set_hotkey(app: AppHandle, combo: String) -> Result<String, String> {
    register_hotkey(&app, &combo)?;
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.hotkey = combo.clone();
    settings::save(&app, &s);
    drop(s);
    emit_state(&app, None);
    Ok(hotkey::label(&app, &combo))
}

/// Ссылка открывается в браузере, а не внутри окна приложения.
#[tauri::command]
fn open_link(url: String) {
    // Только http(s) и почта: открывать произвольные схемы из окна незачем.
    if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
        sys::open_url(&url);
    }
}

#[derive(Clone, Serialize)]
struct UpdateInfo {
    current: String,
    latest: Option<String>,
    newer: bool,
    url: String,
}

/// Где искать новые версии. Пока репозиторий не заведён, проверка просто
/// ничего не находит — приложение от этого не ломается.
const RELEASES_API: &str =
    "https://api.github.com/repos/isoloma-ux/solflow/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/isoloma-ux/solflow/releases/latest";

/// Сравнение версий вида «1.2.3» по числам, а не по строкам.
fn version_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split(['.', '-'])
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let (a, b) = (parse(latest), parse(current));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Отчёт о проблеме: текст пользователя плюс сведения о системе.
#[tauri::command]
fn bug_report(app: AppHandle, description: String) -> String {
    report::build(&app, &description)
}

/// Отправка почтой: заполняем письмо и отдаём его почтовому клиенту.
#[tauri::command]
fn send_bug_report(app: AppHandle, description: String) {
    let body = report::build(&app, &description);
    let encode = |s: &str| -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    let url = format!(
        "mailto:me@isoloma.ru?subject={}&body={}",
        encode("Sol Flow: проблема"),
        encode(&body)
    );
    sys::open_url(&url);
}

/// Версия приложения — окно показывает её в подвале, не спрашивая сеть.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn check_update() -> UpdateInfo {
    latest_release()
}

/// Раз в шесть часов смотрим, не вышла ли новая версия, и говорим окну.
/// Человек не должен узнавать об обновлении, только если сам зайдёт в «О
/// проекте»; первый раз спрашиваем через минуту после старта — при запуске
/// со стартом системы сети может ещё не быть.
fn spawn_update_watch(app: AppHandle) {
    const FIRST_DELAY: u64 = 60;
    const EVERY: u64 = 6 * 60 * 60;

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(FIRST_DELAY));
        loop {
            let info = latest_release();
            if info.newer {
                let _ = app.emit("solflow-update", info);
            }
            std::thread::sleep(std::time::Duration::from_secs(EVERY));
        }
    });
}

fn latest_release() -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = net::get_json(RELEASES_API)
        .ok()
        .and_then(|v| v.get("tag_name")?.as_str().map(|s| s.to_string()));

    UpdateInfo {
        newer: latest
            .as_deref()
            .map(|l| version_newer(l, &current))
            .unwrap_or(false),
        current,
        latest,
        url: RELEASES_PAGE.to_string(),
    }
}

/// Скачивает и ставит новую версию. Файл берётся с GitHub и проверяется по
/// подписи: приложение само запускает то, что скачало, и без проверки это
/// была бы дыра — подменят ответ, и оно послушно поставит чужое.
///
/// Проценты уходят в окно: установщик весит десятки мегабайт.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("не вышло проверить обновление: {e}"))?;

    let Some(update) = update else {
        return Err("новой версии нет".to_string());
    };

    let mut downloaded: u64 = 0;
    let progress = app.clone();
    let done = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let pct = total
                    .map(|t| ((downloaded * 100) / t.max(1)).min(99) as u8)
                    .unwrap_or(0);
                let _ = progress.emit("solflow-update-progress", pct);
            },
            move || {
                let _ = done.emit("solflow-update-progress", 100u8);
            },
        )
        .await
        .map_err(|e| format!("обновление не поставилось: {e}"))?;

    // На Windows установщик просит закрыть приложение сам, но перезапуск
    // делаем явно: иначе человек остаётся перед пустотой.
    app.restart();
}

#[tauri::command]
fn open_accessibility(app: AppHandle) {
    sys::open_accessibility_settings();
    let _ = app;
}

#[tauri::command]
fn list_input_devices() -> Vec<String> {
    audio::input_devices()
}

/// Настройки звука и микрофона одним куском — окно правит их по одной.
#[tauri::command]
fn get_settings(app: AppHandle) -> settings::Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_input_device(app: AppHandle, device: Option<String>) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.input_device = device.filter(|d| !d.is_empty());
    settings::save(&app, &s);
}

/// Тема окна: "system", "light" или "dark".
#[tauri::command]
fn set_theme(app: AppHandle, theme: String) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.theme = match theme.as_str() {
        "light" | "dark" => theme,
        _ => "system".to_string(),
    };
    settings::save(&app, &s);
}

#[tauri::command]
fn set_start_sound(app: AppHandle, enabled: bool) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.start_sound = enabled;
    settings::save(&app, &s);
    drop(s);
    // Сразу проигрываем, чтобы было слышно, что именно включили.
    if enabled {
        history::play_start_sound(&app);
    }
}

/// Пачка настроек одной командой: окно правит их по одной, но каждая
/// правка — это сохранение всего файла, и держать по команде на поле уже
/// накладно.
#[tauri::command]
fn set_option(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        let text = || value.as_str().unwrap_or_default().to_string();
        match key.as_str() {
            "start_hidden" => s.start_hidden = value.as_bool().unwrap_or(false),
            "show_tray_icon" => s.show_tray_icon = value.as_bool().unwrap_or(true),
            "overlay_style" => s.overlay_style = text(),
            "overlay_position" => s.overlay_position = text(),
            "model_unload" => s.model_unload = text(),
            "clipboard_handling" => s.clipboard_handling = text(),
            "auto_submit" => s.auto_submit = value.as_bool().unwrap_or(false),
            "auto_submit_key" => s.auto_submit_key = text(),
            "mute_while_recording" => s.mute_while_recording = value.as_bool().unwrap_or(false),
            "remove_fillers" => s.remove_fillers = value.as_bool().unwrap_or(false),
            "history_limit" => {
                s.history_limit = value.as_u64().unwrap_or(50).clamp(1, 1000) as usize
            }
            "history_retention" => s.history_retention = text(),
            "keep_audio" => s.keep_audio = value.as_bool().unwrap_or(true),
            "sync_audio" => s.sync_audio = value.as_bool().unwrap_or(false),
            "sync_auto_summary" => s.sync_auto_summary = value.as_bool().unwrap_or(true),
            "sync_interval" => s.sync_interval = text(),
            other => return Err(format!("неизвестная настройка «{other}»")),
        }
        settings::save(&app, &s);
    }

    // Часть настроек должна подействовать сейчас, а не при следующем запуске.
    match key.as_str() {
        "show_tray_icon" => apply_tray(&app),
        "history_limit" | "history_retention" => history::apply_limits(&app),
        // Включили звук — он должен поехать сейчас, а не через пять минут.
        "sync_audio" => sync::sync_now(&app),
        "model_unload" => {
            let now = app.state::<AppState>().settings.lock().unwrap().unload_after();
            if now == Some(0) {
                app.state::<AppState>().engine.unload();
                emit_state(&app, None);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Звук диктовки для плеера в истории.
#[tauri::command]
fn history_audio(app: AppHandle, at: i64) -> Result<Vec<u8>, String> {
    std::fs::read(history::audio_path(&app, at)).map_err(|_| "записи нет".to_string())
}

/// Расшифровать сохранённый звук заново — например, другой моделью.
#[tauri::command]
fn history_retranscribe(app: AppHandle, at: i64) {
    std::thread::spawn(move || {
        let path = history::audio_path(&app, at);
        let Ok(mut wav) = wav::WavReader::open(&path) else {
            let _ = app.emit("solflow-history-failed", "звук не найден");
            return;
        };
        let Ok(pcm) = wav.read(0, wav.total_samples as usize) else {
            let _ = app.emit("solflow-history-failed", "звук не читается");
            return;
        };

        ensure_model_loaded(app.clone());
        let state = app.state::<AppState>();
        for _ in 0..60 {
            if state.engine.is_loaded() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let drop_parasites = state.settings.lock().unwrap().remove_fillers;
        match state.engine.transcribe_with(&pcm, drop_parasites) {
            Ok(text) => history::update_text(&app, at, &text),
            Err(e) => {
                let _ = app.emit("solflow-history-failed", format!("{e}"));
            }
        }
    });
}

#[tauri::command]
fn history_list(app: AppHandle) -> Vec<history::Entry> {
    history::all(&app)
}

#[tauri::command]
fn history_delete(app: AppHandle, at: i64) {
    history::remove(&app, at);
}

#[tauri::command]
fn history_clear(app: AppHandle) {
    history::clear(&app);
}

#[tauri::command]
fn list_models(app: AppHandle) -> Vec<models::ModelRow> {
    let state = app.state::<AppState>();
    let active = state.settings.lock().unwrap().active_model.clone();
    app.state::<models::ModelStore>().rows(&app, &active)
}

/// Процессор этой машины — по нему в каталоге показывается ориентир по
/// скорости.
#[tauri::command]
fn machine_chip() -> String {
    let name = sys::cpu_name();
    if name.is_empty() {
        "этом компьютере".to_string()
    } else {
        name
    }
}

/// Есть ли в каталоге Handy модели, которых нет у нас. Сам каталог зашит в
/// приложение — обновляется он вместе с версией, но сказать, что новое
/// появилось, можно и сейчас.
#[tauri::command]
fn catalog_news(app: AppHandle) -> Option<String> {
    const UPSTREAM: &str =
        "https://raw.githubusercontent.com/cjpais/Handy/main/src-tauri/src/catalog/catalog.json";
    let upstream = net::get_json(UPSTREAM).ok()?;
    let names: Vec<&str> = upstream
        .get("models")?
        .as_array()?
        .iter()
        .filter_map(|m| m.get("name")?.as_str())
        .collect();

    let known: std::collections::HashSet<String> = app
        .state::<models::ModelStore>()
        .rows(&app, &None)
        .into_iter()
        .map(|r| r.name)
        .collect();
    let fresh: Vec<&str> = names
        .into_iter()
        .filter(|n| !known.contains(*n))
        .take(3)
        .collect();

    if fresh.is_empty() {
        None
    } else {
        Some(format!(
            "В каталоге Handy появились новые модели: {}. Они приедут со следующей версией приложения",
            fresh.join(", ")
        ))
    }
}

#[tauri::command]
fn list_languages(app: AppHandle) -> Vec<models::LanguageRow> {
    app.state::<models::ModelStore>().languages(&app)
}

#[tauri::command]
fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<models::ModelStore>()
        .download(&app, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_model(app: AppHandle, filename: String) {
    app.state::<models::ModelStore>().cancel_download(&filename);
}

#[tauri::command]
fn delete_model(app: AppHandle, filename: String) {
    app.state::<models::ModelStore>().delete(&app, &filename);
    let _ = app.emit("solflow-models", ());
}

/// Переключение активной модели: настройка сохраняется, модель грузится в
/// фоне — окно живёт, статус показывает загрузку.
#[tauri::command]
fn set_active_model(app: AppHandle, filename: String) {
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.active_model = Some(filename.clone());
        settings::save(&app, &s);
    }
    state.phase.store(PHASE_LOADING, Ordering::SeqCst);
    emit_state(&app, None);
    let _ = app.emit("solflow-models", ());

    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let path = app
            .path()
            .app_data_dir()
            .map(|d| d.join("models").join(&filename))
            .unwrap_or_default();
        let use_gpu = state.settings.lock().unwrap().use_gpu;
        match state.engine.load(&path, use_gpu) {
            Ok(()) => state.phase.store(PHASE_READY, Ordering::SeqCst),
            Err(e) => {
                state.phase.store(PHASE_NO_MODEL, Ordering::SeqCst);
                emit_state(&app, Some(format!("{e}")));
                let _ = app.emit("solflow-models", ());
                return;
            }
        }
        emit_state(&app, None);
        let _ = app.emit("solflow-models", ());
    });
}

// --- команды встреч --------------------------------------------------------

#[tauri::command]
fn meetings_list(app: AppHandle) -> Vec<meetings::MeetingRow> {
    meetings::rows(&app)
}

#[tauri::command]
fn meeting_segments(app: AppHandle, id: i64) -> Vec<meetings::Segment> {
    meetings::load_transcript(&app, id)
}

#[tauri::command]
fn meeting_record_start(app: AppHandle) -> Result<(), String> {
    meetings::record_start(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn meeting_record_stop(app: AppHandle) {
    meetings::record_stop(&app);
}

#[tauri::command]
fn meeting_record_pause(app: AppHandle) {
    meetings::record_pause(&app);
}

/// Диалог выбора и конвертация — в фоне: команда возвращается сразу.
#[tauri::command]
fn meeting_import(app: AppHandle) {
    std::thread::spawn(move || {
        if let Some(path) = meetings::pick_import_file(&app) {
            if let Err(e) = meetings::import(&app, path) {
                log::error!("импорт: {e}");
            }
        }
    });
}

/// Файлы, брошенные в окно: каждый идёт отдельной встречей.
#[tauri::command]
fn meeting_import_paths(app: AppHandle, paths: Vec<String>) {
    for path in paths {
        if let Err(e) = meetings::import(&app, std::path::PathBuf::from(path)) {
            log::error!("импорт перетащенного файла: {e}");
        }
    }
}

/// Импорт по ссылке: качаем звук, дальше обычная цепочка расшифровки.
#[tauri::command]
fn meeting_import_url(app: AppHandle, url: String) {
    std::thread::spawn(move || {
        if let Err(e) = meetings::import_url(&app, url) {
            log::error!("импорт по ссылке: {e}");
        }
    });
}

/// Стоит ли загрузчик для ссылок на видеосервисы.
/// Остановить загрузку или расшифровку встречи.
#[tauri::command]
fn meeting_cancel(app: AppHandle, id: i64) {
    meetings::cancel(&app, id);
}

/// Саммери встречи локальной моделью; модель докачается сама, интерфейс
/// предупреждает о её размере до запуска.
#[tauri::command]
fn meeting_summarize(app: AppHandle, id: i64) {
    meetings::summarize(&app, id);
}

/// Скачана ли модель саммери и сколько она весит — для предупреждения.
#[tauri::command]
fn summary_state(app: AppHandle) -> (bool, u64) {
    (summary::model_ready(&app), summary::MODEL_MB)
}

/// Придумать название по кнопке — для готовых встреч.
#[tauri::command]
fn meeting_autotitle(app: AppHandle, id: i64) {
    meetings::autotitle(&app, id);
}

/// Папка для скачанного по ссылке; пустая строка — не оставлять исходник.
#[tauri::command]
fn set_downloads_dir(app: AppHandle, dir: Option<String>) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.downloads_dir = dir.filter(|d| !d.is_empty());
    settings::save(&app, &s);
}

/// Выбор папки системным диалогом. Команда синхронная, а значит идёт не с
/// главного потока — ждать ответа пользователя тут можно.
/// Окно сообщает, на каком языке оно в итоге открылось: по этому языку
/// говорят меню в трее и сообщения из Rust. Определять язык второй раз тут
/// незачем — два определения рано или поздно разойдутся.
#[tauri::command]
fn set_ui_language(app: AppHandle, language: String) {
    {
        let state = app.state::<lang::Language>();
        let mut current = state.0.lock().unwrap();
        if *current == language {
            return;
        }
        *current = language;
    }
    // Меню в трее уже собрано — пересобираем его на новом языке.
    let _ = app.remove_tray_by_id("main");
    apply_tray(&app);
    emit_state(&app, None);
}

/// Язык интерфейса: "auto", "ru" или "en". Окно после этого перезагружает
/// себя — переводить уже нарисованное на ходу дороже, чем открыть заново.
#[tauri::command]
fn set_language(app: AppHandle, language: String) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.language = language;
    settings::save(&app, &s);
}

/// Режим экспорта: "downloads" — в «Загрузки», "folder" — в выбранную
/// папку, "ask" — спрашивать каждый раз.
#[tauri::command]
fn set_export_mode(app: AppHandle, mode: String) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.export_ask = mode == "ask";
    if mode == "downloads" {
        s.export_dir = None;
    }
    settings::save(&app, &s);
}

/// Куда складывать экспорт встреч; пустая строка — вернуть «Загрузки».
#[tauri::command]
fn set_export_dir(app: AppHandle, dir: Option<String>) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.export_dir = dir.filter(|d| !d.is_empty());
    settings::save(&app, &s);
}

/// Считать на видеокарте или строго на процессоре. Модель перезагружается:
/// устройство выбирается при загрузке и на лету не меняется.
#[tauri::command]
fn set_use_gpu(app: AppHandle, enabled: bool) {
    {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().unwrap();
        if s.use_gpu == enabled {
            return;
        }
        s.use_gpu = enabled;
        settings::save(&app, &s);
    }
    let state = app.state::<AppState>();
    state.engine.unload();
    ensure_model_loaded(app);
}

/// Асинхронная нарочно: синхронные команды Tauri идут по главному потоку, а
/// системный диалог его же и ждёт — окно повисало намертво. Асинхронная
/// уходит в отдельный поток, и ждать диалог там можно.
#[tauri::command]
async fn pick_export_dir(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Куда складывать экспорт встреч")
        .blocking_pick_folder()
        .and_then(|dir| dir.into_path().ok())
        .map(|dir| dir.to_string_lossy().to_string())
}

/// Асинхронная по той же причине, что и pick_export_dir.
#[tauri::command]
async fn pick_downloads_dir(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Куда складывать скачанное")
        .blocking_pick_folder()
        .and_then(|dir| dir.into_path().ok())
        .map(|dir| dir.to_string_lossy().to_string())
}

#[tauri::command]
fn downloader_ready() -> bool {
    tools::ready()
}

#[tauri::command]
fn install_downloader(app: AppHandle) -> Result<(), String> {
    // Проценты уходят в окно: на Windows качается ещё и ffmpeg, это долго.
    let report = |pct: u8| {
        let _ = app.emit("solflow-downloader-progress", pct);
    };
    tools::install(&report).map_err(|e| e.to_string())
}

#[tauri::command]
fn meeting_transcribe(app: AppHandle, id: i64) {
    meetings::transcribe(&app, id);
}

/// Разделение говорящих: 0 — определить число самой.
#[tauri::command]
fn meeting_diarize(app: AppHandle, id: i64, speakers: i32) {
    meetings::diarize(&app, id, speakers);
}

/// Сколько мегабайт качать под разделение и скачаны ли они уже.
#[tauri::command]
fn diarize_status(app: AppHandle) -> (bool, u64) {
    (diarize::models_ready(&app), diarize::DOWNLOAD_MB)
}

#[tauri::command]
fn meeting_rename_speaker(app: AppHandle, id: i64, speaker: u32, name: String) {
    meetings::rename_speaker(&app, id, speaker, name);
}

#[tauri::command]
fn meeting_rename(app: AppHandle, id: i64, title: String) {
    meetings::rename(&app, id, title);
}

#[tauri::command]
fn meeting_delete(app: AppHandle, id: i64) {
    meetings::delete(&app, id);
}

#[tauri::command]
fn meeting_set_project(app: AppHandle, id: i64, project: Option<String>) {
    meetings::set_project(&app, id, project);
}

#[tauri::command]
async fn meeting_export(
    app: AppHandle,
    id: i64,
    format: String,
    title: String,
) -> Result<String, String> {
    // «Спрашивать каждый раз» — это диалог сохранения: человек сам выбирает
    // и папку, и имя. Команда синхронная, то есть идёт не с главного потока,
    // и ждать ответа тут можно.
    let ask = app.state::<AppState>().settings.lock().unwrap().export_ask;
    let target = if ask {
        let name = format!("{}.{}", meetings::safe_file_name(&title), format);
        match app
            .dialog()
            .file()
            .set_title("Куда сохранить")
            .set_file_name(&name)
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
        {
            Some(path) => meetings::Target::File(path),
            // Передумал — это не ошибка.
            None => return Ok(String::new()),
        }
    } else {
        meetings::Target::AsSettings
    };
    meetings::export(&app, id, &format, &title, true, target).map_err(|e| e.to_string())
}

/// Групповые действия из списка. Заголовки приходят из окна — там же, где
/// собирается «Встреча 27 августа» для встреч без своего названия.
#[tauri::command]
async fn meetings_export(
    app: AppHandle,
    ids: Vec<i64>,
    format: String,
    titles: Vec<String>,
) -> Result<usize, String> {
    let mut done = 0;
    let mut last_error = None;
    let mut last_path = None;

    // Пачкой спрашиваем один раз и про папку целиком: диалог на каждую
    // встречу превратил бы выгрузку десятка записей в пытку.
    let ask = app.state::<AppState>().settings.lock().unwrap().export_ask;
    let folder = if ask {
        match app
            .dialog()
            .file()
            .set_title("Куда сохранить выгрузку")
            .blocking_pick_folder()
            .and_then(|dir| dir.into_path().ok())
        {
            Some(dir) => Some(dir),
            None => return Ok(0),
        }
    } else {
        None
    };

    for (index, id) in ids.iter().enumerate() {
        let title = titles.get(index).cloned().unwrap_or_default();
        let target = match &folder {
            Some(dir) => meetings::Target::Dir(dir.clone()),
            None => meetings::Target::AsSettings,
        };
        match meetings::export(&app, *id, &format, &title, false, target) {
            Ok(path) => {
                done += 1;
                last_path = Some(path);
            }
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    // Папку показываем один раз на всю пачку — по последнему сохранённому.
    if let Some(path) = last_path {
        sys::reveal_file(std::path::Path::new(&path));
    }
    match last_error {
        // Часть могла быть без расшифровки — сообщаем, но что вышло, то вышло.
        Some(e) if done == 0 => Err(e),
        _ => Ok(done),
    }
}

/// Выгрузка выбранных одним файлом. `title` приходит из окна («Встречи
/// 31 августа») — там же, где собираются названия встреч без имени.
#[tauri::command]
async fn meetings_export_combined(
    app: AppHandle,
    ids: Vec<i64>,
    format: String,
    titles: Vec<String>,
    title: String,
) -> Result<String, String> {
    let ask = app.state::<AppState>().settings.lock().unwrap().export_ask;
    let target = if ask {
        match app
            .dialog()
            .file()
            .set_title("Куда сохранить выгрузку")
            .blocking_pick_folder()
            .and_then(|dir| dir.into_path().ok())
        {
            Some(dir) => meetings::Target::Dir(dir),
            None => return Ok(String::new()),
        }
    } else {
        meetings::Target::AsSettings
    };
    let items: Vec<(i64, String)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, titles.get(i).cloned().unwrap_or_default()))
        .collect();
    meetings::export_combined(&app, &items, &format, &title, target).map_err(|e| e.to_string())
}

#[tauri::command]
fn meetings_delete(app: AppHandle, ids: Vec<i64>) {
    for id in ids {
        meetings::delete(&app, id);
    }
}

#[tauri::command]
fn meetings_transcribe(app: AppHandle, ids: Vec<i64>) {
    for id in ids {
        meetings::transcribe(&app, id);
    }
}

#[tauri::command]
fn meeting_search(app: AppHandle, query: String) -> Vec<meetings::Hit> {
    meetings::search(&app, &query)
}

/// Запускать ли приложение вместе с системой.
#[tauri::command]
fn autostart_enabled() -> bool {
    autostart::enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    autostart::set(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn projects_list(app: AppHandle) -> Vec<meetings::Project> {
    meetings::projects(&app)
}

#[tauri::command]
fn project_create(app: AppHandle, name: String) -> Option<meetings::Project> {
    meetings::create_project(&app, name)
}

#[tauri::command]
fn project_rename(app: AppHandle, id: String, name: String) {
    meetings::rename_project(&app, &id, name);
}

#[tauri::command]
fn project_delete(app: AppHandle, id: String) {
    meetings::delete_project(&app, &id);
}

// --- синхронизация ---------------------------------------------------------

#[tauri::command]
fn sync_status(app: AppHandle) -> sync::Status {
    sync::status(&app)
}

/// Начать вход: вернуть код, который человек введёт на странице Яндекса.
#[tauri::command]
fn sync_connect(app: AppHandle) -> Result<sync::yandex::DeviceCode, String> {
    sync::connect_start(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_connect_cancel(app: AppHandle) {
    sync::connect_cancel(&app);
}

#[tauri::command]
fn sync_disconnect(app: AppHandle) {
    sync::disconnect(&app);
}

#[tauri::command]
fn sync_now(app: AppHandle) {
    sync::sync_now(&app);
}

#[tauri::command]
fn models_dir(app: AppHandle) -> String {
    app.path()
        .app_data_dir()
        .map(|d| d.join("models").to_string_lossy().to_string())
        .unwrap_or_default()
}

// --- запуск ---------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    report::init();

    let builder = tauri::Builder::default();

    // Плагин просят ставить первым: он перехватывает запуск ещё до окон.
    // Вторая копия отдаёт своё окно первой и завершается.
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));

    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| match event.state() {
                    ShortcutState::Pressed => on_hotkey_press(app),
                    ShortcutState::Released => on_hotkey_release(app),
                })
                .build(),
        );

    // Пилюля-оверлей на macOS живёт в NSPanel — плагин нужен только там.
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .invoke_handler(tauri::generate_handler![
            ui_toggle,
            hud_stop,
            ui_state,
            set_hotkey,
            open_accessibility,
            open_link,
            check_update,
            install_update,
            app_version,
            bug_report,
            send_bug_report,
            models_dir,
            list_input_devices,
            get_settings,
            set_input_device,
            set_start_sound,
            set_theme,
            history_list,
            history_audio,
            history_retranscribe,
            set_option,
            history_delete,
            history_clear,
            list_models,
            list_languages,
            catalog_news,
            machine_chip,
            download_model,
            cancel_model,
            delete_model,
            set_active_model,
            meetings_list,
            meeting_segments,
            meeting_record_start,
            meeting_record_stop,
            meeting_record_pause,
            meetings_export_combined,
            meeting_summarize,
            summary_state,
            meeting_autotitle,
            meeting_import,
            meeting_import_paths,
            meeting_import_url,
            meeting_cancel,
            set_downloads_dir,
            pick_downloads_dir,
            set_export_dir,
            pick_export_dir,
            set_export_mode,
            set_language,
            set_ui_language,
            set_use_gpu,
            downloader_ready,
            install_downloader,
            meeting_transcribe,
            meeting_diarize,
            diarize_status,
            meeting_rename_speaker,
            meeting_rename,
            meeting_delete,
            meeting_set_project,
            meeting_export,
            meetings_export,
            meetings_delete,
            meetings_transcribe,
            meeting_search,
            autostart_enabled,
            set_autostart,
            projects_list,
            project_create,
            project_rename,
            project_delete,
            sync_status,
            sync_connect,
            sync_connect_cancel,
            sync_disconnect,
            sync_now
        ])
        .setup(|app| {
            // Приложение живёт в меню-баре, в доке его нет — как Handy.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Enigo — строго на главном потоке, см. комментарий в paste.rs.
            app.manage(lang::Language::new());

            // Сама «нажималка» появится при первой вставке: до выдачи
            // «Универсального доступа» её создать нельзя, а разрешение
            // человек выдаёт уже после запуска (см. paste.rs).
            app.manage(paste::EnigoState::new());

            // Где лежат скачанные yt-dlp и ffmpeg — запоминается один раз.
            tools::init(app.handle());

            // Модули вычислительных бэкендов (процессор, видеокарта) — до
            // первой загрузки модели. В сборке без отдельных модулей это
            // ничего не делает.
            if let Err(e) = transcribe_cpp::init_backends_default() {
                log::error!("бэкенды не поднялись: {e}");
            }

            let loaded_settings = settings::load(app.handle());
            // Дописываем в файл поля, которых там ещё нет: после обновления
            // приложения настройки должны быть видны целиком, а не появляться
            // по одной при первой правке.
            settings::save(app.handle(), &loaded_settings);
            let state = AppState {
                engine: Arc::new(Engine::new()),
                recorder: Recorder::spawn(),
                phase: AtomicU8::new(PHASE_NO_MODEL),
                press_started: Mutex::new(None),
                settings: Mutex::new(loaded_settings.clone()),
                last_used: Mutex::new(Instant::now()),
            };
            app.manage(state);
            app.manage(models::ModelStore::new());
            app.manage(meetings::MeetingState::new());
            // После настроек и состояния встреч: синхронизация читает и то,
            // и другое.
            sync::init(app.handle());

            hud::create(app.handle());

            apply_tray(app.handle());
            spawn_unload_watch(app.handle().clone());

            // Запуск без окна: приложение уходит в меню-бар молча.
            // Окно создаётся скрытым (см. tauri.conf.json) и показывается
            // здесь: раньше оно при запуске со стартом системы успевало
            // мигнуть на экране и только потом пряталось.
            if !loaded_settings.start_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }

            // Крестик окна прячет его, приложение остаётся жить в меню-баре.
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            spawn_update_watch(app.handle().clone());

            if let Err(e) = register_hotkey(app.handle(), &loaded_settings.hotkey) {
                log::error!("хоткей: {e}");
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    emit_state(&handle, Some(format!("Хоткей: {e}")));
                });
            }

            // Модель грузится в фоне: GGUF весит сотни мегабайт, окно
            // должно открыться сразу.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<AppState>();
                let models = handle
                    .path()
                    .app_data_dir()
                    .map(|d| d.join("models"))
                    .unwrap_or_default();
                let _ = std::fs::create_dir_all(&models);

                // Активная модель из настроек; если её ещё нет на диске —
                // первый попавшийся gguf, как раньше.
                let preferred = state
                    .settings
                    .lock()
                    .unwrap()
                    .active_model
                    .clone()
                    .map(|f| models.join(f))
                    .filter(|p| p.exists());
                match preferred.or_else(|| Engine::find_model(&models)) {
                    Some(path) => {
                        state.phase.store(PHASE_LOADING, Ordering::SeqCst);
                        emit_state(&handle, None);
                        let use_gpu = state.settings.lock().unwrap().use_gpu;
                        match state.engine.load(&path, use_gpu) {
                            Ok(()) => state.phase.store(PHASE_READY, Ordering::SeqCst),
                            Err(e) => {
                                state.phase.store(PHASE_NO_MODEL, Ordering::SeqCst);
                                emit_state(&handle, Some(format!("{e}")));
                                return;
                            }
                        }
                    }
                    None => {
                        state.phase.store(PHASE_NO_MODEL, Ordering::SeqCst);
                    }
                }
                emit_state(&handle, None);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("не удалось запустить Sol Flow");
}
