//! Разборы записи без Tauri: решения и задачи, письмо, оглавление.
//!
//! RUST_LOG=solflow_lib=debug cargo run --release --example derive_check -- <папка встречи> tasks|letter|outline

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use solflow_lib::meetings::{timed_text, Meta, Segment};

fn main() {
    env_logger::init();
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("папка встречи"));
    let kind = std::env::args().nth(2).unwrap_or("outline".into());
    let home = std::env::var("HOME").unwrap();
    let model = std::path::PathBuf::from(home)
        .join("Library/Application Support/ru.ivansolomin.solflow/models")
        .join(solflow_lib::summary::MODEL_FILE);

    let meta: Meta =
        serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap()).unwrap();
    let segments: Vec<Segment> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("transcript.json")).unwrap())
            .unwrap();
    let text = if kind == "letter" {
        segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ")
    } else {
        timed_text(&meta, &segments)
    };
    println!("встреча «{}», разбор {kind}, {} символов", meta.title, text.chars().count());

    let started = std::time::Instant::now();
    let out = solflow_lib::summary::derive_with(
        &model,
        &kind,
        &text,
        |pct| eprint!("\r{pct}%   "),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("разбор");
    eprintln!();
    println!("--- {kind} за {:.1} с ---\n{out}", started.elapsed().as_secs_f32());
}
