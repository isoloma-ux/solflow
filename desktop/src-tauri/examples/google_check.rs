//! Проверка Google Drive без окна: код входа → токен → папки → файл туда
//! и обратно → удаление. Код печатается в первой строке вывода.
//!
//! cargo run --release --example google_check

use std::time::Duration;

use solflow_lib::sync::provider::{Folder, Poll, Provider};

fn main() {
    env_logger::init();
    let g: &dyn Provider = &solflow_lib::sync::google::Google;
    println!("ключи заданы: {}", g.configured());
    let code = g.device_code("Sol Flow · проверка").expect("код");
    println!("КОД {} СТРАНИЦА {}", code.user_code, code.verification_url);
    let tokens = loop {
        std::thread::sleep(Duration::from_secs(code.interval));
        match g.poll_token(&code).expect("опрос") {
            Poll::Pending => continue,
            Poll::Done(t) => break t,
        }
    };
    println!("вошли; refresh-токен: {}", if tokens.refresh_token.is_empty() { "нет" } else { "есть" });
    let token = tokens.access_token.clone();
    println!("аккаунт: {}", g.account(&token).expect("аккаунт"));
    g.prepare(&token).expect("папки");
    println!("папки на месте");
    g.upload(&token, Folder::Meetings, "check.txt", b"sol flow check 1").expect("загрузка");
    g.upload(&token, Folder::Meetings, "check.txt", b"sol flow check 2").expect("перезапись");
    let list = g.list(&token, Folder::Meetings).expect("список");
    for f in &list {
        println!("в meetings: {} md5={} size={} modified={}", f.name, f.md5, f.size, f.modified);
    }
    let back = g.download(&token, Folder::Meetings, "check.txt").expect("скачивание");
    println!("скачали: {}", String::from_utf8_lossy(&back));
    g.delete(&token, Folder::Meetings, "check.txt").expect("удаление");
    let after = g.list(&token, Folder::Meetings).expect("список после");
    println!("после удаления файлов в meetings: {}", after.len());
    let refreshed = g.refresh(&tokens.refresh_token).expect("продление");
    println!("продление токена: ок, истекает через {} мин", (refreshed.expires_at - solflow_lib::sync::provider::now_ms()) / 60000);
    g.revoke(&refreshed.access_token);
    println!("токен отозван — проверка закончена");
}
