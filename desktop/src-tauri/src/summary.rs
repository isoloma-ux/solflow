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

/// Те же саммери для вебинара, интервью, подкаста: разделов «Решения» и
/// «Задачи» там не бывает, вместо них — главные мысли и что взять на
/// заметку (советы, цифры, цитаты).
const SYSTEM_PROMPT_TALK: &str = "Ты помощник, который делает саммери записей: вебинаров, \
лекций, интервью, подкастов. Тебе дают автоматическую расшифровку — в ней бывают ошибки \
распознавания и нет знаков различия говорящих.\n\nСоставь саммери на русском строго в таком \
виде:\n\n## О чем говорили\n- от четырех до восьми пунктов, пропорционально длине и \
насыщенности записи; каждый пункт — конкретная мысль или вывод, с именами и цифрами, если \
они прозвучали\n\n## Главное\n- три-пять самых важных мыслей записи\n\n## На заметку\n- \
советы, цифры, примеры и цитаты, которые стоит запомнить; если таких нет, напиши: «Ничего \
отдельного»\n\nПравила: пиши только то, что есть в расшифровке; не выдумывай имена, цифры и \
факты; не цитируй длинные куски; после раздела «На заметку» ничего не добавляй.";

const MERGE_PROMPT_TALK: &str = "Ниже — ключевые пункты последовательных частей одной \
длинной записи (вебинар, лекция, интервью или подкаст). Собери из них подробный конспект на \
русском строго в таком виде:\n\n## Главное\n- три-пять предложений: о чём запись в целом и \
какие главные выводы\n\n## Ключевые пункты\nсгруппируй пункты по темам; каждая тема — \
подзаголовок «### …» и от двух до пяти пунктов под ним; сохрани конкретику (имена, цифры, \
примеры) и не выбрасывай темы\n\n## На заметку\n- советы, цифры, примеры и цитаты, которые \
стоит запомнить; если их нет — «Ничего отдельного»\n\nЭто подробный конспект, а не краткая \
аннотация: не сжимай всё до трёх пунктов. Ничего не выдумывай и не добавляй от себя.";

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
    kind: &str,
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<String> {
    let llm = load(app, N_CTX)?;
    let parts_n = parts_for(&llm, transcript)?;
    let (system, merge) = if kind == "meeting" || kind.is_empty() {
        (SYSTEM_PROMPT, MERGE_PROMPT)
    } else {
        (SYSTEM_PROMPT_TALK, MERGE_PROMPT_TALK)
    };

    if parts_n == 1 {
        return generate(&llm, transcript, system, MAX_TOKENS, (0, 100), progress, cancelled);
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
        merge,
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

/// Один разбор записи: промпт для куска (он же для короткой записи),
/// бюджет куска, что делать с выписками — склеить кодом или свести
/// вторым проходом модели.
pub struct Breakdown {
    pub id: &'static str,
    part_prompt: &'static str,
    merge_prompt: Option<&'static str>,
    tokens: c_int,
    budget: usize,
    assemble: fn(&[String]) -> String,
    /// Нужно ли время в начале строк расшифровки.
    pub timed: bool,
}

/// Все разборы; какие показывать — решает окно по типу записи.
pub const BREAKDOWNS: &[Breakdown] = &[
    Breakdown { id: "tasks", part_prompt: TASKS_PART_PROMPT, merge_prompt: None, tokens: TASKS_TOKENS, budget: PART_BUDGET, assemble: assemble_tasks, timed: true },
    Breakdown { id: "letter", part_prompt: LETTER_PART_PROMPT, merge_prompt: Some(LETTER_MERGE_PROMPT), tokens: LETTER_TOKENS, budget: PART_BUDGET, assemble: assemble_plain, timed: false },
    Breakdown { id: "outline", part_prompt: OUTLINE_PROMPT, merge_prompt: None, tokens: OUTLINE_TOKENS, budget: PART_BUDGET / 2, assemble: assemble_outline, timed: true },
    Breakdown { id: "theses", part_prompt: THESES_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_theses, timed: true },
    Breakdown { id: "advice", part_prompt: ADVICE_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_advice, timed: true },
    Breakdown { id: "cases", part_prompt: CASES_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_cases, timed: true },
    Breakdown { id: "qa_session", part_prompt: QA_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_qa, timed: true },
    Breakdown { id: "quotes", part_prompt: QUOTES_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_quotes, timed: true },
    Breakdown { id: "guest", part_prompt: GUEST_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_guest, timed: true },
    Breakdown { id: "glossary", part_prompt: GLOSSARY_PROMPT, merge_prompt: None, tokens: LINES_TOKENS, budget: PART_BUDGET, assemble: assemble_glossary, timed: true },
    Breakdown { id: "post", part_prompt: LETTER_PART_PROMPT, merge_prompt: Some(POST_MERGE_PROMPT), tokens: POST_TOKENS, budget: PART_BUDGET, assemble: assemble_plain, timed: false },
];

pub fn breakdown(id: &str) -> Option<&'static Breakdown> {
    BREAKDOWNS.iter().find(|b| b.id == id)
}

const LINES_TOKENS: c_int = 1200;
const POST_TOKENS: c_int = 600;

/// Все промпты-выписки устроены одинаково: строка начинается со
/// слова-метки, заканчивается временем в квадратных скобках; ни примеров,
/// ни шаблонов — модель копирует их буквально.
const THESES_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выпиши главные ",
    "мысли этого текста по порядку — от четырёх до восьми, каждая мысль законченным ",
    "предложением с конкретикой. Строка о мысли начинается со слова «Тезис:».",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Своими словами по тексту, ничего ",
    "не выдумывай — ни имён, ни цифр. Если ничего такого в тексте нет, ответь одним словом: ",
    "нет. /no_think"
);
const ADVICE_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выпиши советы и ",
    "рекомендации — что говорящий предлагает делать или не делать. Строка о совете начинается ",
    "со слова «Совет:» и формулируется как действие. Не больше десяти строк.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Своими словами по тексту, ничего ",
    "не выдумывай — ни имён, ни цифр. Если ничего такого в тексте нет, ответь одним словом: ",
    "нет. /no_think"
);
const CASES_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выпиши примеры ",
    "и цифры: строка о примере или кейсе (название компании, продукта, что сделали и что ",
    "получилось) начинается со слова «Кейс:», строка о цифре (проценты, суммы, сроки, ",
    "количества — и к чему они относятся) начинается со слова «Цифра:». Не больше двенадцати ",
    "строк.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Своими словами по тексту, ничего ",
    "не выдумывай — ни имён, ни цифр. Если ничего такого в тексте нет, ответь одним словом: ",
    "нет. /no_think"
);
const QA_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выпиши вопросы, ",
    "которые задавали (ведущий, слушатели, зал), и ответы на них — по порядку. Строка с ",
    "вопросом начинается со слова «Вопрос:», строка с ответом на него — сразу следом и ",
    "начинается со слова «Ответ:», ответ — суть в одном-двух предложениях. Не больше восьми ",
    "пар.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Своими словами по тексту, ничего ",
    "не выдумывай — ни имён, ни цифр. Если ничего такого в тексте нет, ответь одним словом: ",
    "нет. /no_think"
);
const QUOTES_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выбери самые ",
    "сильные, точные и запоминающиеся фразы говорящих — такие, что годятся в заголовок или ",
    "пост. Строка начинается со слова «Цитата:», дальше фраза, как в тексте, в кавычках «», ",
    "не длиннее двадцати пяти слов; убери запинки и слова-паразиты (э-э, а-а, ну, вот, да), ",
    "поправь явные ошибки распознавания, но не смысл. От трёх до восьми строк.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Ничего не выдумывай. Если ",
    "ничего такого в тексте нет, ответь одним словом: нет. /no_think"
);
const GUEST_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка интервью или подкаста, либо её фрагмент; в тексте ",
    "бывают ошибки распознавания, каждая строка начинается с времени в квадратных скобках. ",
    "Выпиши, что стало известно о госте — о том, кому задают вопросы: кто он, чем занимается, ",
    "его опыт, проекты, позиция и взгляды, которые он высказал. Строка начинается со слова ",
    "«Факт:». Не больше десяти строк.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Своими словами по тексту, ничего ",
    "не выдумывай — ни имён, ни цифр. Если ничего такого в тексте нет, ответь одним словом: ",
    "нет. /no_think"
);
const GLOSSARY_PROMPT: &str = concat!(
    "Ниже — автоматическая расшифровка записи или её фрагмент; в тексте бывают ошибки ",
    "распознавания, каждая строка начинается с времени в квадратных скобках. Выпиши термины, ",
    "названия продуктов, компаний, инструментов и имена людей, которые встречаются в тексте и ",
    "требуют пояснения. Строка начинается со слова «Термин:», потом сам термин, тире и ",
    "короткое пояснение из контекста; если распознавание явно исказило слово, напиши, как оно ",
    "звучит правильно. Не больше двенадцати строк.",
    " Каждая находка — отдельная строка, каждую строку заканчивай временем в квадратных ",
    "скобках — временем той строки текста, откуда она взята. Ничего не выдумывай. Если ",
    "ничего такого в тексте нет, ответь одним словом: нет. /no_think"
);
const POST_MERGE_PROMPT: &str = "Ниже — ключевые пункты одной записи (выступление, интервью, \
подкаст или встреча). Напиши по ним короткий пересказ для поста в канале или соцсети на \
русском: пять-семь предложений живым языком, без заголовков, списков и вводных слов вроде \
«в этой записи», сначала главная мысль, потом два-три самых интересных факта или совета, в \
конце — вывод. Только по пунктам, ничего не выдумывай, без времени в скобках. /no_think";

/// Разбор записи по его id из [BREAKDOWNS]: текст со временем или без —
/// решает вызывающий по `timed`.
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
    let b = breakdown(kind).ok_or_else(|| anyhow!("неизвестный разбор «{kind}»"))?;
    let llm = load_path(model, N_CTX)?;
    let parts_n = parts_for_budget(&llm, text, b.budget)?;

    // Короткая запись, композиция (письмо, пост): один проход по тексту с
    // промптом «целиком», если он есть, иначе выписки и сборка кодом.
    if parts_n == 1 {
        let whole = match b.id {
            "letter" => LETTER_PROMPT,
            "post" => POST_WHOLE_PROMPT,
            _ => b.part_prompt,
        };
        let out = generate(&llm, text, whole, b.tokens, (0, 100), progress, cancelled)?;
        return Ok((b.assemble)(&[out]));
    }

    let parts = split_into(text, parts_n);
    let merged_pass = if b.merge_prompt.is_some() { 1 } else { 0 };
    let slice = 100 / (parts_n + merged_pass) as u8;
    let mut pieces = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let from = slice * i as u8;
        let found = generate(
            &llm,
            part,
            b.part_prompt,
            b.tokens,
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
    let Some(merge_prompt) = b.merge_prompt else {
        return Ok((b.assemble)(&pieces));
    };
    let merged = if pieces.is_empty() {
        "(выписок нет)".to_string()
    } else {
        pieces.join("\n\n---\n\n")
    };
    let out = generate(
        &llm,
        &merged,
        merge_prompt,
        b.tokens,
        (slice * parts_n as u8, 100),
        progress,
        cancelled,
    )?;
    Ok((b.assemble)(&[out]))
}

const POST_WHOLE_PROMPT: &str = "Тебе дают автоматическую расшифровку записи (выступление, \
интервью, подкаст или встреча); в тексте бывают ошибки распознавания. Напиши короткий \
пересказ для поста в канале или соцсети на русском: пять-семь предложений живым языком, без \
заголовков, списков и вводных слов вроде «в этой записи», сначала главная мысль, потом \
два-три самых интересных факта или совета, в конце — вывод. Только по тексту, ничего не \
выдумывай. /no_think";

/// Строка выписки как есть: маркер списка долой, а время, которое модель
/// иногда ставит в начале строки, переезжает в конец — метки в окне и
/// сборщики ждут его там.
fn normalize_line(raw: &str) -> String {
    let line = raw.trim().trim_start_matches(['-', '•', '*', ' ']).trim();
    if line.starts_with('[') {
        if let Some(close) = line.find(']') {
            let clock = &line[1..close];
            if clock_seconds(clock).is_some() {
                let rest = line[close + 1..].trim().trim_start_matches([':', '—', '–', '-', ' ']);
                if rest.ends_with(']') {
                    return rest.to_string();
                }
                return format!("{rest} [{clock}]");
            }
        }
    }
    line.to_string()
}

/// Не больше n строк: модель выписывает по восемь на кусок, и на долгой
/// записи список раздувается до полусотни.
fn take(items: Vec<String>, n: usize) -> Vec<String> {
    items.into_iter().take(n).collect()
}

fn assemble_plain(pieces: &[String]) -> String {
    pieces.join("\n").trim().to_string()
}

/// Строки с одной меткой («Тезис:», «Совет:», …) — плоский список без
/// повторов; пусто — «нет».
fn labeled_list(pieces: &[String], label: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in pieces.iter().flat_map(|p| p.lines()) {
        let line = normalize_line(raw);
        let line = line.as_str();
        let lower = line.to_lowercase();
        let Some(rest) = strip_label(&lower, line, label) else { continue };
        let rest = rest.trim().trim_matches(['*', '_']).trim().to_string();
        if rest.is_empty() || rest.eq_ignore_ascii_case("нет") {
            continue;
        }
        let key = rest.to_lowercase();
        if !out.iter().any(|g| g.to_lowercase() == key) {
            out.push(rest);
        }
    }
    out
}

fn bullets(items: &[String]) -> String {
    if items.is_empty() {
        return "- нет".to_string();
    }
    items.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n")
}

fn assemble_theses(pieces: &[String]) -> String {
    bullets(&take(labeled_list(pieces, "тезис"), 20))
}
fn assemble_advice(pieces: &[String]) -> String {
    bullets(&take(labeled_list(pieces, "совет"), 16))
}
/// Цитаты модель часто выписывает без метки — просто фразой в кавычках,
/// иногда с именем говорящего впереди. Берём и такие.
pub fn assemble_quotes(pieces: &[String]) -> String {
    let mut items = labeled_list(pieces, "цитата");
    for raw in pieces.iter().flat_map(|p| p.lines()) {
        let line = normalize_line(raw);
        let has_quote = line.contains('«') || line.matches('"').count() >= 2;
        if !has_quote || line.eq_ignore_ascii_case("нет") {
            continue;
        }
        let key = line.to_lowercase();
        if !items.iter().any(|i| i.to_lowercase() == key) {
            items.push(line);
        }
    }
    bullets(&take(items, 12))
}
fn assemble_guest(pieces: &[String]) -> String {
    bullets(&take(labeled_list(pieces, "факт"), 15))
}
fn assemble_glossary(pieces: &[String]) -> String {
    bullets(&take(labeled_list(pieces, "термин"), 20))
}
fn assemble_cases(pieces: &[String]) -> String {
    format!(
        "## Кейсы\n{}\n## Цифры\n{}",
        bullets(&take(labeled_list(pieces, "кейс"), 12)),
        bullets(&take(labeled_list(pieces, "цифра"), 12))
    )
}

/// Вопросы и ответы — по порядку: вопрос пунктом, ответ строкой под ним.
pub fn assemble_qa(pieces: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for raw in pieces.iter().flat_map(|p| p.lines()) {
        let line = normalize_line(raw);
        let line = line.as_str();
        let lower = line.to_lowercase();
        if let Some(q) = strip_label(&lower, line, "вопрос") {
            let q = q.trim().trim_matches(['*', '_']).trim();
            if q.is_empty() || seen.iter().any(|s| s == &q.to_lowercase()) {
                continue;
            }
            seen.push(q.to_lowercase());
            out.push(format!("- {q}"));
        } else if let Some(a) = strip_label(&lower, line, "ответ") {
            let a = a.trim().trim_matches(['*', '_']).trim();
            if !a.is_empty() && !out.is_empty() {
                out.push(format!("Ответ: {a}"));
            }
        }
    }
    if out.is_empty() {
        "- нет".to_string()
    } else {
        out.join("\n")
    }
}

/// Строки «Решение: …», «Задача: …», «Вопрос: …» — по разделам, без
/// повторов; пустой раздел получает «нет».
pub fn assemble_tasks(pieces: &[String]) -> String {
    let mut groups: [Vec<String>; 3] = Default::default();
    for raw in pieces.iter().flat_map(|p| p.lines()) {
        let line = normalize_line(raw);
        let line = line.as_str();
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

/// Тип записи по её началу: "meeting", "talk" (вебинар, лекция,
/// выступление), "interview" (интервью, подкаст) или "other". Как и
/// название — маленький контекст и без размышлений.
pub fn classify(app: &AppHandle, transcript_head: &str) -> Result<String> {
    classify_with(&model_path(app), transcript_head)
}

pub fn classify_with(model: &std::path::Path, transcript_head: &str) -> Result<String> {
    let llm = load_path(model, 4096)?;
    let raw = generate(
        &llm,
        transcript_head,
        "Тебе дают начало автоматической расшифровки записи; в тексте бывают ошибки \
         распознавания. Определи, что это за запись, и ответь одним словом из четырёх: \
         встреча — рабочее обсуждение нескольких людей, где что-то решают или планируют; \
         вебинар — выступление, лекция, доклад или обучение, где один говорит для многих; \
         интервью — беседа с гостем, подкаст, вопросы и ответы; другое — всё остальное. \
         Только одно слово, без пояснений. /no_think",
        24,
        (0, 100),
        |_| {},
        Arc::new(AtomicBool::new(false)),
    )?;
    let word = raw.to_lowercase();
    let kind = if word.contains("встреч") {
        "meeting"
    } else if word.contains("вебинар") || word.contains("лекци") {
        "talk"
    } else if word.contains("интервью") || word.contains("подкаст") {
        "interview"
    } else {
        "other"
    };
    Ok(kind.to_string())
}

/// Языки перевода: код → как назвать модели по-русски.
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "английский"),
    ("ru", "Русский", "русский"),
    ("de", "Deutsch", "немецкий"),
    ("es", "Español", "испанский"),
    ("fr", "Français", "французский"),
    ("it", "Italiano", "итальянский"),
    ("pt", "Português", "португальский"),
    ("tr", "Türkçe", "турецкий"),
    ("uk", "Українська", "украинский"),
    ("kk", "Қазақша", "казахский"),
    ("zh", "中文", "китайский"),
    ("ja", "日本語", "японский"),
];

pub fn language_name(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(c, _, _)| *c == code).map(|(_, _, ru)| *ru)
}

/// Сколько строк в одном заходе перевода: больше — модель начинает
/// пропускать и склеивать строки, меньше — дольше из-за накладных.
const TRANSLATE_BATCH: usize = 20;
const TRANSLATE_TOKENS: c_int = 1800;

/// Перевод строк расшифровки по номерам: модель отвечает столько же
/// пронумерованных строк, код раскладывает их обратно. Пропущенную строку
/// оставляем как есть — лучше кусок оригинала, чем дыра.
pub fn translate_lines(
    app: &AppHandle,
    lang: &str,
    lines: &[String],
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<String>> {
    translate_lines_with(&model_path(app), lang, lines, progress, cancelled)
}

pub fn translate_lines_with(
    model: &std::path::Path,
    lang: &str,
    lines: &[String],
    progress: impl Fn(u8) + Send + Sync + Clone + 'static,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<String>> {
    let name = language_name(lang).ok_or_else(|| anyhow!("неизвестный язык «{lang}»"))?;
    let llm = load_path(model, 8192)?;
    let system = format!(
        "Ты переводчик. Тебе дают пронумерованные строки автоматической расшифровки речи \
         (в них бывают ошибки распознавания и разговорные обороты). Переведи каждую строку \
         на {name} язык, естественно и по смыслу. Ответь только переведёнными строками: \
         столько же строк, в том же порядке, каждая начинается с того же номера и точки. \
         Ничего не добавляй, не объединяй, не разбивай и не пропускай строки: если в строке \
         несколько предложений, в ответе они остаются одной строкой под тем же номером. /no_think"
    );
    let batches = lines.chunks(TRANSLATE_BATCH).count().max(1);
    let mut out = Vec::with_capacity(lines.len());
    for (b, chunk) in lines.chunks(TRANSLATE_BATCH).enumerate() {
        let from = (b * 100 / batches) as u8;
        let to = ((b + 1) * 100 / batches) as u8;
        let slice = (from, to.max(from + 1).min(100));
        out.extend(translate_chunk(&llm, &system, chunk, slice, &progress, &cancelled)?);
    }
    Ok(out)
}

/// Пачка строк одним заходом. Если модель ответила не тем числом строк —
/// разбила длинную реплику надвое, и всё после неё съехало бы на одну, —
/// пачка делится пополам и переводится заново, вплоть до одной строки.
fn translate_chunk(
    llm: &Llm,
    system: &str,
    chunk: &[String],
    slice: (u8, u8),
    progress: &(impl Fn(u8) + Send + Sync + Clone + 'static),
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<String>> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("отменено"));
    }
    let numbered = chunk
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}. {}", i + 1, l.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = generate(
        llm,
        &numbered,
        system,
        TRANSLATE_TOKENS,
        slice,
        progress.clone(),
        cancelled.clone(),
    )?;
    let (parsed, seen) = parse_numbered(&raw, chunk.len());
    let complete = seen == chunk.len() && parsed.iter().all(|p| p.is_some());
    log::debug!(
        "перевод: {} строк, ответ на {seen}, целых {}",
        chunk.len(),
        parsed.iter().filter(|p| p.is_some()).count()
    );
    if complete {
        return Ok(parsed.into_iter().map(|p| p.unwrap()).collect());
    }
    if chunk.len() == 1 {
        // Одну строку модель могла разбить на несколько — склеиваем всё,
        // что пронумеровала; если не ответила вовсе — оставляем оригинал.
        let joined = parse_all_numbered(&raw).join(" ");
        return Ok(vec![if joined.is_empty() { chunk[0].clone() } else { joined }]);
    }
    let (left, right) = chunk.split_at(chunk.len() / 2);
    let mid = slice.0 + (slice.1 - slice.0) / 2;
    let mut out = translate_chunk(llm, system, left, (slice.0, mid), progress, cancelled)?;
    out.extend(translate_chunk(llm, system, right, (mid, slice.1), progress, cancelled)?);
    Ok(out)
}

/// Строки «N. текст» → по номерам; чего нет — None. Второе число —
/// сколько пронумерованных строк было в ответе вообще, включая лишние.
pub fn parse_numbered(raw: &str, n: usize) -> (Vec<Option<String>>, usize) {
    let mut out = vec![None; n];
    let mut seen = 0;
    let mut current: Option<usize> = None;
    for line in raw.lines() {
        let line = line.trim().trim_start_matches(['-', '•', '*', ' ']);
        let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
        let rest = &line[digits.len()..];
        let numbered = !digits.is_empty()
            && (rest.starts_with(". ") || rest.starts_with('.') || rest.starts_with(") "));
        if numbered {
            seen += 1;
            let idx: usize = digits.parse().unwrap_or(0);
            let text = rest.trim_start_matches(['.', ')', ' ']).trim();
            if idx >= 1 && idx <= n && out[idx - 1].is_none() {
                out[idx - 1] = Some(text.to_string());
                current = Some(idx - 1);
            } else {
                current = None;
            }
            continue;
        }
        // Продолжение предыдущей строки без номера — доклеиваем.
        if let Some(i) = current {
            if !line.is_empty() {
                if let Some(t) = out[i].as_mut() {
                    t.push(' ');
                    t.push_str(line);
                }
            }
        }
    }
    (out, seen)
}

/// Все пронумерованные строки ответа по порядку, без номеров.
fn parse_all_numbered(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches(['-', '•', '*', ' ']);
            let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
            let rest = &line[digits.len()..];
            if digits.is_empty() || !(rest.starts_with('.') || rest.starts_with(')')) {
                return None;
            }
            let text = rest.trim_start_matches(['.', ')', ' ']).trim();
            (!text.is_empty()).then(|| text.to_string())
        })
        .collect()
}

/// Перевод связного текста (саммери, разбор) с сохранением разметки.
pub fn translate_text(app: &AppHandle, lang: &str, text: &str) -> Result<String> {
    translate_text_with(&model_path(app), lang, text)
}

pub fn translate_text_with(model: &std::path::Path, lang: &str, text: &str) -> Result<String> {
    let name = language_name(lang).ok_or_else(|| anyhow!("неизвестный язык «{lang}»"))?;
    let llm = load_path(model, N_CTX)?;
    let system = format!(
        "Ты переводчик. Переведи текст на {name} язык, естественно и точно. Сохрани разметку: \
         строки, начинающиеся с «##», остаются заголовками, строки с «- » — пунктами, время в \
         квадратных скобках не меняй. Ответь только переводом, без пояснений. /no_think"
    );
    generate(&llm, text, &system, MAX_TOKENS, (0, 100), |_| {}, Arc::new(AtomicBool::new(false)))
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
    fn qa_keeps_order_and_lists_are_flat() {
        let pieces = vec![
            "Вопрос: как строить сообщество [40:08]\nОтвет: начинать с личных встреч [40:30]\nТезис: лишнее".to_string(),
        ];
        assert_eq!(
            super::assemble_qa(&pieces),
            "- как строить сообщество [40:08]\nОтвет: начинать с личных встреч [40:30]"
        );
        assert_eq!(super::assemble_theses(&["- **Тезис:** рынок сжимается [1:00]\nнет".to_string()]), "- рынок сжимается [1:00]");
        assert_eq!(super::assemble_advice(&["нет".to_string()]), "- нет");
        // Время в начале строки переезжает в конец, ответ без метки не теряется.
        assert_eq!(
            super::assemble_qa(&["[5:22] Вопрос: что делать? [5:22]\n[5:40] Ответ: работать".to_string()]),
            "- что делать? [5:22]\nОтвет: работать [5:40]"
        );
        // Цитаты без метки, но в кавычках — тоже цитаты.
        assert_eq!(
            super::assemble_quotes(&["[30:30] Иван: «Я побил рекорд»\n\"Я не верю\" [40:00]".to_string()]),
            "- Иван: «Я побил рекорд» [30:30]\n- \"Я не верю\" [40:00]"
        );
    }

    #[test]
    fn numbered_lines_parse_and_glue() {
        let raw = "1. Hello there\n2) Second line\ncontinues here\n\n4. Fourth";
        let (got, seen) = super::parse_numbered(raw, 4);
        assert_eq!(seen, 3);
        assert_eq!(got[0].as_deref(), Some("Hello there"));
        assert_eq!(got[1].as_deref(), Some("Second line continues here"));
        assert_eq!(got[2], None);
        assert_eq!(got[3].as_deref(), Some("Fourth"));
        // Лишняя строка — признак сдвига: видно по счётчику.
        let (_, seen) = super::parse_numbered("1. a\n2. b\n3. c", 2);
        assert_eq!(seen, 3);
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
