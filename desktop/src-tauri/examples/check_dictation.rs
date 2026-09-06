//! Offline, real-model punctuation check; never opens or modifies app data.
#[allow(dead_code)]
#[path = "../src/cleanup.rs"]
mod cleanup;
#[allow(dead_code)]
#[path = "../src/dictation_text.rs"]
mod dictation_text;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(args.len() == 3, "usage: check_dictation MODEL INPUT_TEXT");
    let input = cleanup::clean(&std::fs::read_to_string(&args[2])?);
    let started = std::time::Instant::now();
    let text = solflow_lib::summary::punctuate_path(std::path::Path::new(&args[1]), input.trim())?;
    eprintln!("punctuation_ms={}", started.elapsed().as_millis());
    println!("{text}");
    Ok(())
}
