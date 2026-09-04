//! Перевод без Tauri: первые N реплик встречи на язык.
//!
//! RUST_LOG=solflow_lib=debug cargo run --release --example translate_check -- <папка встречи> en [N]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use solflow_lib::meetings::Segment;

fn main() {
    env_logger::init();
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("папка встречи"));
    let lang = std::env::args().nth(2).unwrap_or("en".into());
    let n: usize = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(30);
    let home = std::env::var("HOME").unwrap();
    let model = std::path::PathBuf::from(home)
        .join("Library/Application Support/ru.ivansolomin.solflow/models")
        .join(solflow_lib::summary::MODEL_FILE);
    let segments: Vec<Segment> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("transcript.json")).unwrap())
            .unwrap();
    let lines: Vec<String> = segments.iter().take(n).map(|s| s.text.clone()).collect();
    let started = std::time::Instant::now();
    let out = solflow_lib::summary::translate_lines_with(
        &model,
        &lang,
        &lines,
        |pct| eprint!("\r{pct}%   "),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("перевод");
    eprintln!();
    println!("--- {} строк за {:.1} с ---", out.len(), started.elapsed().as_secs_f32());
    for (src, dst) in lines.iter().zip(out.iter()).take(12) {
        println!("• {}\n  → {}", src.chars().take(90).collect::<String>(), dst.chars().take(110).collect::<String>());
    }
}
