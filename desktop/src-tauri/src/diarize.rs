//! Разделение говорящих — порт Diarizer с Android на Rust поверх того же
//! sherpa-onnx (C API, статически влинкован).
//!
//! Схема та же, что на телефоне: запись режется на окна по десять минут,
//! каждое окно диаризуется отдельно, а потом локальные говорящие окон
//! сшиваются в общих по эмбеддингам голосов. Иначе двухчасовая встреча не
//! влезает в память, а нумерация говорящих в разных окнах не совпадает.

use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::wav::{WavReader, SAMPLE_RATE};

// --- пороги (выверены на Android, менять только с замерами) ----------------

/// Порог кластеризации внутри окна — им пользуется сам sherpa.
const CLUSTER_THRESHOLD: f32 = 0.5;

/// Если даже самое дорогое слияние дешевле этого порога, вся запись — один
/// голос. Замер на двух чтецах: свои пары 0.13–0.20, чужие от 0.25.
const SINGLE_VOICE_DISTANCE: f32 = 0.3;

const WINDOW_SEC: usize = 600;
const EMBED_SPEECH_SEC: f32 = 10.0;
const THREADS: i32 = 4;

/// Сколько речи делает локального говорящего «крупным».
const BIG_SPEAKER_SEC: f32 = 30.0;

/// Эмбеддинг-модель качается: 28 МБ в бинаре ради редкой операции не нужны.
const EMB_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx";
const EMB_BYTES: u64 = 28_281_164;

/// Сколько мегабайт качать — для текста в окне.
pub const DOWNLOAD_MB: u64 = 27;

// --- C API sherpa-onnx -----------------------------------------------------

#[repr(C)]
struct PyannoteConfig {
    model: *const c_char,
    window_shift_ratio: f32,
}

#[repr(C)]
struct SegmentationConfig {
    pyannote: PyannoteConfig,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
}

#[repr(C)]
struct EmbeddingConfig {
    model: *const c_char,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
}

#[repr(C)]
struct ClusteringConfig {
    num_clusters: i32,
    threshold: f32,
}

#[repr(C)]
struct DiarizationConfig {
    segmentation: SegmentationConfig,
    embedding: EmbeddingConfig,
    clustering: ClusteringConfig,
    min_duration_on: f32,
    min_duration_off: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CSegment {
    start: f32,
    end: f32,
    speaker: i32,
}

type ProgressCallback = extern "C" fn(i32, i32, *mut c_void) -> i32;

extern "C" {
    fn SherpaOnnxCreateOfflineSpeakerDiarization(config: *const DiarizationConfig) -> *const c_void;
    fn SherpaOnnxDestroyOfflineSpeakerDiarization(sd: *const c_void);
    fn SherpaOnnxOfflineSpeakerDiarizationProcessWithCallback(
        sd: *const c_void,
        samples: *const f32,
        n: i32,
        callback: ProgressCallback,
        arg: *mut c_void,
    ) -> *const c_void;
    fn SherpaOnnxOfflineSpeakerDiarizationResultGetNumSegments(result: *const c_void) -> i32;
    fn SherpaOnnxOfflineSpeakerDiarizationResultSortByStartTime(
        result: *const c_void,
    ) -> *const CSegment;
    fn SherpaOnnxOfflineSpeakerDiarizationDestroySegment(segments: *const CSegment);
    fn SherpaOnnxOfflineSpeakerDiarizationDestroyResult(result: *const c_void);

    fn SherpaOnnxCreateSpeakerEmbeddingExtractor(config: *const EmbeddingConfig) -> *const c_void;
    fn SherpaOnnxDestroySpeakerEmbeddingExtractor(p: *const c_void);
    fn SherpaOnnxSpeakerEmbeddingExtractorDim(p: *const c_void) -> i32;
    fn SherpaOnnxSpeakerEmbeddingExtractorCreateStream(p: *const c_void) -> *const c_void;
    fn SherpaOnnxSpeakerEmbeddingExtractorComputeEmbedding(
        p: *const c_void,
        stream: *const c_void,
    ) -> *const f32;
    fn SherpaOnnxSpeakerEmbeddingExtractorDestroyEmbedding(v: *const f32);
    fn SherpaOnnxOnlineStreamAcceptWaveform(
        stream: *const c_void,
        sample_rate: i32,
        samples: *const f32,
        n: i32,
    );
    fn SherpaOnnxOnlineStreamInputFinished(stream: *const c_void);
    fn SherpaOnnxDestroyOnlineStream(stream: *const c_void);
}

/// Обёртки, чтобы указатели гарантированно освобождались.
struct Diarization(*const c_void);
impl Drop for Diarization {
    fn drop(&mut self) {
        unsafe { SherpaOnnxDestroyOfflineSpeakerDiarization(self.0) }
    }
}

struct Extractor(*const c_void);
impl Drop for Extractor {
    fn drop(&mut self) {
        unsafe { SherpaOnnxDestroySpeakerEmbeddingExtractor(self.0) }
    }
}

// --- модели ----------------------------------------------------------------

fn models_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join("diarization"))
        .unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Сегментация маленькая (6 МБ) и лежит в бинаре — как asset на Android:
/// без неё разделение не начать, а качать две модели вместо одной хуже.
/// ONNX читает только с диска, поэтому файл выкладывается рядом с моделями.
fn ensure_segmentation(app: &tauri::AppHandle) -> Result<PathBuf> {
    const BYTES: &[u8] = include_bytes!("../models/segmentation.onnx");
    let path = models_dir(app).join("segmentation.onnx");
    if path.metadata().map(|m| m.len()).unwrap_or(0) != BYTES.len() as u64 {
        std::fs::write(&path, BYTES)?;
    }
    Ok(path)
}

fn embedding_file(app: &tauri::AppHandle) -> PathBuf {
    models_dir(app).join("embedding.onnx")
}

pub fn models_ready(app: &tauri::AppHandle) -> bool {
    embedding_file(app).metadata().map(|m| m.len()).unwrap_or(0) == EMB_BYTES
}

/// Докачка эмбеддинг-модели тем же curl, что и модели распознавания.
pub fn download(
    app: &tauri::AppHandle,
    on_progress: &dyn Fn(u8),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let target = embedding_file(app);
    let tmp = target.with_extension("part");
    let _ = std::fs::remove_file(&tmp);

    let mut child = std::process::Command::new("/usr/bin/curl")
        .args(["-L", "-f", "-s", "--connect-timeout", "10", "-o"])
        .arg(&tmp)
        .arg(EMB_URL)
        .spawn()?;

    loop {
        if cancelled() {
            let _ = child.kill();
            let _ = std::fs::remove_file(&tmp);
            return Err(anyhow!("отменено"));
        }
        match child.try_wait()? {
            Some(status) => {
                let size = tmp.metadata().map(|m| m.len()).unwrap_or(0);
                if !status.success() || size != EMB_BYTES {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(anyhow!("модель не скачалась"));
                }
                std::fs::rename(&tmp, &target)?;
                return Ok(());
            }
            None => {
                let done = tmp.metadata().map(|m| m.len()).unwrap_or(0);
                on_progress(((done * 100 / EMB_BYTES).min(99)) as u8);
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
    }
}

// --- разбор ----------------------------------------------------------------

/// Отрезок «этот человек говорил тут», в секундах всей записи.
#[derive(Clone, Copy)]
struct Turn {
    start: f32,
    end: f32,
    speaker: usize,
}

struct Progress<'a> {
    window: usize,
    windows: usize,
    on_progress: &'a dyn Fn(u8),
    cancelled: &'a dyn Fn() -> bool,
}

extern "C" fn progress_trampoline(done: i32, total: i32, arg: *mut c_void) -> i32 {
    let p = unsafe { &*(arg as *const Progress) };
    let share = if total > 0 { done as f32 / total as f32 } else { 0.0 };
    let pct = ((p.window as f32 + share) * 100.0 / p.windows as f32).clamp(0.0, 99.0);
    (p.on_progress)(pct as u8);
    if (p.cancelled)() {
        1
    } else {
        0
    }
}

/// Прогоняет диаризацию по WAV и возвращает номер говорящего для каждой
/// реплики таймлайна. [num_speakers] — сколько людей было (0 — определить
/// самой).
pub fn run(
    app: &tauri::AppHandle,
    audio: &Path,
    segments: &[(f32, f32)],
    num_speakers: i32,
    on_progress: &dyn Fn(u8),
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<usize>> {
    if segments.is_empty() {
        return Err(anyhow!("расшифровки нет"));
    }
    let seg_model = ensure_segmentation(app)?;
    let emb_model = embedding_file(app);
    if !models_ready(app) {
        return Err(anyhow!("модель голосов не скачана"));
    }
    let turns = turns_of(
        audio,
        &seg_model,
        &emb_model,
        num_speakers,
        on_progress,
        cancelled,
    )?;
    if cancelled() {
        return Err(anyhow!("отменено"));
    }
    Ok(assign_speakers(segments, &turns))
}

/// Косинусная близость голосов в двух записях — для проверки, различает ли
/// модель конкретные голоса (пример diarize_check, режим compare).
pub fn voice_similarity(a: &Path, b: &Path, emb_model: &Path) -> Result<f32> {
    let emb_path = CString::new(emb_model.to_string_lossy().as_bytes())?;
    let provider = CString::new("cpu")?;
    let config = EmbeddingConfig {
        model: emb_path.as_ptr(),
        num_threads: THREADS,
        debug: 0,
        provider: provider.as_ptr(),
    };
    let p = unsafe { SherpaOnnxCreateSpeakerEmbeddingExtractor(&config) };
    if p.is_null() {
        return Err(anyhow!("экстрактор не создался"));
    }
    let extractor = Extractor(p);

    let embed = |path: &Path| -> Result<Vec<f32>> {
        let mut wav = WavReader::open(path)?;
        let samples = wav.read(0, wav.total_samples as usize)?;
        let whole = [CSegment {
            start: 0.0,
            end: samples.len() as f32 / SAMPLE_RATE as f32,
            speaker: 0,
        }];
        Ok(speaker_embedding(&extractor, &samples, &whole, 0))
    };
    Ok(cosine(&embed(a)?, &embed(b)?))
}

/// Кто когда говорил — по путям моделей, без Tauri: этим же пользуется
/// проверочный пример diarize_check.
pub fn turns_for_example(
    audio: &Path,
    seg_model: &Path,
    emb_model: &Path,
    num_speakers: i32,
) -> Result<Vec<(f32, f32, usize)>> {
    let turns = turns_of(
        audio,
        seg_model,
        emb_model,
        num_speakers,
        &|_| {},
        &|| false,
    )?;
    Ok(turns
        .into_iter()
        .map(|t| (t.start, t.end, t.speaker))
        .collect())
}

fn turns_of(
    audio: &Path,
    seg_model: &Path,
    emb_model: &Path,
    num_speakers: i32,
    on_progress: &dyn Fn(u8),
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Turn>> {
    let seg_path = CString::new(seg_model.to_string_lossy().as_bytes())?;
    let emb_path = CString::new(emb_model.to_string_lossy().as_bytes())?;
    let provider = CString::new("cpu")?;

    let config = DiarizationConfig {
        segmentation: SegmentationConfig {
            pyannote: PyannoteConfig {
                model: seg_path.as_ptr(),
                window_shift_ratio: 0.0,
            },
            num_threads: THREADS,
            debug: 0,
            provider: provider.as_ptr(),
        },
        embedding: EmbeddingConfig {
            model: emb_path.as_ptr(),
            num_threads: THREADS,
            debug: 0,
            provider: provider.as_ptr(),
        },
        // sherpa всегда просим дробить по порогу, даже когда число людей
        // известно: с num_clusters он схлопывает лишнее сам и на двух
        // говорящих отдавал одного. Нужное число получается своей
        // кластеризацией эмбеддингов ниже — она же сшивает окна.
        clustering: ClusteringConfig {
            num_clusters: -1,
            threshold: CLUSTER_THRESHOLD,
        },
        // Те же пороги, что у Kotlin-обёртки на Android: короче 0.2 с —
        // не речь, паузы короче 0.5 с не разрывают реплику.
        min_duration_on: 0.2,
        min_duration_off: 0.5,
    };

    let sd = unsafe { SherpaOnnxCreateOfflineSpeakerDiarization(&config) };
    if sd.is_null() {
        return Err(anyhow!("диаризация не создалась"));
    }
    let sd = Diarization(sd);

    let mut wav = WavReader::open(audio)?;
    diarize_windowed(
        &sd,
        &emb_path,
        &provider,
        &mut wav,
        num_speakers,
        on_progress,
        cancelled,
    )
}

/// Окна по десять минут; одно окно — если запись короче двенадцати.
fn diarize_windowed(
    sd: &Diarization,
    emb_path: &CString,
    provider: &CString,
    wav: &mut WavReader,
    num_speakers: i32,
    on_progress: &dyn Fn(u8),
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Turn>> {
    let sr = SAMPLE_RATE as u64;
    let window = WINDOW_SEC as u64 * sr;
    let total = wav.total_samples;
    let single_window = total <= window * 12 / 10;
    let windows = if single_window {
        1
    } else {
        ((total + window - 1) / window) as usize
    };

    // Экстрактор нужен всегда: он и сшивает окна, и сводит лишние голоса
    // внутри одного окна — sherpa намеренно дробит по порогу, а сколько
    // людей на самом деле, решает кластеризация ниже.
    let extractor = {
        let config = EmbeddingConfig {
            model: emb_path.as_ptr(),
            num_threads: THREADS,
            debug: 0,
            provider: provider.as_ptr(),
        };
        let p = unsafe { SherpaOnnxCreateSpeakerEmbeddingExtractor(&config) };
        if p.is_null() {
            return Err(anyhow!("экстрактор голосов не создался"));
        }
        Extractor(p)
    };

    // Сшивка идёт после всех окон: сначала собираем по эмбеддингу на каждого
    // локального говорящего каждого окна, потом кластеризуем их разом.
    let mut local_turns: Vec<(usize, usize, Turn)> = Vec::new();
    let mut embeddings: Vec<((usize, usize), Vec<f32>)> = Vec::new();
    let mut speech_sec: std::collections::HashMap<(usize, usize), f32> = Default::default();

    for w in 0..windows {
        let offset = w as u64 * window;
        let count = if single_window {
            total
        } else {
            window.min(total - offset)
        } as usize;
        let samples = wav.read(offset, count)?;

        let mut progress = Progress {
            window: w,
            windows,
            on_progress,
            cancelled,
        };
        let result = unsafe {
            SherpaOnnxOfflineSpeakerDiarizationProcessWithCallback(
                sd.0,
                samples.as_ptr(),
                samples.len() as i32,
                progress_trampoline,
                &mut progress as *mut Progress as *mut c_void,
            )
        };
        if result.is_null() {
            return Err(anyhow!("диаризация не отработала"));
        }
        let local = collect_segments(result);
        unsafe { SherpaOnnxOfflineSpeakerDiarizationDestroyResult(result) };
        if cancelled() {
            return Ok(Vec::new());
        }

        for s in &local {
            local_turns.push((
                w,
                s.speaker as usize,
                Turn {
                    start: offset as f32 / sr as f32 + s.start,
                    end: offset as f32 / sr as f32 + s.end,
                    speaker: 0,
                },
            ));
            *speech_sec.entry((w, s.speaker as usize)).or_insert(0.0) += s.end - s.start;
        }

        let mut speakers: Vec<i32> = local.iter().map(|s| s.speaker).collect();
        speakers.sort_unstable();
        speakers.dedup();
        for speaker in speakers {
            embeddings.push((
                (w, speaker as usize),
                speaker_embedding(&extractor, &samples, &local, speaker),
            ));
        }
    }

    on_progress(99);
    if embeddings.is_empty() {
        return Ok(Vec::new());
    }

    let points: Vec<&[f32]> = embeddings.iter().map(|(_, v)| v.as_slice()).collect();
    let sizes: Vec<f32> = embeddings
        .iter()
        .map(|(key, _)| speech_sec.get(key).copied().unwrap_or(0.0))
        .collect();
    let labels = cluster(&points, &sizes, num_speakers);
    let global: std::collections::HashMap<(usize, usize), usize> = embeddings
        .iter()
        .enumerate()
        .map(|(i, (key, _))| (*key, labels[i]))
        .collect();

    Ok(local_turns
        .into_iter()
        .map(|(w, local, turn)| Turn {
            speaker: global.get(&(w, local)).copied().unwrap_or(0),
            ..turn
        })
        .collect())
}

fn collect_segments(result: *const c_void) -> Vec<CSegment> {
    unsafe {
        let count = SherpaOnnxOfflineSpeakerDiarizationResultGetNumSegments(result);
        if count <= 0 {
            return Vec::new();
        }
        let ptr = SherpaOnnxOfflineSpeakerDiarizationResultSortByStartTime(result);
        if ptr.is_null() {
            return Vec::new();
        }
        let out = std::slice::from_raw_parts(ptr, count as usize).to_vec();
        SherpaOnnxOfflineSpeakerDiarizationDestroySegment(ptr);
        out
    }
}

/// Эмбеддинг говорящего по его самым длинным репликам, до десяти секунд.
fn speaker_embedding(
    extractor: &Extractor,
    samples: &[f32],
    local: &[CSegment],
    speaker: i32,
) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let mut speech: Vec<&CSegment> = local.iter().filter(|s| s.speaker == speaker).collect();
    speech.sort_by(|a, b| (b.end - b.start).total_cmp(&(a.end - a.start)));
    let mut need = (EMBED_SPEECH_SEC * sr) as usize;

    unsafe {
        let stream = SherpaOnnxSpeakerEmbeddingExtractorCreateStream(extractor.0);
        for s in speech {
            if need == 0 {
                break;
            }
            let from = ((s.start * sr) as usize).min(samples.len());
            let to = ((s.end * sr) as usize).clamp(from, samples.len());
            let take = (to - from).min(need);
            if take > 0 {
                SherpaOnnxOnlineStreamAcceptWaveform(
                    stream,
                    SAMPLE_RATE as i32,
                    samples[from..from + take].as_ptr(),
                    take as i32,
                );
                need -= take;
            }
        }
        SherpaOnnxOnlineStreamInputFinished(stream);
        let dim = SherpaOnnxSpeakerEmbeddingExtractorDim(extractor.0) as usize;
        let raw = SherpaOnnxSpeakerEmbeddingExtractorComputeEmbedding(extractor.0, stream);
        let out = if raw.is_null() {
            vec![0.0; dim]
        } else {
            let v = std::slice::from_raw_parts(raw, dim).to_vec();
            SherpaOnnxSpeakerEmbeddingExtractorDestroyEmbedding(raw);
            v
        };
        SherpaOnnxDestroyOnlineStream(stream);
        out
    }
}

// --- сшивка окон -----------------------------------------------------------

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let d = na.sqrt() * nb.sqrt();
    if d > 0.0 {
        dot / d
    } else {
        0.0
    }
}

/// Агломеративная кластеризация (average linkage, косинусная дистанция).
/// Точек мало — пар «окно, говорящий» даже у двухчасовой записи десятки.
fn agglomerate(points: &[&[f32]], target: usize, merges: Option<&mut Vec<f32>>) -> Vec<Vec<usize>> {
    let mut clusters: Vec<Vec<usize>> = (0..points.len()).map(|i| vec![i]).collect();
    let mut trace = merges;

    let linkage = |a: &[usize], b: &[usize]| -> f32 {
        let mut sum = 0.0;
        for &i in a {
            for &j in b {
                sum += 1.0 - cosine(points[i], points[j]);
            }
        }
        sum / (a.len() * b.len()) as f32
    };

    while clusters.len() > target.max(1) {
        let mut best = (0usize, 0usize, f32::MAX);
        for i in 0..clusters.len() {
            for j in i + 1..clusters.len() {
                let d = linkage(&clusters[i], &clusters[j]);
                if d < best.2 {
                    best = (i, j, d);
                }
            }
        }
        if best.2 == f32::MAX {
            break;
        }
        if let Some(trace) = trace.as_deref_mut() {
            trace.push(best.2);
        }
        let merged = clusters.remove(best.1);
        clusters[best.0].extend(merged);
    }
    clusters
}

/// Сколько людей в записи — по следу слияний: пока склеиваются куски одного
/// голоса, слияния дешёвые, а первое склеивание двух разных людей заметно
/// дороже предыдущего.
fn auto_target(points: &[&[f32]]) -> usize {
    if points.len() <= 1 {
        return 1;
    }
    let mut merges = Vec::new();
    agglomerate(points, 1, Some(&mut merges));
    log::info!(
        "сшивка: след слияний {}",
        merges
            .iter()
            .map(|m| format!("{m:.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Все слияния дешёвые — один голос; все дорогие — все голоса разные.
    if merges.last().copied().unwrap_or(0.0) < SINGLE_VOICE_DISTANCE {
        return 1;
    }
    if merges[0] >= SINGLE_VOICE_DISTANCE {
        return points.len();
    }

    let mut cut = 0;
    let mut best_gap = 0.0;
    for i in 0..merges.len().saturating_sub(1) {
        let gap = merges[i + 1] - merges[i];
        if gap >= best_gap {
            best_gap = gap;
            cut = i;
        }
    }
    points.len() - (cut + 1)
}

/// Кластеризуются только крупные говорящие: осколки с парой секунд речи
/// шумные и сцепляют чужие кластеры друг с другом. Осколки прикрепляются к
/// готовым кластерам в конце.
fn cluster(points: &[&[f32]], speech_sec: &[f32], num_speakers: i32) -> Vec<usize> {
    for i in 0..points.len() {
        let row: Vec<String> = (0..points.len())
            .map(|j| format!("{:.2}", 1.0 - cosine(points[i], points[j])))
            .collect();
        log::info!("сшивка: дистанции[{i}] ({:.0} с): {}", speech_sec[i], row.join(" "));
    }

    // «Крупный» — тот, у кого заметная доля всей речи, но не больше
    // полуминуты порога: абсолютные 30 секунд с Android отсекали всех в
    // коротких записях, а доля сама подстраивается под длину встречи.
    let total_speech: f32 = speech_sec.iter().sum();
    let big_threshold = BIG_SPEAKER_SEC.min(total_speech * 0.05);

    let big: Vec<usize> = {
        let filtered: Vec<usize> = (0..points.len())
            .filter(|&i| speech_sec[i] >= big_threshold)
            .collect();
        if filtered.is_empty() {
            (0..points.len()).collect()
        } else {
            filtered
        }
    };
    let big_points: Vec<&[f32]> = big.iter().map(|&i| points[i]).collect();

    let target = if num_speakers > 0 {
        (num_speakers as usize).min(big.len())
    } else {
        auto_target(&big_points)
    };
    log::info!(
        "сшивка: целевое число говорящих {target}, крупных {} из {}",
        big.len(),
        points.len()
    );

    let clusters = agglomerate(&big_points, target, None);
    let mut labels = vec![usize::MAX; points.len()];
    for (index, members) in clusters.iter().enumerate() {
        for &p in members {
            labels[big[p]] = index;
        }
    }
    for i in 0..points.len() {
        if labels[i] != usize::MAX {
            continue;
        }
        labels[i] = clusters
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let mean = |c: &Vec<usize>| {
                    c.iter()
                        .map(|&p| 1.0 - cosine(points[i], big_points[p]))
                        .sum::<f32>()
                        / c.len().max(1) as f32
                };
                mean(a).total_cmp(&mean(b))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
    }
    labels
}

/// Каждой реплике таймлайна — говорящий с наибольшим пересечением по
/// времени. Реплика без пересечений наследует говорящего предыдущей.
/// Номера идут по порядку появления: «говорящий 1» — тот, кто заговорил
/// первым, а не кого кластеризация посчитала первым.
fn assign_speakers(segments: &[(f32, f32)], turns: &[Turn]) -> Vec<usize> {
    let mut previous = 0usize;
    let raw: Vec<usize> = segments
        .iter()
        .map(|&(start, end)| {
            let mut overlap: std::collections::HashMap<usize, f32> = Default::default();
            for t in turns {
                let span = end.min(t.end) - start.max(t.start);
                if span > 0.0 {
                    *overlap.entry(t.speaker).or_insert(0.0) += span;
                }
            }
            let speaker = overlap
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(speaker, _)| speaker)
                .unwrap_or(previous);
            previous = speaker;
            speaker
        })
        .collect();

    let mut order: Vec<usize> = Vec::new();
    for &s in &raw {
        if !order.contains(&s) {
            order.push(s);
        }
    }
    raw.iter()
        .map(|s| order.iter().position(|o| o == s).unwrap_or(0))
        .collect()
}
