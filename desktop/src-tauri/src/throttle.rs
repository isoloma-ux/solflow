//! События окну не чаще нескольких раз в секунду. Прогресс загрузки и
//! расшифровки шлёт уведомление на каждый процент, а быстрая сеть — десятки
//! в секунду; окно на каждое перерисовывало списки, и интерфейс мерцал так,
//! что нельзя было перетащить запись в проект. Лишние уведомления
//! схлопываются, последнее всегда доходит.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

const GAP: Duration = Duration::from_millis(200);

struct Gate {
    last: Instant,
    pending: Arc<AtomicBool>,
}

static GATES: Mutex<Option<HashMap<&'static str, Gate>>> = Mutex::new(None);

pub fn emit(app: &AppHandle, event: &'static str) {
    let mut gates = GATES.lock().unwrap();
    let gates = gates.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    match gates.get_mut(event) {
        Some(gate) if now.duration_since(gate.last) < GAP => {
            // Слишком часто: одно отложенное уведомление на всю пачку.
            if !gate.pending.swap(true, Ordering::SeqCst) {
                let pending = gate.pending.clone();
                let wait = GAP - now.duration_since(gate.last);
                let app = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(wait);
                    pending.store(false, Ordering::SeqCst);
                    if let Some(gates) = GATES.lock().unwrap().as_mut() {
                        if let Some(gate) = gates.get_mut(event) {
                            gate.last = Instant::now();
                        }
                    }
                    let _ = app.emit(event, ());
                });
            }
        }
        Some(gate) => {
            gate.last = now;
            let _ = app.emit(event, ());
        }
        None => {
            gates.insert(
                event,
                Gate {
                    last: now,
                    pending: Arc::new(AtomicBool::new(false)),
                },
            );
            let _ = app.emit(event, ());
        }
    }
}
