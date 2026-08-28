//! Нарезка длинной записи по паузам — порт Segmenter из Android-версии.
//! GigaAM обучен на репликах до ~25 секунд, дальше точность падает, поэтому
//! длинную диктовку режем по тишине, а не по таймеру.

const FRAME_MS: usize = 20;
const MIN_PAUSE_MS: usize = 350;
const MIN_SEGMENT_SEC: f32 = 4.0;
pub const MAX_SEGMENT_SEC: f32 = 24.0;
const NOISE_FACTOR: f32 = 3.0;
const ABSOLUTE_FLOOR: f32 = 0.004;

/// Отсчётов в одном 20-мс кадре.
pub fn frame_samples(sample_rate: usize) -> usize {
    FRAME_MS * sample_rate / 1000
}

/// Громкость одного кадра — для потокового первого прохода по файлу.
pub fn frame_energy(pcm: &[f32], offset: usize, frame: usize) -> f32 {
    let sum: f64 = pcm[offset..offset + frame]
        .iter()
        .map(|&v| (v as f64) * (v as f64))
        .sum();
    (sum / frame as f64).sqrt() as f32
}

/// Кадры, по которым надо резать. Вход — громкость каждого 20-мс кадра
/// записи целиком: порог тишины считается от общего шумового фона. Логика
/// работает на энергиях, а не на самом звуке, — так ей пользуется и
/// расшифровка встреч, где двухчасовой файл в память не влезает.
pub fn cut_frames(loud: &[f32]) -> Vec<usize> {
    let threshold = threshold(loud);
    let is_speech: Vec<bool> = loud.iter().map(|&l| l > threshold).collect();

    let min_pause_frames = MIN_PAUSE_MS / FRAME_MS;
    let min_segment_frames = (MIN_SEGMENT_SEC * 1000.0 / FRAME_MS as f32) as usize;
    let max_segment_frames = (MAX_SEGMENT_SEC * 1000.0 / FRAME_MS as f32) as usize;

    let mut cuts = Vec::new();
    let mut segment_start = 0usize;
    let mut silence_run = 0usize;

    for index in 0..is_speech.len() {
        silence_run = if is_speech[index] { 0 } else { silence_run + 1 };
        let length = index - segment_start;

        if silence_run >= min_pause_frames && length >= min_segment_frames {
            // Режем в середине паузы: ни одна фраза не теряет края.
            let cut = index - silence_run / 2;
            cuts.push(cut);
            segment_start = cut;
            silence_run = 0;
        } else if length >= max_segment_frames {
            // Пауз не нашлось — режем по самой тихой точке в хвосте куска.
            let from = segment_start + min_segment_frames;
            let quietest = (from..index)
                .min_by(|&a, &b| loud[a].total_cmp(&loud[b]))
                .unwrap_or(index);
            cuts.push(quietest);
            segment_start = quietest;
            silence_run = 0;
        }
    }
    cuts
}

pub fn split(pcm: &[f32], sample_rate: usize) -> Vec<Vec<f32>> {
    let max_samples = (MAX_SEGMENT_SEC * sample_rate as f32) as usize;
    if pcm.len() <= max_samples {
        return vec![pcm.to_vec()];
    }

    let frame = frame_samples(sample_rate);
    let loud: Vec<f32> = (0..pcm.len() / frame)
        .map(|f| frame_energy(pcm, f * frame, frame))
        .collect();
    let cuts = cut_frames(&loud);

    let mut bounds = vec![0usize];
    bounds.extend(cuts);
    bounds.push(loud.len());

    bounds
        .windows(2)
        .map(|w| {
            let from = w[0] * frame;
            let to = (w[1] * frame).min(pcm.len());
            pcm[from..to].to_vec()
        })
        .filter(|s| s.len() > frame * 5)
        .collect()
}

/// Шумовой фон — 10-й процентиль громкости кадров.
fn threshold(loud: &[f32]) -> f32 {
    let mut sorted = loud.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let idx = ((sorted.len() as f32 * 0.1) as usize).min(sorted.len().saturating_sub(1));
    (sorted[idx] * NOISE_FACTOR).max(ABSOLUTE_FLOOR)
}
