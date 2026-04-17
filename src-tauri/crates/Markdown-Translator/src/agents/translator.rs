use std::collections::HashSet;
use std::sync::Arc;

use tracing::{error, info, warn};

use crate::agents::prompts::{build_translator_prompts, parse_marked_translations};
use crate::error::{Error, Result};
use crate::provider::LlmProvider;
use crate::types::{DocumentContext, SegmentBundle, TranslationResult};

/// Agent responsible for translating a bundle of segments via LLM.
pub struct TranslatorAgent {
    provider: Arc<dyn LlmProvider>,
}

impl TranslatorAgent {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Translate all segments in a bundle, returning one `TranslationResult` per segment.
    ///
    /// Uses text-based prompts with integrity markers for robust parsing.
    /// Validates that every expected segment_id is present in the response.
    pub async fn translate_bundle(
        &self,
        bundle: &SegmentBundle,
        context: &DocumentContext,
        target_lang: &str,
    ) -> Result<Vec<TranslationResult>> {
        let (system, user) = build_translator_prompts(bundle, context, target_lang);

        info!(
            bundle_id = %bundle.bundle_id,
            segments = bundle.segments.len(),
            "TranslatorAgent: translating"
        );

        let response_text = self.provider.chat_text(&system, &user, "translate").await?;

        let parsed = parse_marked_translations(&response_text).ok_or_else(|| {
            error!(bundle_id = %bundle.bundle_id, "Failed to parse translation response with integrity markers");
            // Use a serde_json::Error from a failed parse as the source
            Error::LlmOutputParse {
                bundle_id: bundle.bundle_id.clone(),
                source: serde_json::from_str::<serde_json::Value>("").unwrap_err(),
            }
        })?;

        if parsed.is_empty() {
            return Err(Error::EmptyResponse {
                bundle_id: bundle.bundle_id.clone(),
            });
        }

        // Build TranslationResponse from parsed results
        let translations: Vec<TranslationResult> = parsed
            .into_iter()
            .map(|(segment_id, translated_text)| TranslationResult {
                segment_id,
                translated_text,
                notes: Vec::new(),
                applied_terms: Default::default(),
                confidence: 0.0,
            })
            .collect();

        // Validate completeness
        let translated_ids: HashSet<&str> = translations
            .iter()
            .map(|t| t.segment_id.as_str())
            .collect();
        let expected_ids: HashSet<&str> = bundle
            .segments
            .iter()
            .map(|s| s.segment_id.as_str())
            .collect();
        let missing: Vec<String> = expected_ids
            .difference(&translated_ids)
            .map(|s| (*s).to_owned())
            .collect();

        if !missing.is_empty() {
            warn!(
                bundle_id = %bundle.bundle_id,
                missing_segments = ?missing,
                "TranslatorAgent: some segments missing in response"
            );
            return Err(Error::MissingSegments {
                bundle_id: bundle.bundle_id.clone(),
                missing,
            });
        }

        // Preserve original order from bundle
        let ordered: Vec<TranslationResult> = bundle
            .segments
            .iter()
            .filter_map(|seg| {
                translations
                    .iter()
                    .find(|t| t.segment_id == seg.segment_id)
                    .cloned()
            })
            .collect();

        Ok(ordered)
    }
}
