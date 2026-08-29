//! Одна загруженная модель на всё приложение — как Engine на Android.
//! Движок тот же transcribe.cpp, здесь через официальный crate с Metal.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use transcribe_cpp::{
    Backend, CancelToken, Device, Model, ModelOptions, RunOptions, Session, SessionOptions,
};

use crate::{cleanup, segmenter};

pub struct Engine {
    session: Mutex<Option<Session>>,
    pub model_name: Mutex<Option<String>>,
    /// На чём считает загруженная модель — окно показывает это словами.
    pub device: Mutex<Option<Device>>,
    /// Флаг «бросай считать»: движок опрашивает его между шагами декодера,
    /// поэтому отмена срабатывает посреди куска, а не после него.
    cancel: CancelToken,
}

/// Сколько потоков отдать движку. Ноль — «как решит библиотека», а решает
/// она консервативно: на многоядерном процессоре это заметно медленнее, чем
/// нужно. На Apple тяжёлое считает Metal, и лишние потоки только мешают
/// ему, — там оставляем выбор библиотеке.
fn cpu_threads() -> i32 {
    if cfg!(target_os = "macos") {
        return 0;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // Одно ядро оставляем интерфейсу, выше восьми ggml почти не ускоряется.
    cores.saturating_sub(1).clamp(1, 8) as i32
}

impl Engine {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            model_name: Mutex::new(None),
            device: Mutex::new(None),
            cancel: CancelToken::new(),
        }
    }

    /// Первый .gguf из папки моделей приложения.
    pub fn find_model(models_dir: &PathBuf) -> Option<PathBuf> {
        std::fs::read_dir(models_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map(|e| e == "gguf").unwrap_or(false))
    }

    /// [use_gpu] — пробовать ли видеокарту. Auto берёт лучшее из того, что
    /// собрано и завелось на этой машине, и сам откатывается на процессор;
    /// Cpu — строго процессор.
    pub fn load(&self, path: &PathBuf, use_gpu: bool) -> Result<()> {
        let options = ModelOptions {
            backend: if use_gpu { Backend::Auto } else { Backend::Cpu },
            device: None,
        };
        let model = Model::load_with(path, &options)
            .map_err(|e| anyhow!("модель не загрузилась: {e}"))?;
        let mut session = model
            .session_with(&SessionOptions {
                n_threads: cpu_threads(),
                ..Default::default()
            })
            .map_err(|e| anyhow!("сессия не создалась: {e}"))?;
        self.cancel.reset();
        session.set_cancel_token(&self.cancel);
        *self.session.lock().unwrap() = Some(session);
        *self.model_name.lock().unwrap() = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string());
        *self.device.lock().unwrap() = model.device().ok();
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    /// Отпускает модель: сотни мегабайт в памяти и на GPU ни к чему, пока
    /// диктовкой не пользуются. Путь запоминаем — по нему грузим обратно.
    pub fn unload(&self) {
        *self.session.lock().unwrap() = None;
        *self.device.lock().unwrap() = None;
    }

    /// Просит движок бросить текущий кусок. Флаг снимается перед следующим
    /// куском — иначе отменённая встреча утащила бы за собой следующую.
    pub fn request_cancel(&self) {
        self.cancel.cancel();
    }

    pub fn clear_cancel(&self) {
        self.cancel.reset();
    }

    /// Чем считается модель, словами для окна: «видеокарта Intel Arc» или
    /// «процессор».
    pub fn device_label(&self) -> Option<String> {
        let device = self.device.lock().unwrap();
        let device = device.as_ref()?;
        Some(match device.kind.as_str() {
            "cpu" | "accel" => "процессор".to_string(),
            _ if device.description.is_empty() => format!("видеокарта ({})", device.name),
            _ => format!("видеокарта {}", device.description),
        })
    }

    /// Один кусок расшифровки встречи: куски уже нарезаны по паузам, чистка —
    /// на стороне вызывающего. Блокировка на кусок, а не на файл: диктовка,
    /// нажатая посреди долгой расшифровки, ждёт секунды, а не минуты.
    pub fn transcribe_segment(&self, pcm: &[f32]) -> Result<String> {
        let mut guard = self.session.lock().unwrap();
        let session = guard.as_mut().ok_or_else(|| anyhow!("модель не загружена"))?;
        let result = session
            .run(pcm, &RunOptions::default())
            .map_err(|e| anyhow!("распознавание не удалось: {e}"))?;
        Ok(result.text.trim().to_string())
    }

    pub fn transcribe(&self, pcm: &[f32]) -> Result<String> {
        self.transcribe_with(pcm, false)
    }

    /// Распознаёт запись целиком: длинная режется по паузам, куски
    /// склеиваются через пробел, чистка — после склейки.
    pub fn transcribe_with(&self, pcm: &[f32], drop_parasites: bool) -> Result<String> {
        let mut guard = self.session.lock().unwrap();
        let session = guard.as_mut().ok_or_else(|| anyhow!("модель не загружена"))?;

        let mut parts = Vec::new();
        for segment in segmenter::split(pcm, crate::audio::TARGET_RATE) {
            let result = session
                .run(&segment, &RunOptions::default())
                .map_err(|e| anyhow!("распознавание не удалось: {e}"))?;
            let text = result.text.trim().to_string();
            if !text.is_empty() {
                parts.push(text);
            }
        }
        Ok(cleanup::clean_with(&parts.join(" "), drop_parasites))
    }
}
