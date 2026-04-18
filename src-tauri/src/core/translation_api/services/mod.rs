#![allow(dead_code)]

//! Translation service implementations and factory.
//!
//! Simplified to DeepL + DeepLX only.
//! Quality LLM translation is handled via the ai_provider module directly.

pub mod deepl;
pub mod mymemory;

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::{TranslationError, TranslationResult};

/// Core trait implemented by all translation providers.
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str;

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send;

    fn translate_stream(
        &self,
        _text: &str,
        _source_lang: &str,
        _target_lang: &str,
        _on_delta: impl Fn(String) + Send + Sync + 'static,
    ) -> impl Future<Output = Result<(), TranslationError>> + Send {
        async move { Ok(()) }
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

pub enum TranslationService {
    DeepL(deepl::DeepLService),
    DeepLX(deepl::DeepLXService),
    MyMemory(mymemory::MyMemoryService),
}

impl TranslationService {
    pub async fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, TranslationError> {
        match self {
            Self::DeepL(service) => service.translate(text, source_lang, target_lang).await,
            Self::DeepLX(service) => service.translate(text, source_lang, target_lang).await,
            Self::MyMemory(service) => service.translate(text, source_lang, target_lang).await,
        }
    }

    pub async fn translate_stream<F>(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
        on_delta: F,
    ) -> Result<(), TranslationError>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        match self {
            Self::DeepL(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::DeepLX(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::MyMemory(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
        }
    }
}

/// Create a translation provider by name using config credentials.
pub fn create_provider(
    name: &str,
    ai_config: &AiConfig,
) -> Result<TranslationService, TranslationError> {
    match name {
        "deepl" => Ok(TranslationService::DeepL(deepl::DeepLService::new(
            ai_config,
        ))),
        "deeplx" => Ok(TranslationService::DeepLX(deepl::DeepLXService::new(
            ai_config,
        ))),
        "mymemory" => Ok(TranslationService::MyMemory(
            mymemory::MyMemoryService::new(ai_config),
        )),
        _ => Err(TranslationError::ProviderNotFound(name.to_string())),
    }
}

/// Translate text using a named provider.
pub async fn translate_with_provider(
    provider_name: &str,
    ai_config: &AiConfig,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<TranslationResult, TranslationError> {
    let provider = create_provider(provider_name, ai_config)?;
    provider.translate(text, source_lang, target_lang).await
}

/// Streaming translation using a named provider.
pub async fn translate_stream_with_provider(
    provider_name: &str,
    ai_config: &AiConfig,
    text: &str,
    source_lang: &str,
    target_lang: &str,
    on_delta: impl Fn(String) + Send + Sync + 'static,
) -> Result<(), TranslationError> {
    let provider = create_provider(provider_name, ai_config)?;
    provider
        .translate_stream(text, source_lang, target_lang, on_delta)
        .await
}

/// Check if a provider has runtime configuration available.
pub fn provider_has_runtime_configuration(provider_name: &str, ai_config: &AiConfig) -> bool {
    match provider_name {
        "deepl" => !ai_config.translation_api.deepl_key.trim().is_empty(),
        "deeplx" => true, // DeepLX always available (bundled free endpoint)
        "mymemory" => true, // MyMemory always available (anonymous free endpoint)
        _ => false,
    }
}
