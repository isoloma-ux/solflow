//! Вопрос к записи без Tauri: модель из папки приложения, расшифровка из
//! папки встречи, вопрос из аргументов.
//!
//! RUST_LOG=info cargo run --release --example ask_check -- <папка встречи> "вопрос"

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use solflow_lib::meetings::{timed_text, Meta, Segment};

fn main() {
    env_logger::init();
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("папка встречи"));
    let question = std::env::args().nth(2).expect("вопрос");
    let home = std::env::var("HOME").unwrap();
    let model = std::path::PathBuf::from(home)
        .join("Library/Application Support/ru.ivansolomin.solflow/models")
        .join(solflow_lib::summary::MODEL_FILE);

    let meta: Meta =
        serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap()).unwrap();
    let segments: Vec<Segment> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("transcript.json")).unwrap())
            .unwrap();
    let timed = timed_text(&meta, &segments);
    println!(
        "встреча «{}»: {} реплик, {} строк со временем, {} символов",
        meta.title,
        segments.len(),
        timed.lines().count(),
        timed.chars().count()
    );
    println!("--- первые строки ---\n{}\n---", timed.lines().take(3).collect::<Vec<_>>().join("\n"));

    let started = std::time::Instant::now();
    let answer = solflow_lib::summary::ask_with(
        &model,
        &timed,
        &question,
        |pct| eprint!("\r{pct}%   "),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("ответ");
    eprintln!();
    println!("--- ответ за {:.1} с ---\n{answer}", started.elapsed().as_secs_f32());
}
