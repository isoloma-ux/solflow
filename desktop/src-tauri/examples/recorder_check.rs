//! Проверка заранее открытого потока записи без Tauri: два цикла
//! старт/стоп подряд, между ними пауза — в это время снимается экран,
//! чтобы увидеть, горит ли индикатор микрофона у потока на паузе.
//!
//! RUST_LOG=info cargo run --release --example recorder_check -- [снимок.png]

use std::time::{Duration, Instant};

use solflow_lib::audio::{OutputKeeper, Recorder};

fn shot(dir: &Option<String>, name: &str) {
    if let Some(dir) = dir {
        let path = format!("{dir}/{name}.png");
        let _ = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-x", &path])
            .status();
        println!("снимок: {path}");
    }
}

fn main() {
    env_logger::init();
    let shot_dir = std::env::args().nth(1);
    shot(&shot_dir, "0-before");
    // Тихий поток, который держит выход наготове: должен открыться без
    // ошибок и не мешать записи.
    let keeper = OutputKeeper::spawn();
    keeper.set(true);
    std::thread::sleep(Duration::from_millis(700));
    let recorder = Recorder::spawn();

    for round in 1..=3 {
        let t = Instant::now();
        recorder.start(None).expect("старт");
        println!("круг {round}: старт занял {} мс", t.elapsed().as_millis());
        std::thread::sleep(Duration::from_millis(1500));
        let t = Instant::now();
        let pcm = recorder.stop().expect("стоп");
        println!(
            "круг {round}: стоп занял {} мс, записано {:.2} с",
            t.elapsed().as_millis(),
            pcm.len() as f32 / 16_000.0
        );
        if round == 1 {
            // Поток на паузе: индикатор микрофона гореть не должен.
            std::thread::sleep(Duration::from_millis(1500));
            shot(&shot_dir, "1-paused-1500");
            std::thread::sleep(Duration::from_millis(3500));
            shot(&shot_dir, "2-paused-5000");
        }
    }
    // Смена микрофона в настройках: поток должен переоткрыться заранее.
    recorder.prepare(Some("нет такого микрофона".into()));
    std::thread::sleep(Duration::from_millis(500));
    let t = Instant::now();
    recorder.start(Some("нет такого микрофона".into())).expect("старт с несуществующим именем");
    println!("старт после prepare под чужое имя: {} мс", t.elapsed().as_millis());
    let pcm = recorder.stop().expect("стоп");
    println!("записано {:.2} с", pcm.len() as f32 / 16_000.0);
    drop(recorder);
    keeper.set(false);
    std::thread::sleep(Duration::from_millis(1500));
    shot(&shot_dir, "3-dropped-1500");
    std::thread::sleep(Duration::from_millis(3500));
    shot(&shot_dir, "4-dropped-5000");
}
