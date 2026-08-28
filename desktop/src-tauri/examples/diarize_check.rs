//! Проверка разделения говорящих без Tauri: гоняет sherpa-onnx по WAV и
//! печатает, кто когда говорил.
//!
//! cargo run --release --example diarize_check -- dialog.wav seg.onnx emb.onnx [сколько]

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    // Режим сравнения: насколько похожи голоса в двух записях.
    let first = args.next().expect("нужен путь к wav или compare");
    if first == "compare" {
        let a = args.next().expect("нужен первый wav");
        let b = args.next().expect("нужен второй wav");
        let emb = args.next().expect("нужен embedding.onnx");
        let similarity = solflow_lib::voice_similarity(
            std::path::Path::new(&a),
            std::path::Path::new(&b),
            std::path::Path::new(&emb),
        )
        .expect("не удалось посчитать");
        println!("похожесть {similarity:.3}, дистанция {:.3}", 1.0 - similarity);
        return;
    }
    let audio = first;
    let seg = args.next().expect("нужен segmentation.onnx");
    let emb = args.next().expect("нужен embedding.onnx");
    let speakers: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let started = std::time::Instant::now();
    let turns = solflow_lib::diarize_file(
        std::path::Path::new(&audio),
        std::path::Path::new(&seg),
        std::path::Path::new(&emb),
        speakers,
    )
    .expect("диаризация не удалась");

    println!("отрезков: {}", turns.len());
    let mut voices: Vec<usize> = turns.iter().map(|t| t.2).collect();
    voices.sort_unstable();
    voices.dedup();
    println!("голосов: {}", voices.len());
    for (start, end, speaker) in &turns {
        println!("  {start:6.1} — {end:6.1} с: говорящий {}", speaker + 1);
    }
    println!("заняло {:.1} с", started.elapsed().as_secs_f32());
}
