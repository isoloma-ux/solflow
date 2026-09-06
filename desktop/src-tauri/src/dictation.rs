use tauri::AppHandle;

pub struct Formatted {
    pub text: String,
    pub status: &'static str,
}

/// All failure paths return the caller's full text, never a partial generation.
pub fn format(app: &AppHandle, text: &str, enabled: bool) -> Formatted {
    let fallback = |status| Formatted { text: text.to_string(), status };
    if !enabled || text.is_empty() {
        log::info!("dictation punctuation: fast mode, no LLM pass");
        return fallback("off");
    }
    if !crate::dictation_text::eligible(text) { return fallback("too_long"); }
    if !crate::summary::model_ready(app) { return fallback("no_model"); }
    let gate = crate::meetings::inference_gate(app);
    let Ok(_guard) = gate.try_lock() else { return fallback("busy"); };
    let started = std::time::Instant::now();
    match crate::summary::punctuate_path(&crate::summary::model_path(app), text) {
        Ok(result) => {
            log::info!("dictation punctuation: {} ms", started.elapsed().as_millis());
            let status = if result == text { "unchanged" } else { "applied" };
            Formatted { text: result, status }
        }
        Err(e) => {
            // Do not log user text or generated content.
            log::warn!("dictation punctuation skipped: {e}");
            fallback("failed")
        }
    }
}
