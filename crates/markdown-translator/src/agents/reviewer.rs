use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use crate::agents::prompts::{build_reviewer_prompts, parse_marked_translations};
use crate::error::Result;
use crate::provider::LlmProvider;
use crate::types::{DocumentContext, SegmentBundle, TranslationResult};

/// Agent responsible for reviewing and improving existing translations.
pub struct ReviewerAgent {
    provider: Option<Arc<dyn LlmProvider>>,
}

impl ReviewerAgent {
    pub fn new(provider: Option<Arc<dyn LlmProvider>>) -> Self {
        Self { provider }
    }

    /// Review translations and return improved versions.
    ///
    /// If no provider is configured, returns the original translations unchanged.
    pub async fn review_bundle(
        &self,
        bundle: &SegmentBundle,
        translations: &[TranslationResult],
        context: &DocumentContext,
    ) -> Result<Vec<TranslationResult>> {
        let provider = match &self.provider {
            Some(p) => p,
            None => return Ok(translations.to_vec()),
        };

        let (system, user) = build_reviewer_prompts(bundle, context, translations);

        info!(
            bundle_id = %bundle.bundle_id,
            segments = translations.len(),
            "ReviewerAgent: reviewing"
        );

        let response_text = provider.chat_text(&system, &user, "review").await?;

        let parsed = match parse_marked_translations(&response_text) {
            Some(p) => p,
            None => return Ok(translations.to_vec()),
        };

        // Merge: use reviewed version where available, keep original otherwise.
        let reviewed_map: HashMap<&str, &str> = parsed
            .iter()
            .map(|(id, text)| (id.as_str(), text.as_str()))
            .collect();

        let results = translations
            .iter()
            .map(|orig| {
                if let Some(text) = reviewed_map.get(orig.segment_id.as_str()) {
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
