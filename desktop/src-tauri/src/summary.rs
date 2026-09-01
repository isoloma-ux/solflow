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

/// Контекст умеренный: KV-кэш на 32k токенов занимал ~5 ГБ и душил машины
/// с 16 ГБ; на 16k — вдвое меньше, а длинные встречи и так режутся на куски.
const N_CTX: c_int = 16384;
const MAX_TOKENS: c_int = 2000;

/// Сколько токенов расшифровки кладём в один кусок: остальное место
/// контекста — промпту и ответу.
const PART_BUDGET: usize = 12000;

/// Ответ обычно 300–1500 токенов вместе с размышлениями — по этой оценке
/// рисуется прогресс генерации.
const EXPECTED_ANSWER: f32 = 1500.0;

extern "C" {
    fn sf_llm_load(model_path: *const c_char, n_ctx: c_int, n_threads: c_int) -> *mut c_void;
    fn sf_llm_free(handle: *mut c_void);
    fn sf_llm_count_tokens(handle: *mut c_void, text: *const c_char) -> c_int;
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
    /// Сырой прогресс шима: 0–100 — чтение текста, 100+N — токены ответа.
    progress: Box<dyn Fn(i32) + Send + Sync>,
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
    (state.progress)(percent);
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

/// Загруженная модель: держится один раз на весь заход — перезагрузка
/// 2.4 ГБ с диска на каждый кусок съедала бы десятки секунд.
struct Llm(*mut c_void);

impl Drop for Llm {
    fn drop(&mut self) {
        unsafe { sf_llm_free(self.0) };
    }
}

/// Один проход по уже влезающему в контекст тексту. `slice` — доля общего
/// прогресса этого куска (from..to из 100): чтение текста занимает первые
/// 60% доли, генерация — остальное.
fn generate(
    llm: &Llm,
    transcript: &str,
    system: &str,
    slice: (u8, u8),
    progress: impl Fn(u8) + Send + Sync + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let sys = CString::new(system)?;
    let user = CString::new(format!("Расшифровка встречи:\n\n{transcript}"))?;
    let (from, to) = slice;
    let span = (to - from) as f32;
    let state = GenState {
        out: Mutex::new(String::new()),
        progress: Box::new(move |raw: i32| {
            let pct = if raw > 100 {
                // Генерация: оцениваем по ожидаемой длине ответа.
                let g = ((raw - 100) as f32 / EXPECTED_ANSWER).min(1.0);
                from + (span * (0.6 + 0.4 * g)) as u8
            } else {
                from + (raw.clamp(0, 100) as f32 / 100.0 * span * 0.6) as u8
            };
            progress(pct.min(99));
        }),
        cancelled,
    };

    let rc = unsafe {
        sf_llm_generate(
            llm.0,
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

    match rc {
        0 => Ok(state.out.into_inner().unwrap().trim().to_string()),
        -4 => Err(anyhow!("расшифровка не влезла в контекст")),
        -5 => Err(anyhow!("отменено")),
        _ => Err(anyhow!("генерация не удалась ({rc})")),
    }
}

/// Полное саммери: длинная расшифровка режется на куски по бюджету токенов,
/// куски сводятся финальным проходом. Модель грузится один раз.
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

    let path = CString::new(model.to_string_lossy().as_bytes())?;
    let handle = unsafe { sf_llm_load(path.as_ptr(), N_CTX, 0) };
    if handle.is_null() {
        return Err(anyhow!("модель саммери не загрузилась"));
    }
    let llm = Llm(handle);

    let text_c = CString::new(transcript)?;
    let tokens = unsafe { sf_llm_count_tokens(llm.0, text_c.as_ptr()) };
    if tokens < 0 {
        return Err(anyhow!("текст не токенизировался"));
    }
    let parts_n = ((tokens as usize).div_ceil(PART_BUDGET)).max(1);

    if parts_n == 1 {
        return generate(&llm, transcript, SYSTEM_PROMPT, (0, 100), progress, cancelled);
    }

    // Куски + финальный свод делят полосу прогресса поровну.
    let parts = split_into(transcript, parts_n);
    let slice = 100 / (parts_n + 1) as u8;
    let mut partials = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let from = slice * i as u8;
        partials.push(generate(
            &llm,
            part,
            SYSTEM_PROMPT,
            (from, from + slice),
            progress.clone(),
            cancelled.clone(),
        )?);
    }
    let merged = partials.join("\n\n---\n\n");
    generate(
        &llm,
        &merged,
        "Ниже — саммери частей одной встречи. Сведи их в одно общее саммери \
         в том же формате: «## О чем говорили» (до пяти пунктов), «## Решения», \
         «## Задачи». Не повторяйся и ничего не добавляй от себя.",
        (slice * parts_n as u8, 100),
        progress,
        cancelled,
    )
}

/// Режет текст на n примерно равных кусков по границам предложений.
fn split_into(text: &str, n: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let step = text.len() / n;
    let mut start = 0;
    for i in 1..n {
        let mut cut = (step * i).max(start + 1).min(text.len());
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        // К ближайшей границе предложения справа, чтобы не рвать мысль.
        if let Some(dot) = text[cut..].find(". ") {
            cut += dot + 1;
        }
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        if cut > start && cut < text.len() {
            parts.push(text[start..cut].to_string());
            start = cut;
        }
    }
    parts.push(text[start..].to_string());
    parts
}
