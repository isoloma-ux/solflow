//! Запись с микрофона. cpal-поток живёт в собственном треде (Stream не Send),
//! команды приходят по каналу. Звук копится как f32 моно на нативной частоте
//! устройства и приводится к 16 кГц при остановке — движок ждёт только их.
//!
//! Поток открывается заранее и между записями стоит на паузе: открытие
//! микрофона на Windows (WASAPI) занимает до секунд, и всё это время
//! человек уже говорил в закрытый микрофон, а пилюля ждала. Запуск
//! готового потока — миллисекунды. Пауза означает остановленный поток:
//! система не считает микрофон занятым, индикатор не горит.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const TARGET_RATE: usize = 16_000;

enum Command {
    /// Имя микрофона из настроек; None — системный по умолчанию.
    Start(Option<String>, Sender<Result<()>>),
    Stop(Sender<Vec<f32>>),
    /// Открыть поток заранее (или переоткрыть под другой микрофон), чтобы
    /// следующий Start был мгновенным. Ничего не ждёт.
    Prepare(Option<String>),
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

    /// Открыть поток заранее — при старте приложения и после смены
    /// микрофона в настройках.
    pub fn prepare(&self, device: Option<String>) {
        let _ = self.tx.send(Command::Prepare(device));
    }
}

struct Active {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    sample_rate: usize,
    /// Какой микрофон просили и какой открылся на деле — чтобы заметить,
    /// что настройка или системный микрофон сменились.
    wanted: Option<String>,
    device_name: String,
    /// Поток пожаловался на ошибку (устройство вынули) — переоткрыть.
    broken: Arc<AtomicBool>,
    recording: bool,
}

impl Active {
    /// Годится ли заранее открытый поток для следующей записи.
    fn reusable(&self, wanted: &Option<String>) -> bool {
        if self.recording || self.broken.load(Ordering::Relaxed) || &self.wanted != wanted {
            return false;
        }
        // Без явного выбора микрофона поток был открыт на системном по
        // умолчанию, а тот мог смениться (воткнули наушники). Сам вопрос
        // системе дешёвый, в отличие от открытия потока.
        if wanted.is_none() {
            let current = cpal::default_host()
                .default_input_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_default();
            if current != self.device_name {
                log::info!(
                    "системный микрофон сменился: «{}» → «{current}», открываю заново",
                    self.device_name
                );
                return false;
            }
        }
        true
    }
}

fn audio_thread(rx: Receiver<Command>, level: Arc<AtomicU32>) {
    let mut active: Option<Active> = None;

    while let Ok(command) = rx.recv() {
        match command {
            Command::Start(device, reply) => {
                let _ = reply.send(begin(&mut active, &level, device));
            }
            Command::Stop(reply) => {
                level.store(0f32.to_bits(), Ordering::Relaxed);
                let pcm = match active.as_mut() {
                    Some(a) if a.recording => {
                        a.recording = false;
                        // Поток остаётся открытым на паузе — следующая
                        // запись начнётся сразу. Если пауза не удалась,
                        // поток ненадёжен: закрываем.
                        if let Err(e) = a.stream.pause() {
                            log::warn!("микрофон не встал на паузу: {e}");
                            a.broken.store(true, Ordering::Relaxed);
                        }
                        let raw = a
                            .buf
                            .lock()
                            .map(|mut b| std::mem::take(&mut *b))
                            .unwrap_or_default();
                        resample(&raw, a.sample_rate, TARGET_RATE)
                    }
                    _ => Vec::new(),
                };
                let _ = reply.send(pcm);
                if active.as_ref().map(|a| a.broken.load(Ordering::Relaxed)) == Some(true) {
                    active = None;
                }
            }
            Command::Prepare(device) => {
                let fresh = match &active {
                    Some(a) => !a.reusable(&device),
                    None => true,
                };
                if fresh && !active.as_ref().map(|a| a.recording).unwrap_or(false) {
                    active = None;
                    match open_stream(level.clone(), device) {
                        Ok(a) => active = Some(a),
                        Err(e) => log::warn!("микрофон заранее не открылся: {e}"),
                    }
                }
            }
        }
    }
}

/// Начать запись: на готовом потоке — просто снять с паузы, иначе открыть.
/// Если готовый поток не запустился (устройство вынули, пока стояли на
/// паузе), пробуем один раз заново с нуля.
fn begin(active: &mut Option<Active>, level: &Arc<AtomicU32>, wanted: Option<String>) -> Result<()> {
    let reuse = active.as_ref().map(|a| a.reusable(&wanted)).unwrap_or(false);
    if !reuse {
        *active = None;
        *active = Some(open_stream(level.clone(), wanted.clone())?);
    }
    let started = std::time::Instant::now();
    let a = active.as_mut().expect("поток только что открыт");
    if let Ok(mut b) = a.buf.lock() {
        b.clear();
    }
    if let Err(e) = a.stream.play() {
        if reuse {
            log::warn!("готовый поток не запустился ({e}), открываю заново");
            *active = None;
            let mut fresh = open_stream(level.clone(), wanted)?;
            fresh
                .stream
                .play()
                .map_err(|e| anyhow!("поток не стартовал: {e}"))?;
            fresh.recording = true;
            *active = Some(fresh);
            return Ok(());
        }
        *active = None;
        return Err(anyhow!("поток не стартовал: {e}"));
    }
    a.recording = true;
    log::info!(
        "запись: поток {} запущен за {} мс",
        if reuse { "заранее открытый" } else { "новый" },
        started.elapsed().as_millis()
    );
    Ok(())
}

/// Открывает поток и оставляет его на паузе — запускает `begin`.
fn open_stream(level: Arc<AtomicU32>, wanted: Option<String>) -> Result<Active> {
    // Каждый шаг открытия микрофона замеряется: на Windows между нажатием
    // сочетания и началом записи бывает пауза в секунды, и по одному
    // общему числу не понять, где она — в поиске устройства, в WASAPI
    // или в запуске потока.
    let started = std::time::Instant::now();
    let device = pick_device(&wanted).ok_or_else(|| anyhow!("микрофон не найден"))?;
    let picked_ms = started.elapsed().as_millis();
    let config = device
        .default_input_config()
        .map_err(|e| anyhow!("микрофон не открылся: {e}"))?;
    let config_ms = started.elapsed().as_millis() - picked_ms;

    let sample_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let cb_buf = buf.clone();
    let broken = Arc::new(AtomicBool::new(false));
    let cb_broken = broken.clone();
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
            move |e| {
                log::error!("ошибка аудиопотока: {e}");
                cb_broken.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| anyhow!("не удалось открыть поток: {e}"))?;
    let built_ms = started.elapsed().as_millis() - picked_ms - config_ms;

    let total_ms = started.elapsed().as_millis();
    let device_name = device.name().unwrap_or_default();
    let line = format!(
        "микрофон «{device_name}» {sample_rate} Гц, {channels} кан., {format:?}: открыт за \
         {total_ms} мс (устройство {picked_ms}, формат {config_ms}, поток {built_ms})"
    );
    // Долгий старт — уже жалоба: пусть попадёт в отчёт о проблеме.
    if total_ms >= 500 {
        log::warn!("{line}");
    } else {
        log::info!("{line}");
    }

    Ok(Active {
        stream,
        buf,
        sample_rate,
        wanted,
        device_name,
        broken,
        recording: false,
    })
}

/// Держит звуковой выход проснувшимся: тихий поток нулей в системное
/// устройство вывода. На Windows звуковая карта после простоя уходит в сон
/// и просыпается до двух секунд — ровно настолько запаздывал сигнал начала
/// записи, хотя сама запись уже шла. Пока поток играет тишину, устройство
/// не засыпает и сигнал звучит сразу.
///
/// Поток живёт в своём треде (cpal::Stream не Send) и раз в несколько
/// минут переоткрывается: системное устройство вывода могло смениться —
/// воткнули наушники, — а тишина продолжала бы идти в колонки.
pub struct OutputKeeper {
    tx: Sender<bool>,
}

impl OutputKeeper {
    pub fn spawn() -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || keeper_thread(rx));
        Self { tx }
    }

    /// Включить или отпустить выход. Ничего не ждёт.
    pub fn set(&self, on: bool) {
        let _ = self.tx.send(on);
    }
}

const KEEPER_REOPEN: std::time::Duration = std::time::Duration::from_secs(300);

fn keeper_thread(rx: Receiver<bool>) {
    use std::sync::mpsc::RecvTimeoutError;

    let mut wanted = false;
    let mut stream: Option<cpal::Stream> = None;
    loop {
        match rx.recv_timeout(KEEPER_REOPEN) {
            Ok(on) => wanted = on,
            // Плановое переоткрытие под текущее системное устройство.
            Err(RecvTimeoutError::Timeout) => stream = None,
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if !wanted {
            stream = None;
            continue;
        }
        if stream.is_none() {
            match open_silence() {
                Ok(s) => stream = Some(s),
                Err(e) => log::warn!("звуковой выход не удалось держать наготове: {e}"),
            }
        }
    }
}

fn open_silence() -> Result<cpal::Stream> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("устройство вывода не найдено"))?;
    let config = device
        .default_output_config()
        .map_err(|e| anyhow!("выход не открылся: {e}"))?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let err = |e| log::warn!("тихий поток: {e}");
    let stream = match format {
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            |data: &mut [i16], _| data.fill(0),
            err,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config,
            |data: &mut [u16], _| data.fill(u16::MAX / 2),
            err,
            None,
        ),
        _ => device.build_output_stream(
            &config,
            |data: &mut [f32], _| data.fill(0.0),
            err,
            None,
        ),
    }
    .map_err(|e| anyhow!("тихий поток не открылся: {e}"))?;
    stream
        .play()
        .map_err(|e| anyhow!("тихий поток не стартовал: {e}"))?;
    log::info!(
        "звуковой выход «{}» держится наготове",
        device.name().unwrap_or_default()
    );
    Ok(stream)
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
