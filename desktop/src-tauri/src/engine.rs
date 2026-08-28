//! Одна загруженная модель на всё приложение — как Engine на Android.
//! Движок тот же transcribe.cpp, здесь через официальный crate с Metal.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use transcribe_cpp::{Model, RunOptions, Session};

use crate::{cleanup, segmenter};

pub struct Engine {
    session: Mutex<Option<Session>>,
    pub model_name: Mutex<Option<String>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            model_name: Mutex::new(None),
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

    pub fn load(&self, path: &PathBuf) -> Result<()> {
        let model = Model::load(path).map_err(|e| anyhow!("модель не загрузилась: {e}"))?;
        let session = model
            .session()
            .map_err(|e| anyhow!("сессия не создалась: {e}"))?;
        *self.session.lock().unwrap() = Some(session);
        *self.model_name.lock().unwrap() = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string());
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    /// Отпускает модель: сотни мегабайт в памяти и на GPU ни к чему, пока
    /// диктовкой не пользуются. Путь запоминаем — по нему грузим обратно.
    pub fn unload(&self) {
        *self.session.lock().unwrap() = None;
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
