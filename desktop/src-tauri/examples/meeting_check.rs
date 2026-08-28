//! Проверка расшифровки встречи без Tauri: те же два прохода, что в
//! meetings::transcribe_job — энергии кадров потоком, нарезка по паузам,
//! чтение кусков с диска. Заодно гоняет потоковый ресемплер против
//! разового, чтобы стык кусков не терял отсчёты.
//!
//! cargo run --release --example meeting_check -- meeting.wav [model.gguf]

use solflow_lib::wav::{WavReader, SAMPLE_RATE};
use solflow_lib::{cleanup_clean, frame_energy, frame_samples, cut_frames, MAX_SEGMENT_SEC};

fn main() {
    let path = std::env::args().nth(1).expect("нужен путь к wav");
    let model = std::env::args().nth(2);

    let mut wav = WavReader::open(std::path::Path::new(&path)).unwrap();
    let total = wav.total_samples;
    let frame = frame_samples(SAMPLE_RATE);
    println!("всего {:.1} с", total as f32 / SAMPLE_RATE as f32);

    // Первый проход.
    let frames = (total / frame as u64) as usize;
    let mut loud = Vec::with_capacity(frames);
    let block = frame * 500;
    let mut offset = 0u64;
    while loud.len() < frames {
        let pcm = wav.read(offset, block).unwrap();
        if pcm.is_empty() {
            break;
        }
        let mut o = 0usize;
        while o + frame <= pcm.len() && loud.len() < frames {
            loud.push(frame_energy(&pcm, o, frame));
            o += frame;
        }
        offset += o as u64;
    }
    println!("кадров: {} (ожидалось {frames})", loud.len());

    let max_single = (MAX_SEGMENT_SEC * SAMPLE_RATE as f32) as u64;
    let cuts = if total <= max_single {
        Vec::new()
    } else {
        cut_frames(&loud)
    };
    let mut bounds = vec![0u64];
    bounds.extend(cuts.iter().map(|&c| c as u64 * frame as u64));
    bounds.push(frames as u64 * frame as u64);
    let ranges: Vec<(u64, u64)> = bounds
        .windows(2)
        .map(|w| (w[0], w[1].min(total)))
        .filter(|(a, b)| b.saturating_sub(*a) > (frame * 5) as u64)
        .collect();

    println!("кусков: {}", ranges.len());
    for (from, to) in &ranges {
        println!(
            "  {:.1} — {:.1} с",
            *from as f32 / SAMPLE_RATE as f32,
            *to as f32 / SAMPLE_RATE as f32
        );
    }
    let covered: u64 = ranges.iter().map(|(a, b)| b - a).sum();
    println!(
        "покрыто {:.1}% звука",
        covered as f32 * 100.0 / total as f32
    );

    if let Some(model) = model {
        let engine = solflow_lib::engine_for_test(&std::path::PathBuf::from(model));
        for (from, to) in &ranges {
            let pcm = wav.read(*from, (*to - *from) as usize).unwrap();
            let text = cleanup_clean(&engine.transcribe_segment(&pcm).unwrap());
            if !text.is_empty() {
                println!("[{:.0} с] {text}", *from as f32 / SAMPLE_RATE as f32);
            }
        }
    }
}
