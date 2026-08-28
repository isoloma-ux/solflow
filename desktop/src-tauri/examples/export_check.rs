//! Проверка форматов экспорта без Tauri: собирает те же тексты, что и
//! meetings::export, и прогоняет их через системные конвертеры.
//!
//! cargo run --release --example export_check -- /куда/сложить

use std::process::Command;

use solflow_lib::export_bodies;

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or(".".into());
    let out_dir = std::path::PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let segments = vec![
        segment(0.0, 21.4, "Начнем со статуса по анимациям, вчера доделали волну.", Some(0)),
        segment(21.4, 44.0, "Дальше по плану нижние листы и каскад списков.", Some(1)),
        segment(3800.0, 3820.0, "Договорились, что пороги оставляем как в токенах.", Some(0)),
    ];
    let (text, markdown, html) = export_bodies("Планерка по 2.1", "1 ч 3 мин", &segments);

    println!("--- txt ---\n{text}");
    println!("--- md ---\n{markdown}");

    // Смена говорящего должна давать подпись, повтор — нет.
    assert!(text.contains("Говорящий 1"), "нет подписи первого говорящего");
    assert!(text.contains("Говорящий 2"), "нет подписи второго говорящего");
    assert_eq!(text.matches("Говорящий 1").count(), 2, "подписи ставятся не на смене голоса");
    assert!(text.contains("1:03:20"), "часовая метка времени неверна");

    let source = out_dir.join("export.html");
    std::fs::write(&source, &html).unwrap();

    // docx собирает свой генератор, а читает его системный textutil: если
    // тот разобрал файл и вернул текст реплик — формат корректный.
    let docx = out_dir.join("Планерка.docx");
    std::fs::write(&docx, solflow_lib::export_docx("Планерка по 2.1", "1 ч 3 мин", &segments)).unwrap();
    let back = out_dir.join("docx-back.txt");
    let ok = Command::new("/usr/bin/textutil")
        .args(["-convert", "txt", "-output"])
        .arg(&back)
        .arg(&docx)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let read_back = std::fs::read_to_string(&back).unwrap_or_default();
    println!("docx: {ok}, {} байт, прочитано обратно:\n{read_back}",
             docx.metadata().map(|m| m.len()).unwrap_or(0));
    assert!(ok, "textutil не смог прочитать собранный docx");
    assert!(read_back.contains("Планерка по 2.1"), "в docx нет заголовка");
    assert!(read_back.contains("волну"), "в docx нет текста реплики");
    assert!(read_back.contains("1:03:20"), "в docx нет метки времени");

    // Длинная встреча — чтобы проверить перенос строк и вторую страницу.
    let mut many = segments.clone();
    for i in 0..40 {
        many.push(segment(
            600.0 + i as f32 * 30.0,
            630.0 + i as f32 * 30.0,
            "Проверяем перенос длинной реплики: она должна разложиться по \
             строкам ровно по ширине колонки и не залезть на поля страницы, \
             а при нехватке места уехать на следующую страницу целиком.",
            Some(i % 2),
        ));
    }
    let bytes = solflow_lib::export_pdf("Планерка по 2.1", "1 ч 3 мин", &many);
    let pdf = out_dir.join("Планерка.pdf");
    std::fs::write(&pdf, &bytes).unwrap();
    println!("pdf: {} байт", bytes.len());
    assert!(bytes.starts_with(b"%PDF"));

    println!("готово: {}", out_dir.display());
}

fn segment(s: f32, e: f32, text: &str, spk: Option<u32>) -> solflow_lib::MeetingSegment {
    solflow_lib::MeetingSegment {
        s,
        e,
        text: text.to_string(),
        spk,
    }
}
