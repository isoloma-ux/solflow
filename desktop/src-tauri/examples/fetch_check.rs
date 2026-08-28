//! Проверка загрузки по ссылке без Tauri: качает звук во временный
//! каталог и печатает, что получилось.
//!
//! cargo run --release --example fetch_check -- <ссылка> [куда]

fn main() {
    env_logger::init();
    let url = std::env::args().nth(1).expect("нужна ссылка");
    let dir = std::env::args().nth(2).unwrap_or("/tmp/solflow-fetch".into());
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();


    let started = std::time::Instant::now();
    match solflow_lib::fetch_url(&url, &dir) {
        Ok((file, title)) => {
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
            println!("название: {title}");
            println!("файл: {} ({:.1} МБ)", file.display(), size as f64 / 1e6);
            println!("заняло {:.1} с", started.elapsed().as_secs_f32());
        }
        Err(e) => println!("не вышло: {e}"),
    }
}
