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
/// температура низкая, штраф повторов обязателен. Для коротких записей —
/// сжатое саммери; длинные идут кусками через [PART_PROMPT] и [MERGE_PROMPT].
const SYSTEM_PROMPT: &str = "Ты помощник, который делает саммери рабочих встреч и интервью. \
Тебе дают автоматическую расшифровку — в ней бывают ошибки распознавания и нет знаков \
различия говорящих.\n\nСоставь саммери на русском строго в таком виде:\n\n## О чем говорили\n\
- от четырех до восьми пунктов, пропорционально длине и насыщенности разговора; каждый пункт — \
конкретная мысль или вывод, с именами и цифрами, если они прозвучали\n\n## Решения\n- что \
решили; если явных решений не было, напиши: «Явных решений не зафиксировано»\n\n## Задачи\n\
- кто что делает дальше, если это прозвучало; если нет — «Задачи не проговаривались»\n\n\
Правила: пиши только то, что есть в расшифровке; не выдумывай имена, цифры и факты; не \
цитируй длинные куски; после раздела «Задачи» ничего не добавляй.";

/// Кусок длинной записи: не сжимать, а выписать конкретику — из этих
/// пунктов потом собирается общий конспект.
const PART_PROMPT: &str = "Тебе дают фрагмент автоматической расшифровки длинного разговора \
(встречи или интервью); в тексте бывают ошибки распознавания. Выпиши ключевые пункты этого \
фрагмента: от шести до десяти, каждый — конкретная мысль, факт, договорённость или вывод, с \
именами и цифрами, если они прозвучали. Если во фрагменте были решения или поставленные \
задачи — добавь их отдельными строками «Решение: …» и «Задача: …». Пиши только по содержанию \
фрагмента, ничего не выдумывай и не добавляй выводов от себя.";

/// Свод кусков: подробный конспект по темам, а не аннотация в три строки.
const MERGE_PROMPT: &str = "Ниже — ключевые пункты последовательных частей одного длинного \
разговора. Собери из них подробный конспект на русском строго в таком виде:\n\n## Главное\n\
- три-пять предложений: о чём разговор в целом и к чему пришли\n\n## Ключевые пункты\n\
сгруппируй пункты по темам; каждая тема — подзаголовок «### …» и от двух до пяти пунктов под \
ним; сохрани конкретику (имена, цифры, договорённости) и не выбрасывай темы\n\n## Решения\n\
- все решения из частей; если их нет — «Явных решений не зафиксировано»\n\n## Задачи\n- все \
задачи из частей; если их нет — «Задачи не проговаривались»\n\nЭто подробный конспект, а не \
краткая аннотация: не сжимай всё до трёх пунктов. Ничего не выдумывай и не добавляй от себя.";

/// Вопрос к записи: ответ с опорой на время реплик. Размышления выключены —
/// на процессоре они растягивали бы ответ на минуты, а вопрос — про то,
/// что уже сказано в тексте, думать тут не над чем.
const ASK_PROMPT: &str = "Ты отвечаешь на вопросы по автоматической расшифровке разговора \
(встреча, интервью, лекция). В тексте бывают ошибки распознавания. Каждая строка \
расшифровки начинается с времени в квадратных скобках, например [12:40] или [1:05:12]. \
Отвечай на русском, коротко и по делу: сначала сам ответ в одном-трёх предложениях, потом, \
если есть что добавить, до пяти пунктов с подробностями. После каждого утверждения ставь \
время места в записи, откуда оно взято, в том же виде — [12:40]. Пиши только то, что есть в \
расшифровке; если ответа в ней нет, напиши: «В записи об этом не говорили». /no_think";

/// Кусок длинной записи: не отвечать, а выписать всё, что относится к
/// вопросу, — ответ соберётся из выписок.
const ASK_PART_PROMPT: &str = "Ниже — фрагмент автоматической расшифровки длинного \
разговора; в тексте бывают ошибки распознавания, каждая строка начинается с времени в \
квадратных скобках. Выпиши из фрагмента всё, что относится к вопросу: близкие к тексту \
цитаты или пересказ, каждая строка — с временем из начала её строки в том же виде, \
например [12:40]. До восьми строк. Если во фрагменте нет ничего по вопросу, ответь одним \
словом: нет. /no_think";

const ASK_MERGE_PROMPT: &str = "Ниже — выписки из разных частей одной длинной расшифровки, \
собранные по вопросу; у каждой выписки есть время в квадратных скобках. Ответь на вопрос \
по этим выпискам на русском, коротко и по делу: сначала сам ответ в одном-трёх \
предложениях, потом до пяти пунктов с подробностями. После каждого утверждения ставь время \
из выписки в том же виде — [12:40]. Ничего не выдумывай. Если выписок нет или в них нет \
ответа на вопрос, весь ответ — одна фраза «В записи об этом не говорили»; если ответ \
есть, этой фразы быть не должно. /no_think";

const ASK_TOKENS: c_int = 700;

/// Разборы записи той же моделью: решения и задачи, письмо по итогам,
/// оглавление. Формат жёсткий, размышления выключены — это выписки из
/// текста, а не сочинение.
/// Решения и задачи: модель только выписывает строки трёх типов, разделы
/// собирает код. Маленькая модель копирует любой пример или шаблон из
/// промпта буквально — вплоть до выдуманной «Марины к пятнице», — поэтому
/// в промпте нет ни примеров, ни шаблонов, только слова-метки строк.
const TASKS_PART_PROMPT: &str = "Ниже — автоматическая расшифровка разговора или её \
фрагмент; в тексте бывают ошибки распознавания, каждая строка начинается с времени в \
квадратных скобках. Выпиши из текста принятые решения, поставленные задачи и открытые \
вопросы (что обсуждали, но не решили). Каждая находка — отдельная строка. Строка о решении \
начинается со слова «Решение:», о задаче — со слова «Задача:», о вопросе — со слова \
«Вопрос:». В задаче сначала назови, кто её взял, если это прозвучало, потом суть, потом \
срок, если он был. Каждую строку заканчивай временем в квадратных скобках — временем той \
строки текста, откуда взята находка. Не больше десяти строк, своими словами по тексту, \
ничего не выдумывай — ни имён, ни сроков, ни цифр. Если ничего такого в тексте нет, ответь \
одним словом: нет. /no_think";

const LETTER_PROMPT: &str = "Тебе дают автоматическую расшифровку рабочей встречи; в тексте \
бывают ошибки распознавания. Напиши участникам письмо по итогам встречи на русском, деловым \
и живым тоном, без канцелярита. Строго в таком виде:\n\nКоллеги, добрый день!\n\nодин \
абзац: о чём была встреча и к чему пришли\n\nДоговорились:\n- пункты\n\nЗадачи:\n- кто: \
что — срок, если прозвучал\n\nСледующие шаги:\n- пункты\n\nС уважением,\n[Имя]\n\nПиши \
только то, что было на встрече, не выдумывай имена, цифры и сроки; времени в скобках в \
письме быть не должно; если задач или шагов не было, пропусти этот раздел. /no_think";

const LETTER_MERGE_PROMPT: &str = "Ниже — ключевые пункты последовательных частей одной \
рабочей встречи. Напиши по ним участникам письмо по итогам встречи на русском, деловым и \
живым тоном, без канцелярита. Строго в таком виде:\n\nКоллеги, добрый день!\n\nодин абзац: \
о чём была встреча и к чему пришли\n\nДоговорились:\n- пункты\n\nЗадачи:\n- кто: что — \
срок, если прозвучал\n\nСледующие шаги:\n- пункты\n\nС уважением,\n[Имя]\n\nПиши только \
по пунктам, не выдумывай имена, цифры и сроки; времени в скобках в письме быть не должно; \
если задач или шагов не было, пропусти этот раздел. /no_think";

/// Оглавление: куски мельче обычных (модель бросала вторую половину
/// длинного куска), темы просит по две-пять на кусок, а итог прореживает
/// код — темы ближе трёх минут друг к другу склеиваются.
const OUTLINE_PROMPT: &str = "Ниже — автоматическая расшифровка разговора (встреча, \
интервью, лекция) или её фрагмент; в тексте бывают ошибки распознавания, каждая строка \
начинается с времени в квадратных скобках. Разбей этот текст на крупные темы по порядку: от \
двух до пяти тем, каждая тема объединяет много строк и длится несколько минут; темы должны \
покрыть текст от начала до конца. Каждая тема — одна строка: сначала время в квадратных \
скобках той строки текста, где тема началась, потом название темы своими словами (без \
нумерации и без слова «тема»), потом тире и одно предложение о том, что в ней говорили. \
Только эти строки, без заголовков и пояснений. /no_think";

const LETTER_PART_PROMPT: &str = concat!(
    "Тебе дают фрагмент автоматической расшифровки длинного разговора (встречи или ",
    "интервью); в тексте бывают ошибки распознавания. Выпиши ключевые пункты этого фрагмента: ",
    "от шести до десяти, каждый — конкретная мысль, факт, договорённость или вывод, с именами ",
    "и цифрами, если они прозвучали. Если во фрагменте были решения или поставленные задачи — ",
    "добавь их отдельными строками «Решение: …» и «Задача: …». Пиши только по содержанию ",
    "фрагмента, ничего не выдумывай и не добавляй выводов от себя. /no_think"
);

const TASKS_TOKENS: c_int = 1500;
const LETTER_TOKENS: c_int = 1200;
const OUTLINE_TOKENS: c_int = 900;

/// Контекст умеренный: KV-кэш на 32k токенов занимал ~5 ГБ и душил машины
/// с 16 ГБ; на 16k — вдвое меньше, а длинные встречи и так режутся на куски.
const N_CTX: c_int = 16384;
const MAX_TOKENS: c_int = 2000;
/// Куску хватает пунктов на тысячу токенов; своду нужно больше — это
/// подробный конспект всей записи.
const PART_TOKENS: c_int = 1600;
const MERGE_TOKENS: c_int = 3500;

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
    fn sf_llm_devices(out: *mut c_char, cap: c_int) -> c_int;
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

/// Какие вычислители видит llama — «NVIDIA ... (Vulkan), CPU». Пустая
/// строка, если библиотека не смогла ответить.
pub fn devices() -> String {
    let mut buf = vec![0u8; 512];
    let n = unsafe { sf_llm_devices(buf.as_mut_ptr() as *mut c_char, buf.len() as c_int) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf8_lossy(&buf[..n as usize]).to_string()
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
    max_tokens: c_int,
    slice: (u8, u8),
    progress: impl Fn(u8) + Send + Sync + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let sys = CString::new(system)?;
    let user = CString::new(format!("Текст:\n\n{transcript}"))?;
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
            max_tokens,
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

/// Загрузка модели на один заход.
fn load(app: &AppHandle, n_ctx: c_int) -> Result<Llm> {
    load_path(&model_path(app), n_ctx)
}

fn load_path(model: &std::path::Path, n_ctx: c_int) -> Result<Llm> {
    if !model.exists() {
        return Err(anyhow!("модель саммери не скачана"));
    }
    let path = CString::new(model.to_string_lossy().as_bytes())?;
    let handle = unsafe { sf_llm_load(path.as_ptr(), n_ctx, 0) };
    if handle.is_null() {
        return Err(anyhow!("модель саммери не загрузилась"));
    }
    Ok(Llm(handle))
}

/// На сколько кусков резать текст, чтобы каждый влез в бюджет контекста.
fn parts_for(llm: &Llm, text: &str) -> Result<usize> {
    parts_for_budget(llm, text, PART_BUDGET)
}

fn parts_for_budget(llm: &Llm, text: &str, budget: usize) -> Result<usize> {
    let text_c = CString::new(text)?;
    let tokens = unsafe { sf_llm_count_tokens(llm.0, text_c.as_ptr()) };
    if tokens < 0 {
        return Err(anyhow!("текст не токенизировался"));
    }
    Ok(((tokens as usize).div_ceil(budget)).max(1))
}

/// Полное саммери: длинная расшифровка режется на куски по бюджету токенов,
/// куски сводятся финальным проходом. Модель грузится один раз.
pub fn summarize(
    app: &AppHandle,
    transcript: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let llm = load(app, N_CTX)?;
    let parts_n = parts_for(&llm, transcript)?;

    if parts_n == 1 {
        return generate(&llm, transcript, SYSTEM_PROMPT, MAX_TOKENS, (0, 100), progress, cancelled);
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
            PART_PROMPT,
            PART_TOKENS,
            (from, from + slice),
            progress.clone(),
            cancelled.clone(),
        )?);
    }
    let merged = partials.join("\n\n---\n\n");
    generate(
        &llm,
        &merged,
        MERGE_PROMPT,
        MERGE_TOKENS,
        (slice * parts_n as u8, 100),
        progress,
        cancelled,
    )
}

/// Ответ на вопрос по расшифровке. `timed` — текст, где каждая строка
/// начинается с времени «[мм:сс]»: по этим меткам модель ссылается на
/// места записи, а окно превращает их в переходы к репликам. Длинная
/// запись идёт кусками: из каждого выписывается относящееся к вопросу,
/// ответ собирается по выпискам.
pub fn ask(
    app: &AppHandle,
    timed: &str,
    question: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    ask_with(&model_path(app), timed, question, progress, cancelled)
}

/// То же по пути к модели — для проверок без приложения.
pub fn ask_with(
    model: &std::path::Path,
    timed: &str,
    question: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let question = question.trim();
    if question.is_empty() {
        return Err(anyhow!("вопрос пустой"));
    }
    let llm = load_path(model, N_CTX)?;
    let parts_n = parts_for(&llm, timed)?;
    let with_question = |prompt: &str| format!("{prompt}\n\nВопрос: {question}");

    if parts_n == 1 {
        return generate(
            &llm,
            timed,
            &with_question(ASK_PROMPT),
            ASK_TOKENS,
            (0, 100),
            progress,
            cancelled,
        );
    }

    let parts = split_into(timed, parts_n);
    let slice = 100 / (parts_n + 1) as u8;
    let mut notes = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let from = slice * i as u8;
        let found = generate(
            &llm,
            part,
            &with_question(ASK_PART_PROMPT),
            ASK_TOKENS,
            (from, from + slice),
            progress.clone(),
            cancelled.clone(),
        )?;
        let cleaned = found.trim().trim_end_matches('.');
        log::debug!("вопрос, кусок {}/{parts_n}: {found}", i + 1);
        if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case("нет") {
            notes.push(found);
        }
    }
    let merged = if notes.is_empty() {
        "(выписок нет)".to_string()
    } else {
        notes.join("\n\n---\n\n")
    };
    generate(
        &llm,
        &merged,
        &with_question(ASK_MERGE_PROMPT),
        ASK_TOKENS,
        (slice * parts_n as u8, 100),
        progress,
        cancelled,
    )
}

/// Разбор записи: "tasks" — решения и задачи (текст со временем), "letter" —
/// письмо по итогам (текст без времени), "outline" — оглавление (текст со
/// временем; куски просто склеиваются — темы и так идут по порядку).
pub fn derive(
    app: &AppHandle,
    kind: &str,
    text: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    derive_with(&model_path(app), kind, text, progress, cancelled)
}

pub fn derive_with(
    model: &std::path::Path,
    kind: &str,
    text: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let llm = load_path(model, N_CTX)?;
    // Выписки для письма — без размышлений: с ними куски считались вдвое
    // дольше, а качества письму это не прибавляло.
    let (whole, part_prompt, merge_prompt, tokens, budget) = match kind {
        "tasks" => (TASKS_PART_PROMPT, TASKS_PART_PROMPT, "", TASKS_TOKENS, PART_BUDGET),
        "letter" => (
            LETTER_PROMPT,
            LETTER_PART_PROMPT,
            LETTER_MERGE_PROMPT,
            LETTER_TOKENS,
            PART_BUDGET,
        ),
        "outline" => (OUTLINE_PROMPT, OUTLINE_PROMPT, "", OUTLINE_TOKENS, PART_BUDGET / 2),
        other => return Err(anyhow!("неизвестный разбор «{other}»")),
    };
    let parts_n = parts_for_budget(&llm, text, budget)?;

    if parts_n == 1 {
        let out = generate(&llm, text, whole, tokens, (0, 100), progress, cancelled)?;
        return Ok(assemble(kind, &[out]));
    }

    let parts = split_into(text, parts_n);
    let merged_pass = if merge_prompt.is_empty() { 0 } else { 1 };
    let slice = 100 / (parts_n + merged_pass) as u8;
    let mut pieces = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let from = slice * i as u8;
        let found = generate(
            &llm,
            part,
            part_prompt,
            tokens,
            (from, from + slice),
            progress.clone(),
            cancelled.clone(),
        )?;
        log::debug!("разбор {kind}, кусок {}/{parts_n}: {found}", i + 1);
        let cleaned = found.trim().trim_end_matches('.');
        if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case("нет") {
            pieces.push(found);
        }
    }
    if merge_prompt.is_empty() {
        return Ok(assemble(kind, &pieces));
    }
    let merged = if pieces.is_empty() {
        "(выписок нет)".to_string()
    } else {
        pieces.join("\n\n---\n\n")
    };
    generate(
        &llm,
        &merged,
        merge_prompt,
        tokens,
        (slice * parts_n as u8, 100),
        progress,
        cancelled,
    )
}

/// Итог из выписок кусков — кодом, без второго прохода модели.
fn assemble(kind: &str, pieces: &[String]) -> String {
    match kind {
        "tasks" => assemble_tasks(pieces),
        "outline" => assemble_outline(pieces),
        _ => pieces.join("\n"),
    }
}

/// Строки «Решение: …», «Задача: …», «Вопрос: …» — по разделам, без
/// повторов; пустой раздел получает «нет».
pub fn assemble_tasks(pieces: &[String]) -> String {
    let mut groups: [Vec<String>; 3] = Default::default();
    for line in pieces.iter().flat_map(|p| p.lines()) {
        let line = line.trim().trim_start_matches(['-', '•', '*', ' ']).trim();
        let lower = line.to_lowercase();
        let (slot, rest) = if let Some(r) = strip_label(&lower, line, "решение") {
            (0, r)
        } else if let Some(r) = strip_label(&lower, line, "задача") {
            (1, r)
        } else if let Some(r) = strip_label(&lower, line, "вопрос") {
            (2, r)
        } else {
            continue;
        };
        let rest = rest.trim().trim_matches(['*', '_']).trim().to_string();
        if rest.is_empty() || rest.eq_ignore_ascii_case("нет") {
            continue;
        }
        let key = rest.to_lowercase();
        if !groups[slot].iter().any(|g| g.to_lowercase() == key) {
            groups[slot].push(rest);
        }
    }
    let mut out = String::new();
    for (title, items) in ["Решения", "Задачи", "Открытые вопросы"].iter().zip(groups.iter()) {
        out.push_str(&format!("## {title}\n"));
        if items.is_empty() {
            out.push_str("- нет\n");
        }
        for item in items {
            out.push_str(&format!("- {item}\n"));
        }
    }
    out.trim_end().to_string()
}

/// Остаток строки после метки вроде «Задача:» (регистр и звёздочки
/// разметки не важны).
fn strip_label<'a>(lower: &str, line: &'a str, label: &str) -> Option<&'a str> {
    let start = lower.trim_start_matches(['*', '_', ' ']);
    let skipped = lower.len() - start.len();
    if !start.starts_with(label) {
        return None;
    }
    let after = &line[skipped + label.len()..];
    let after = after.trim_start_matches(['*', '_', ' ']);
    after.strip_prefix(':').or_else(|| after.strip_prefix('—').or_else(|| after.strip_prefix('-')))
}

/// Строки «[мм:сс] тема — о чём» по времени, темы ближе трёх минут друг к
/// другу склеиваются (остаётся первая), не больше тридцати.
pub fn assemble_outline(pieces: &[String]) -> String {
    const MIN_GAP: u32 = 180;
    let mut rows: Vec<(u32, String)> = Vec::new();
    for line in pieces.iter().flat_map(|p| p.lines()) {
        let line = line.trim().trim_start_matches(['-', '•', '*', ' ']).trim();
        let Some(close) = line.find(']') else { continue };
        if !line.starts_with('[') {
            continue;
        }
        let Some(secs) = clock_seconds(&line[1..close]) else { continue };
        // После времени модель иногда ставит тире — оно тут лишнее.
        let body = line[close + 1..].trim().trim_start_matches(['–', '—', '-', ' ']);
        if body.is_empty() {
            continue;
        }
        rows.push((secs, format!("[{}] {body}", &line[1..close])));
    }
    rows.sort_by_key(|(s, _)| *s);
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<u32> = None;
    for (secs, text) in rows {
        if let Some(prev) = last {
            if secs < prev + MIN_GAP {
                continue;
            }
        }
        last = Some(secs);
        out.push(text);
        if out.len() >= 30 {
            break;
        }
    }
    out.join("\n")
}

/// «12:40» или «1:02:34» → секунды.
fn clock_seconds(text: &str) -> Option<u32> {
    let mut total = 0u32;
    for part in text.trim().split(':') {
        total = total.checked_mul(60)?.checked_add(part.trim().parse().ok()?)?;
    }
    Some(total)
}

/// Короткое название записи по началу расшифровки. Контекст маленький —
/// ради пары слов не разворачиваем гигабайтный KV-кэш.
pub fn title(app: &AppHandle, transcript_head: &str) -> Result<String> {
    let llm = load(app, 4096)?;

    // /no_think обязателен: на размышления модель тратила весь короткий
    // бюджет токенов и до самого названия не доходила (проверено — после
    // расшифровки название «не появлялось»).
    let raw = generate(
        &llm,
        transcript_head,
        "Тебе дают начало автоматической расшифровки записи (встреча, интервью, лекция \
         или заметка); в тексте бывают ошибки распознавания. Придумай короткое название \
         этой записи на русском: от двух до пяти слов, по сути разговора. Ответь только \
         самим названием — без кавычек, точки в конце и пояснений. /no_think",
        96,
        (0, 100),
        |_| {},
        Arc::new(AtomicBool::new(false)),
    )?;

    // Модель иногда всё же заворачивает ответ в кавычки или льёт лишнее —
    // берём первую строку и чистим края.
    let title = raw
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| "«»\"'“”.".contains(c))
        .trim()
        .to_string();
    // Слишком длинное «название» — признак, что модель ушла в пересказ.
    if title.chars().count() > 60 {
        return Ok(String::new());
    }
    Ok(title)
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

#[cfg(test)]
mod tests {
    use super::{assemble_outline, assemble_tasks};

    #[test]
    fn tasks_group_and_dedupe() {
        let pieces = vec![
            "Решение: делать гостиную [12:40]\n- **Задача:** Иван собирает список к пятнице [13:00]\nВопрос: как строить сообщество [40:08]".to_string(),
            "задача: Иван собирает список к пятнице [13:00]\nнет".to_string(),
        ];
        let out = assemble_tasks(&pieces);
        assert_eq!(
            out,
            "## Решения\n- делать гостиную [12:40]\n## Задачи\n- Иван собирает список к пятнице [13:00]\n## Открытые вопросы\n- как строить сообщество [40:08]"
        );
        assert!(assemble_tasks(&["нет".to_string()]).contains("## Задачи\n- нет"));
    }

    #[test]
    fn outline_sorts_and_thins() {
        let pieces = vec![
            "[12:40] – Партнёры — о чём\n[13:10] Слишком близко — лишняя\n- [0:29] Вступление — начало".to_string(),
            "[1:02:34] Тренды — вторая половина".to_string(),
        ];
        assert_eq!(
            assemble_outline(&pieces),
            "[0:29] Вступление — начало\n[12:40] Партнёры — о чём\n[1:02:34] Тренды — вторая половина"
        );
    }
}
