//! Саммери встречи локальной языковой моделью через libsolflow_llama
//! (узкий C-интерфейс поверх llama.cpp — см. llama-shim/).
//!
//! Модель тяжёлая (гигабайты), поэтому грузится на время работы и сразу
//! выгружается: держать её в памяти рядом с моделью расшифровки — слишком
//! жирно. Очередь одна на все саммери — движок один.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Manager};

/// Имя файла модели в папке моделей приложения. Одно, знакомое каталогу.
pub const MODEL_FILE: &str = "Qwen3-4B-Q4_K_M.gguf";
pub const MODEL_URL: &str =
    "https://huggingface.co/unsloth/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf";
pub const MODEL_MB: u64 = 2440;

/// Подобрано на живых встречах (см. память проекта): формат жёсткий,
/// температура низкая, штраф повторов обязателен.
const SYSTEM_PROMPT: &str = "Ты помощник, который делает краткие саммери рабочих встреч. \
Тебе дают автоматическую расшифровку — в ней бывают ошибки распознавания и нет знаков \
различия говорящих.\n\nСоставь саммери на русском строго в таком виде:\n\n## О чем говорили\n\
- от трех до пяти пунктов, каждый одним предложением\n\n## Решения\n- что решили; если явных \
решений не было, напиши: «Явных решений не зафиксировано»\n\n## Задачи\n- кто что делает \
дальше, если это прозвучало; если нет — «Задачи не проговаривались»\n\nПравила: пиши только \
то, что есть в расшифровке; не выдумывай имена, цифры и факты; не цитируй длинные куски; \
после раздела «Задачи» ничего не добавляй.";

const N_CTX: c_int = 32768;
const MAX_TOKENS: c_int = 2500;

extern "C" {
    fn sf_llm_load(model_path: *const c_char, n_ctx: c_int, n_threads: c_int) -> *mut c_void;
    fn sf_llm_free(handle: *mut c_void);
    fn sf_llm_generate(
        handle: *mut c_void,
        system_prompt: *const c_char,
        user_prompt: *const c_char,
        max_tokens: c_int,
        temperature: f32,
        repeat_penalty: f32,
        on_piece: extern "C" fn(*const c_char, c_int, *mut c_void),
        on_progress: extern "C" fn(c_int, *mut c_void),
        should_stop: extern "C" fn(*mut c_void) -> bool,
        userdata: *mut c_void,
    ) -> c_int;
}

struct GenState {
    out: Mutex<String>,
    progress: Box<dyn Fn(u8) + Send + Sync>,
    cancelled: Arc<AtomicBool>,
}

extern "C" fn on_piece(piece: *const c_char, len: c_int, ud: *mut c_void) {
    let state = unsafe { &*(ud as *const GenState) };
    let bytes = unsafe { std::slice::from_raw_parts(piece as *const u8, len as usize) };
    if let Ok(text) = std::str::from_utf8(bytes) {
        state.out.lock().unwrap().push_str(text);
    } else {
        // Токен разрезал многобайтовый символ — докидываем как есть,
        // String соберётся из валидных кусков ниже по потоку.
        state
            .out
            .lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(bytes));
    }
}

extern "C" fn on_progress(percent: c_int, ud: *mut c_void) {
    let state = unsafe { &*(ud as *const GenState) };
    (state.progress)(percent.clamp(0, 100) as u8);
}

extern "C" fn should_stop(ud: *mut c_void) -> bool {
    let state = unsafe { &*(ud as *const GenState) };
    state.cancelled.load(Ordering::Relaxed)
}

pub fn model_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("models").join(MODEL_FILE))
        .unwrap_or_default()
}

pub fn model_ready(app: &AppHandle) -> bool {
    model_path(app).exists()
}

/// Скачивание модели саммери — тем же способом, что эмбеддинги диаризации.
pub fn download(
    app: &AppHandle,
    on_progress: &dyn Fn(u8),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let target = model_path(app);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = target.with_extension("part");
    let _ = std::fs::remove_file(&tmp);

    let expected = MODEL_MB * 1024 * 1024;
    let percent = |done: u64, total: u64| {
        let total = if total > 0 { total } else { expected };
        on_progress(((done * 100 / total).min(99)) as u8);
    };
    crate::net::download(MODEL_URL, &tmp, &percent, cancelled)?;
    std::fs::rename(&tmp, &target)?;
    Ok(())
}

/// Саммери одним проходом. Расшифровка длиннее контекста режется на куски
/// в [summarize] — сюда приходит уже влезающий текст.
fn generate(
    model: &PathBuf,
    transcript: &str,
    system: &str,
    progress: impl Fn(u8) + Send + Sync + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let path = CString::new(model.to_string_lossy().as_bytes())?;
    let handle = unsafe { sf_llm_load(path.as_ptr(), N_CTX, 0) };
    if handle.is_null() {
        return Err(anyhow!("модель саммери не загрузилась"));
    }

    let sys = CString::new(system)?;
    let user = CString::new(format!("Расшифровка встречи:\n\n{transcript}"))?;
    let state = GenState {
        out: Mutex::new(String::new()),
        progress: Box::new(progress),
        cancelled,
    };

    let rc = unsafe {
        sf_llm_generate(
            handle,
            sys.as_ptr(),
            user.as_ptr(),
            MAX_TOKENS,
            0.4,
            1.15,
            on_piece,
            on_progress,
            should_stop,
            &state as *const GenState as *mut c_void,
        )
    };
    unsafe { sf_llm_free(handle) };

    match rc {
        0 => Ok(state.out.into_inner().unwrap().trim().to_string()),
        -4 => Err(anyhow!("расшифровка не влезла в контекст")),
        -5 => Err(anyhow!("отменено")),
        _ => Err(anyhow!("генерация не удалась ({rc})")),
    }
}

/// Полное саммери: если расшифровка не влезает в контекст целиком, режем
/// пополам по границе реплик и сводим саммери кусков вторым проходом.
pub fn summarize(
    app: &AppHandle,
    transcript: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let model = model_path(app);
    if !model.exists() {
        return Err(anyhow!("модель саммери не скачана"));
    }

    match generate(&model, transcript, SYSTEM_PROMPT, progress.clone(), cancelled.clone()) {
        Err(e) if e.to_string().contains("не влезла") => {
            // Двухчасовые встречи: куски по половине, потом свод.
            let parts = split_in_half(transcript);
            let mut partials = Vec::new();
            for (i, part) in parts.iter().enumerate() {
                let base = (i as u8) * 45;
                let p = progress.clone();
                let sub =
                    move |pct: u8| p(base + (pct as u16 * 45 / 100) as u8);
                partials.push(generate(
                    &model,
                    part,
                    SYSTEM_PROMPT,
                    sub,
                    cancelled.clone(),
                )?);
            }
            let merged = partials.join("\n\n---\n\n");
            let p = progress.clone();
            generate(
                &model,
                &merged,
                "Ниже — саммери частей одной встречи. Сведи их в одно общее саммери \
                 в том же формате: «## О чем говорили» (до пяти пунктов), «## Решения», \
                 «## Задачи». Не повторяйся и ничего не добавляй от себя.",
                move |pct| p(90 + (pct as u16 / 10) as u8),
                cancelled,
            )
        }
        other => other,
    }
}

/// Режет текст примерно пополам по границе предложения.
fn split_in_half(text: &str) -> Vec<String> {
    let mid = text.len() / 2;
    let cut = text[mid..]
        .find(". ")
        .map(|i| mid + i + 1)
        .unwrap_or(mid.min(text.len()));
    // Граница могла попасть внутрь многобайтового символа — двигаемся к ней.
    let mut cut = cut;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    vec![text[..cut].to_string(), text[cut..].to_string()]
}
