use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use crate::agents::prompts::{build_guard_prompts, parse_marked_translations};
use crate::error::Result;
use crate::provider::LlmProvider;
use crate::types::{SegmentBundle, TranslationResult};

/// Agent responsible for repairing Markdown format issues in translations.
pub struct FormatGuardAgent {
    provider: Option<Arc<dyn LlmProvider>>,
}

impl FormatGuardAgent {
    pub fn new(provider: Option<Arc<dyn LlmProvider>>) -> Self {
        Self { provider }
    }

    /// Repair formatting issues in translations.
    ///
    /// If no provider is configured, returns the original translations unchanged.
    pub async fn repair_bundle(
        &self,
        bundle: &SegmentBundle,
        translations: &[TranslationResult],
    ) -> Result<Vec<TranslationResult>> {
        let provider = match &self.provider {
            Some(p) => p,
            None => return Ok(translations.to_vec()),
        };

        let (system, user) = build_guard_prompts(bundle, translations);

        info!(
            bundle_id = %bundle.bundle_id,
            segments = translations.len(),
            "FormatGuardAgent: repairing"
        );

        let response_text = provider.chat_text(&system, &user, "format_guard").await?;

        let parsed = match parse_marked_translations(&response_text) {
            Some(p) => p,
            None => return Ok(translations.to_vec()),
        };

        // Merge: use repaired version where available.
        let repaired: HashMap<&str, &str> = parsed
            .iter()
            .map(|(id, text)| (id.as_str(), text.as_str()))
            .collect();

        let results = translations
            .iter()
            .map(|orig| {
                if let Some(text) = repaired.get(orig.segment_id.as_str()) {
                    TranslationResult {
                        segment_id: orig.segment_id.clone(),
                        translated_text: (*text).to_string(),
                        notes: orig.notes.clone(),
                        applied_terms: orig.applied_terms.clone(),
                        confidence: orig.confidence,
                    }
                } else {
                    orig.clone()
                }
            })
            .collect();

        Ok(results)
    }
}
