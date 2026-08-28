//! Проверка WavReader на реальных файлах: свой заголовок и вывод afconvert
//! с FLLR-заполнителем. Запуск: cargo run --release --example wav_check -- a.wav

fn main() {
    let path = std::env::args().nth(1).expect("нужен путь к wav");
    let mut reader = solflow_lib::wav::WavReader::open(std::path::Path::new(&path)).unwrap();
    println!("total_samples: {}", reader.total_samples);
    println!("seconds: {:.2}", reader.total_samples as f32 / 16000.0);

    // РМС по секундам: у теста чередуются тон и тишина.
    let mut second = 0u64;
    while second * 16000 < reader.total_samples {
        let pcm = reader.read(second * 16000, 16000).unwrap();
        let rms = (pcm.iter().map(|v| (v * v) as f64).sum::<f64>() / pcm.len().max(1) as f64)
            .sqrt();
        println!("second {second}: rms {rms:.4} ({} samples)", pcm.len());
        second += 1;
    }
}
