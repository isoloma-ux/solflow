//! Запись с микрофона. cpal-поток живёт в собственном треде (Stream не Send),
//! команды приходят по каналу. Звук копится как f32 моно на нативной частоте
//! устройства и приводится к 16 кГц при остановке — движок ждёт только их.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const TARGET_RATE: usize = 16_000;

enum Command {
    /// Имя микрофона из настроек; None — системный по умолчанию.
    Start(Option<String>, Sender<Result<()>>),
    Stop(Sender<Vec<f32>>),
}

/// Микрофоны, которые видит система, — для списка в настройках.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Микрофон по имени из настроек; если его нет на месте (наушники вынули),
/// молча берём системный — запись важнее точного совпадения.
pub fn pick_device(wanted: &Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = wanted {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(found) =
                devices.find(|d| d.name().map(|n| &n == name).unwrap_or(false))
            {
                return Some(found);
            }
            log::warn!("микрофон «{name}» не найден, беру системный");
        }
    }
    host.default_input_device()
}

pub struct Recorder {
    tx: Sender<Command>,
    /// Текущая громкость [0..1] — для волны в интерфейсе (f32 в битах).
    level: Arc<AtomicU32>,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (tx, rx) = channel();
        let level = Arc::new(AtomicU32::new(0));
        let thread_level = level.clone();
        std::thread::spawn(move || audio_thread(rx, thread_level));
        Self { tx, level }
    }

    pub fn start(&self, device: Option<String>) -> Result<()> {
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(Command::Start(device, reply_tx))
            .map_err(|_| anyhow!("аудиопоток недоступен"))?;
        reply_rx.recv().map_err(|_| anyhow!("аудиопоток не ответил"))?
    }

    /// Останавливает запись и отдаёт 16 кГц моно.
    pub fn stop(&self) -> Result<Vec<f32>> {
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(Command::Stop(reply_tx))
            .map_err(|_| anyhow!("аудиопоток недоступен"))?;
        reply_rx.recv().map_err(|_| anyhow!("аудиопоток не ответил"))
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

struct Active {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    sample_rate: usize,
}

fn audio_thread(rx: Receiver<Command>, level: Arc<AtomicU32>) {
    let mut active: Option<Active> = None;

    while let Ok(command) = rx.recv() {
        match command {
            Command::Start(device, reply) => {
                let result = start_stream(level.clone(), device).map(|a| {
                    active = Some(a);
                });
                let _ = reply.send(result);
            }
            Command::Stop(reply) => {
                level.store(0f32.to_bits(), Ordering::Relaxed);
                let pcm = match active.take() {
                    Some(a) => {
                        let raw = a.buf.lock().map(|b| b.clone()).unwrap_or_default();
                        resample(&raw, a.sample_rate, TARGET_RATE)
                    }
                    None => Vec::new(),
                };
                let _ = reply.send(pcm);
            }
        }
    }
}

fn start_stream(level: Arc<AtomicU32>, wanted: Option<String>) -> Result<Active> {
    let device = pick_device(&wanted).ok_or_else(|| anyhow!("микрофон не найден"))?;
    let config = device
        .default_input_config()
        .map_err(|e| anyhow!("микрофон не открылся: {e}"))?;

    let sample_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;
    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let cb_buf = buf.clone();
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                // Сведение каналов в моно усреднением + уровень для волны.
                let mut sum = 0.0f64;
                let mut mono = Vec::with_capacity(data.len() / channels);
                for frame in data.chunks_exact(channels) {
                    let v = frame.iter().sum::<f32>() / channels as f32;
                    mono.push(v);
                    sum += (v as f64) * (v as f64);
                }
                if !mono.is_empty() {
                    let rms = (sum / mono.len() as f64).sqrt();
                    // Поджато корнем, как на Android: тихая речь иначе не видна.
                    let shown = ((rms.sqrt() * 2.2) as f32).clamp(0.0, 1.0);
                    level.store(shown.to_bits(), Ordering::Relaxed);
                }
                if let Ok(mut b) = cb_buf.lock() {
                    b.extend_from_slice(&mono);
                }
            },
            |e| log::error!("ошибка аудиопотока: {e}"),
            None,
        )
        .map_err(|e| anyhow!("не удалось открыть поток: {e}"))?;

    stream.play().map_err(|e| anyhow!("поток не стартовал: {e}"))?;

    Ok(Active {
        _stream: stream,
        buf,
        sample_rate,
    })
}

/// Эталон для теста потокового ресемплера встреч.
#[cfg(test)]
pub fn resample_for_test(input: &[f32], src: usize, dst: usize) -> Vec<f32> {
    resample(input, src, dst)
}

/// Приведение частоты: вниз — среднее по интервалу (заодно фильтр от
/// алиасинга), вверх — линейная интерполяция. Порт ресемплера с Android.
fn resample(input: &[f32], src: usize, dst: usize) -> Vec<f32> {
    if src == dst || input.is_empty() {
        return input.to_vec();
    }
    let ratio = src as f64 / dst as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);

    if ratio > 1.0 {
        let mut position = 0.0f64;
        while position + ratio <= input.len() as f64 {
            let from = position as usize;
            let to = ((position + ratio) as usize).min(input.len());
            let sum: f32 = input[from..to].iter().sum();
            out.push(sum / (to - from).max(1) as f32);
            position += ratio;
        }
    } else {
        let mut position = 0.0f64;
        while (position + 1.0) < input.len() as f64 {
            let i = position as usize;
            let frac = (position - i as f64) as f32;
            out.push(input[i] * (1.0 - frac) + input[i + 1] * frac);
            position += ratio;
        }
    }
    out
}
