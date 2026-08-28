//! Каталог моделей — тот же catalog.json, что на Android (генерируется
//! build-catalog.py из каталога Handy). На Mac показываем по одной
//! рекомендуемой версии на модель: выбор квантов — лишняя сложность здесь.
//!
//! Скачивание — системным curl: свой HTTP-стек ради загрузки файла не нужен,
//! а прогресс читается по росту файла (полный размер известен из каталога).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize)]
struct Catalog {
    mirrors: Vec<String>,
    #[serde(default)]
    languages: HashMap<String, CatalogLanguage>,
    models: Vec<CatalogModel>,
}

#[derive(Deserialize, Clone)]
struct CatalogLanguage {
    name: String,
    models: u32,
}

#[derive(Deserialize, Clone)]
struct CatalogModel {
    id: String,
    revision: String,
    name: String,
    description: String,
    languages: Vec<String>,
    language_count: u32,
    accuracy_score: u32,
    #[serde(default)]
    speed_score: u32,
    #[serde(default)]
    speed_note: String,
    #[serde(default)]
    streaming: bool,
    #[serde(default)]
    translate: bool,
    default_quant: String,
    files: Vec<CatalogFile>,
}

#[derive(Deserialize, Clone)]
struct CatalogFile {
    filename: String,
    quant: String,
    size_bytes: u64,
    sha256: String,
}

/// Строка каталога для интерфейса.
#[derive(Serialize, Clone)]
pub struct ModelRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub languages: String,
    pub language_codes: Vec<String>,
    pub language_count: u32,
    pub accuracy: u32,
    pub speed: u32,
    /// Человеческое объяснение скорости и качества из каталога.
    pub note: String,
    /// Умеет показывать текст по ходу речи.
    pub streaming: bool,
    /// Умеет переводить на английский.
    pub translate: bool,
    pub filename: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub active: bool,
    pub progress: Option<u8>,
}

/// Язык для фильтра: код, русское название, сколько моделей его знают.
#[derive(Serialize, Clone)]
pub struct LanguageRow {
    pub code: String,
    pub name: String,
    pub models: u32,
}

pub struct ModelStore {
    catalog: Catalog,
    /// Проценты идущих загрузок по имени файла.
    progress: Mutex<HashMap<String, u8>>,
    cancel: Mutex<HashMap<String, std::sync::Arc<AtomicBool>>>,
}

impl ModelStore {
    pub fn new() -> Self {
        let catalog: Catalog =
            serde_json::from_str(include_str!("../catalog.json")).expect("каталог сломан");
        Self {
            catalog,
            progress: Mutex::new(HashMap::new()),
            cancel: Mutex::new(HashMap::new()),
        }
    }

    fn models_dir(app: &AppHandle) -> PathBuf {
        app.path()
            .app_data_dir()
            .map(|d| d.join("models"))
            .unwrap_or_default()
    }

    /// Рекомендуемый файл модели: default_quant, иначе первый.
    fn default_file<'a>(model: &'a CatalogModel) -> Option<&'a CatalogFile> {
        model
            .files
            .iter()
            .find(|f| f.quant == model.default_quant)
            .or_else(|| model.files.first())
    }

    pub fn rows(&self, app: &AppHandle, active: &Option<String>) -> Vec<ModelRow> {
        let dir = Self::models_dir(app);
        let progress = self.progress.lock().unwrap();
        let mut rows: Vec<ModelRow> = self
            .catalog
            .models
            .iter()
            .filter_map(|m| {
                let file = Self::default_file(m)?;
                let path = dir.join(&file.filename);
                let downloaded = path.exists()
                    && path.metadata().map(|md| md.len() == file.size_bytes).unwrap_or(false);
                Some(ModelRow {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    description: m.description.clone(),
                    languages: if m.language_count > 1 {
                        format!("языков: {}", m.language_count)
                    } else {
                        m.languages.join(", ")
                    },
                    language_codes: m.languages.clone(),
                    language_count: m.language_count,
                    accuracy: m.accuracy_score,
                    speed: m.speed_score,
                    note: m.speed_note.clone(),
                    streaming: m.streaming,
                    translate: m.translate,
                    filename: file.filename.clone(),
                    size_bytes: file.size_bytes,
                    downloaded,
                    active: active.as_deref() == Some(file.filename.as_str()),
                    progress: progress.get(&file.filename).copied(),
                })
            })
            .collect();
        // Активная и скачанные сверху, дальше по точности.
        let accuracy: HashMap<String, u32> = self
            .catalog
            .models
            .iter()
            .map(|m| (m.id.clone(), m.accuracy_score))
            .collect();
        rows.sort_by_key(|r| {
            (
                !r.active,
                !r.downloaded,
                std::cmp::Reverse(accuracy.get(&r.id).copied().unwrap_or(0)),
            )
        });
        rows
    }

    /// Языки каталога, популярные сверху — как на Android.
    pub fn languages(&self) -> Vec<LanguageRow> {
        let mut rows: Vec<LanguageRow> = self
            .catalog
            .languages
            .iter()
            .map(|(code, lang)| LanguageRow {
                code: code.clone(),
                name: lang.name.clone(),
                models: lang.models,
            })
            .collect();
        rows.sort_by_key(|l| std::cmp::Reverse(l.models));
        rows
    }

    /// Источники по очереди: Hugging Face, затем зеркало Handy (оно хранит
    /// только рекомендуемые версии — как раз наш случай).
    fn urls(&self, model: &CatalogModel, file: &CatalogFile) -> Vec<String> {
        let mut urls = vec![format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            model.id, model.revision, file.filename
        )];
        for mirror in &self.catalog.mirrors {
            urls.push(format!("{}/{}", mirror.trim_end_matches('/'), file.filename));
        }
        urls
    }

    pub fn download(&self, app: &AppHandle, model_id: &str) -> Result<()> {
        let model = self
            .catalog
            .models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .ok_or_else(|| anyhow!("модель не найдена"))?;
        let file = Self::default_file(&model)
            .cloned()
            .ok_or_else(|| anyhow!("у модели нет файлов"))?;

        {
            let progress = self.progress.lock().unwrap();
            if progress.contains_key(&file.filename) {
                return Ok(());
            }
        }
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        self.progress.lock().unwrap().insert(file.filename.clone(), 0);
        self.cancel
            .lock()
            .unwrap()
            .insert(file.filename.clone(), cancelled.clone());

        let dir = Self::models_dir(app);
        let _ = std::fs::create_dir_all(&dir);
        let urls = self.urls(&model, &file);
        let app = app.clone();

        std::thread::spawn(move || {
            let target = dir.join(&file.filename);
            let tmp = dir.join(format!("{}.part", file.filename));
            let mut ok = false;

            'sources: for url in urls {
                let _ = std::fs::remove_file(&tmp);
                let mut child = match Command::new("/usr/bin/curl")
                    .args(["-L", "-f", "-s", "--connect-timeout", "10", "-o"])
                    .arg(&tmp)
                    .arg(&url)
                    .spawn()
                {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                loop {
                    if cancelled.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        break 'sources;
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            if status.success()
                                && tmp.metadata().map(|m| m.len()).unwrap_or(0) == file.size_bytes
                                && sha256_ok(&tmp, &file.sha256)
                            {
                                let _ = std::fs::rename(&tmp, &target);
                                ok = true;
                                break 'sources;
                            }
                            break;
                        }
                        Ok(None) => {
                            let done = tmp.metadata().map(|m| m.len()).unwrap_or(0);
                            let pct = ((done * 100) / file.size_bytes.max(1)).min(99) as u8;
                            let store = app.state::<ModelStore>();
                            store.progress.lock().unwrap().insert(file.filename.clone(), pct);
                            let _ = app.emit("solflow-models", ());
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                        Err(_) => break,
                    }
                }
            }

            let _ = std::fs::remove_file(&tmp);
            let store = app.state::<ModelStore>();
            store.progress.lock().unwrap().remove(&file.filename);
            store.cancel.lock().unwrap().remove(&file.filename);
            if !ok && !cancelled.load(Ordering::Relaxed) {
                log::error!("не удалось скачать {}", file.filename);
            }
            let _ = app.emit("solflow-models", ());
        });
        Ok(())
    }

    pub fn cancel_download(&self, filename: &str) {
        if let Some(flag) = self.cancel.lock().unwrap().get(filename) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn delete(&self, app: &AppHandle, filename: &str) {
        let _ = std::fs::remove_file(Self::models_dir(app).join(filename));
    }
}

fn sha256_ok(path: &PathBuf, expected: &str) -> bool {
    if expected.is_empty() {
        return true;
    }
    Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .next()
                .map(|h| h.eq_ignore_ascii_case(expected))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}
