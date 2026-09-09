//! Проверка движка без микрофона: WAV (16 кГц моно PCM16) → текст.
//! cargo run --example transcribe_file -- путь/к/model.gguf путь/к/файлу.wav

use std::time::Instant;
#[path = "../src/wav.rs"]
mod wav;

fn main() {
    let mut args = std::env::args().skip(1);
    let model = args.next().expect("нужен путь к модели");
    let wav = args.next().expect("нужен путь к wav");

    let mut input = wav::WavReader::open(std::path::Path::new(&wav)).expect("wav не читается");
    let pcm = input.read(0, input.total_samples as usize).expect("данные wav не читаются");
    let seconds = pcm.len() as f32 / 16000.0;

    let loaded = Instant::now();
    let model = transcribe_cpp::Model::load(&model).expect("модель не загрузилась");
    let mut session = model.session().expect("сессия не создалась");
    println!("модель загружена за {:.1} с", loaded.elapsed().as_secs_f32());

    let started = Instant::now();
    let mut parts = Vec::new();
    for segment in solflow_lib::segmenter_split(&pcm, 16000) {
        let text = session
            .run(&segment, &transcribe_cpp::RunOptions::default())
            .expect("распознавание упало")
            .text;
        parts.push(text.trim().to_string());
    }
    let text = solflow_lib::cleanup_clean(&parts.join(" "));
    let took = started.elapsed().as_secs_f32();

    println!("--- {:.1} с звука за {:.1} с ({:.1}x realtime)", seconds, took, seconds / took);
    println!("{text}");
}
