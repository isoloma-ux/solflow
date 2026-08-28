//! Проверка цепочки записи встречи без Tauri: микрофон → ресемплер → WAV.
//! Пишет заданное число секунд и печатает, что получилось.
//!
//! cargo run --release --example record_check -- 3 out.wav

use std::sync::mpsc::channel;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use solflow_lib::wav::{WavReader, WavWriter, SAMPLE_RATE};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let path = std::env::args().nth(2).unwrap_or("record_check.wav".into());
    let path = std::path::PathBuf::from(path);

    let host = cpal::default_host();
    let device = host.default_input_device().expect("микрофон не найден");
    let config = device.default_input_config().expect("микрофон не открылся");
    let src_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;
    println!("устройство: {}", device.name().unwrap_or_default());
    println!("частота {src_rate} Гц, каналов {channels}");

    let (tx, rx) = channel::<Vec<f32>>();
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mono: Vec<f32> = data
                    .chunks_exact(channels.max(1))
                    .map(|f| f.iter().sum::<f32>() / channels.max(1) as f32)
                    .collect();
                let _ = tx.send(mono);
            },
            |e| eprintln!("ошибка потока: {e}"),
            None,
        )
        .expect("поток не создался");
    stream.play().expect("поток не стартовал");

    let mut wav = WavWriter::create(&path).unwrap();
    let mut down = solflow_lib::downsampler_for_test(src_rate, SAMPLE_RATE);
    let mut out = Vec::new();
    let mut peak = 0.0f32;

    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < until {
        if let Ok(chunk) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            for &v in &chunk {
                peak = peak.max(v.abs());
            }
            out.clear();
            down.feed(&chunk, &mut out);
            wav.write(&out).unwrap();
        }
    }
    drop(stream);
    let written = wav.finish().unwrap();

    println!("записано отсчётов: {written}");
    println!("это {:.2} с на 16 кГц (просили {seconds})", written as f32 / SAMPLE_RATE as f32);
    println!("пик громкости: {peak:.4}");

    let reader = WavReader::open(&path).unwrap();
    println!("перечитано: {} отсчётов", reader.total_samples);
    assert_eq!(reader.total_samples, written, "файл читается не так, как писался");
}
