//! Translate skill copy into the language chosen in Settings.
//! Simplified Chinese is the default.
//!
//! Machine translation uses Google. LLM translation uses the OpenAI-compatible
//! endpoint (`llm_base_url`), model id (`llm_model`), and manually entered API
//! key (`secrets/translation/api_key`) configured in Settings → Translation.
//! The shell asks for missing strings; it does not call the network itself.

mod cache;
mod config;
mod engine;
mod models;
mod themes;

pub use cache::{lookup, remember};
pub use config::{
    DEFAULT_LLM_URL, DEFAULT_TARGET, Engine, LlmAuth, TRANSLATION_LANGUAGES, TranslationConfig,
    TranslationLanguage, canonical_target, description_choice, descriptions_blocked,
    descriptions_enabled, load_api_key, load_config, save_api_key, save_config,
    set_description_choice, set_description_choices, translation_language,
};
pub use models::{ModelListError, list_models};
pub use themes::{DEFAULT_READER, Theme, reader_themes};

/// One finished translation. Failures are omitted so the caller can retry later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    pub source: String,
    pub text: String,
}

/// One batch. HTTP 400 sets [`FillReport::rejection`] and skips the rest of the queue.
#[derive(Clone, Debug, Default)]
pub struct FillReport {
    pub done: Vec<Translation>,
    pub rejection: Option<ModelRejection>,
}

/// The configured model refused the chat request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRejection {
    pub model: String,
    pub detail: String,
}

/// The language Settings currently translates into.
pub fn active_target() -> &'static str {
    config::active_target_code()
}

/// Text that is not already in `target`. An unknown target is never translated.
///
/// zh-CN / zh-TW share Han characters. A short marker list separates them.
/// ponytail: not a converter; shared-only Han stays put.
pub fn needs_translation(text: &str, target: &str) -> bool {
    if translation_language(target).is_none() {
        return false;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if target == "zh-CN" && contains_any(trimmed, TRADITIONAL_ONLY) {
        return true;
    }
    if target == "zh-TW" && contains_any(trimmed, SIMPLIFIED_ONLY) {
        return true;
    }
    let counts = letter_counts(trimmed);
    let (home, foreign) = match target {
        "en" => (counts.latin, counts.han + counts.kana + counts.hangul),
        "ja" => (counts.kana, counts.latin + counts.han + counts.hangul),
        "ko" => (counts.hangul, counts.latin + counts.han + counts.kana),
        _ => (counts.han, counts.latin + counts.kana + counts.hangul),
    };
    home == 0 || (foreign > 0 && home * 2 < foreign)
}

const SIMPLIFIED_ONLY: &str = "这说们为会时过发对现经还问后样从构应国产";
const TRADITIONAL_ONLY: &str = "這說們為會時過發對現經還問後樣從構應國產";

struct LetterCounts {
    latin: usize,
    han: usize,
    kana: usize,
    hangul: usize,
}

fn letter_counts(text: &str) -> LetterCounts {
    let mut counts = LetterCounts {
        latin: 0,
        han: 0,
        kana: 0,
        hangul: 0,
    };
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            counts.latin += 1;
        } else if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            counts.han += 1;
        } else if ('\u{3040}'..='\u{30ff}').contains(&ch) {
            counts.kana += 1;
        } else if ('\u{ac00}'..='\u{d7af}').contains(&ch) {
            counts.hangul += 1;
        }
    }
    counts
}

fn contains_any(text: &str, markers: &str) -> bool {
    text.chars().any(|ch| markers.contains(ch))
}

enum Attempt {
    Ready(Translation),
    Rejected(String),
    Miss,
}

/// Translate `sources` that are not already cached. Writes each success before returning.
///
/// The first HTTP 400 stops the rest of this batch. Other failures are logged and left
/// for a later paint.
pub async fn fill(sources: Vec<String>, target: &str, auth: LlmAuth) -> FillReport {
    if translation_language(target).is_none() {
        return FillReport::default();
    }
    let config = load_config().unwrap_or_default();
    let mut done = Vec::new();
    let mut pending = Vec::new();
    for source in sources {
        if lookup(&source, target).is_some() {
            continue;
        }
        if !needs_translation(&source, target) {
            continue;
        }
        pending.push(source);
    }
    let mut rejection_detail = None;
    for chunk in pending.chunks(4) {
        let mut tasks = Vec::new();
        for source in chunk {
            let source = source.clone();
            let target = target.to_string();
            let config = config.clone();
            let auth = auth.clone();
            tasks.push(tokio::spawn(async move {
                match engine::translate_text(&source, &target, &config, &auth).await {
                    Ok(text) => Attempt::Ready(Translation { source, text }),
                    Err(error) => {
                        if let Some(detail) = engine::llm_rejection_detail(&error) {
                            if detail.is_empty() {
                                tracing::warn!("translation failed: {error}");
                            } else {
                                tracing::warn!("translation failed: {error}: {detail}");
                            }
                            Attempt::Rejected(detail)
                        } else {
                            tracing::warn!("translation failed: {error}");
                            Attempt::Miss
                        }
                    }
                }
            }));
        }
        for task in tasks {
            match task.await {
                Ok(Attempt::Ready(item)) => {
                    remember(&item.source, target, &item.text);
                    done.push(item);
                }
                Ok(Attempt::Rejected(detail)) if rejection_detail.is_none() => {
                    rejection_detail = Some(detail);
                }
                Ok(Attempt::Rejected(_) | Attempt::Miss) | Err(_) => {}
            }
        }
        if rejection_detail.is_some() {
            break;
        }
    }
    FillReport {
        done,
        rejection: rejection_detail.map(|detail| ModelRejection {
            model: config.llm_model.trim().to_string(),
            detail,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::needs_translation;

    #[test]
    fn english_needs_chinese_and_chinese_does_not() {
        assert!(needs_translation("Build desktop apps.", "zh-CN"));
        assert!(!needs_translation("构建桌面应用", "zh-CN"));
        assert!(!needs_translation("Build desktop apps.", "en"));
        assert!(needs_translation("构建桌面应用", "en"));
        assert!(needs_translation("Build desktop apps.", "ja"));
        assert!(!needs_translation("これはテストです", "ja"));
        assert!(needs_translation("这是一个技能", "zh-TW"));
        assert!(!needs_translation("這是一個技能", "zh-TW"));
        assert!(needs_translation("這是一個技能", "zh-CN"));
        assert!(!needs_translation("   ", "zh-CN"));
        assert!(!needs_translation("Build desktop apps.", "klingon"));
    }
}
