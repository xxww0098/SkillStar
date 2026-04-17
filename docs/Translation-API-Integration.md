# Translation API Integration Plan (rockbenben parity)

## Overview

Replace the current AI-only translation layer with a unified translation service
supporting 5 traditional APIs and 9+ AI LLM providers — matching
[md-translator](https://github.com/rockbenben/md-translator) feature parity.

## Current State

| Feature | Current Implementation |
|---------|----------------------|
| SKILL.md translation | `markdown-translator` crate (AST/segment LLM pipeline via `mdtx_bridge`) |
| Short text translation | `ai_provider` module (AI LLM + MyMemory fallback) |
| API keys | Encrypted in `~/.skillstar/config/ai.json` via `AiConfig` |
| Config struct | `core/ai_provider/config.rs` — `AiConfig` |
| Translation commands | `commands/ai/translate.rs` |

## Target State

```
TranslationService trait
├── Traditional APIs (non-LLM)
│   ├── DeepL (deepl-node / DeepLX free endpoint)
│   ├── Google Translate (REST v2)
│   ├── Azure Translate (REST v3)
│   ├── GTX API (Google Translate AJAX free)
│   └── GTX Web (server-side Google Translate proxy)
│
└── AI LLM Providers
    ├── DeepSeek
    ├── Claude (Anthropic Messages API)
    ├── OpenAI (Chat Completions)
    ├── Gemini (GenerateContent)
    ├── Perplexity
    ├── Azure OpenAI
    ├── SiliconFlow
    ├── Groq
    ├── OpenRouter
    ├── Nvidia NIM
    └── Custom LLM (OpenAI-compatible)
```

## Phase 1 — Config & Types

### File: `src-tauri/src/core/translation_api/mod.rs` (new)

```rust
// TranslationService enum — one variant per API
pub enum TranslationService {
    DeepL(DeepLConfig),
    DeepLX(DeepLXConfig),
    GoogleTranslate(GoogleConfig),
    AzureTranslate(AzureConfig),
    GtxFreeAPI(GtxConfig),   // Google AJAX — no key needed
    GtxWebProxy(GtxConfig),  // server-side proxy — no key needed
    // LLM providers
    DeepSeek(OpenAICompatConfig),
    OpenAI(OpenAICompatConfig),
    Claude(ClaudeConfig),
    Gemini(GeminiConfig),
    Perplexity(OpenAICompatConfig),
    AzureOpenAI(AzureOpenAIConfig),
    SiliconFlow(OpenAICompatConfig),
    Groq(OpenAICompatConfig),
    OpenRouter(OpenAICompatConfig),
    NvidiaNIM(NvidiaNIMConfig),
    CustomLLM(CustomLLMConfig),
}
```

### File: `src-tauri/src/core/ai_provider/config.rs`

Add `translation_api: TranslationApiConfig` to `AiConfig`. Structure:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationApiConfig {
    // Per-API keys (encrypted at rest like existing api_key)
    pub deepl_key: String,
    pub deeplx_url: String,       // free endpoint override
    pub google_key: String,
    pub azure_key: String,
    pub azure_region: String,
    pub deepseek_key: String,
    pub claude_key: String,
    pub openai_key: String,
    pub gemini_key: String,
    pub perplexity_key: String,
    pub azure_openai_key: String,
    pub azure_openai_url: String,
    pub siliconflow_key: String,
    pub groq_key: String,
    pub openrouter_key: String,
    pub nvidia_key: String,
    pub custom_llm_url: String,
    pub custom_llm_key: String,

    // Default provider selection
    pub default_short_text_provider: String,  // e.g. "deepl"
    pub default_skill_provider: String,        // e.g. "deepseek"
}
```

## Phase 2 — Service Implementations

### File: `src-tauri/src/core/translation_api/services/mod.rs`

```rust
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<String>> + Send;

    // Optional streaming
    fn translate_stream(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
        on_delta: impl Fn(String) + Send + Sync,
    ) -> impl Future<Output = Result<()>> + Send {
        // Default: non-streaming, call translate() and emit once
        async {}
    }
}
```

### Individual service files

```
services/
├── mod.rs              // registry + factory
├── deepl.rs            // DeepL API + DeepLX free endpoint
├── google.rs           // Google Translate v2
├── azure.rs            // Azure Translator v3
├── gtx.rs              // GTX free AJAX + web proxy
├── deepseek.rs         // DeepSeek Chat
├── openai_compat.rs    // OpenAI, Perplexity, SiliconFlow, Groq, OpenRouter
├── claude.rs           // Anthropic Messages API
├── gemini.rs           // Google Gemini GenerateContent
├── azure_openai.rs     // Azure OpenAI Deployment
├── nvidia.rs           // Nvidia NIM
└── custom_llm.rs       // User-supplied OpenAI-compatible endpoint
```

## Phase 3 — Service Factory & Registry

```rust
// services/mod.rs
pub struct TranslationRegistry;

impl TranslationRegistry {
    pub fn create(name: &str, config: &TranslationApiConfig) -> Result<Arc<dyn TranslationProvider>>;
}

pub async fn translate_text(
    provider_name: &str,
    config: &TranslationApiConfig,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String> {
    let svc = TranslationRegistry::create(provider_name, config)?;
    svc.translate(text, source_lang, target_lang).await
}
```

## Phase 4 — Tauri Commands (replacing `commands/ai/translate.rs`)

```rust
#[tauri::command]
pub async fn translate_short_text(
    text: String,
    source_lang: Option<String>,   // default "auto"
    target_lang: Option<String>,   // default from ai_config
    provider: Option<String>,       // override default provider
) -> Result<ShortTextTranslationPayload, String>;

#[tauri::command]
pub async fn translate_skill(
    content: String,
    force_refresh: Option<bool>,
    provider: Option<String>,
) -> Result<String, String>;

#[tauri::command]
pub async fn translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    force_refresh: Option<bool>,
    provider: Option<String>,
) -> Result<String, String>;

#[tauri::command]
pub async fn translate_generic(
    text: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
    provider: Option<String>,
    stream: Option<bool>,
) -> Result<String, String>;
```

## Phase 5 — Frontend Changes

### `src/types/` (shared TypeScript types)
```typescript
interface TranslationApiConfig {
  deepl_key: string;
  deeplx_url: string;
  google_key: string;
  azure_key: string;
  azure_region: string;
  deepseek_key: string;
  claude_key: string;
  openai_key: string;
  gemini_key: string;
  perplexity_key: string;
  azure_openai_key: string;
  azure_openai_url: string;
  siliconflow_key: string;
  groq_key: string;
  openrouter_key: string;
  nvidia_key: string;
  custom_llm_url: string;
  custom_llm_key: string;
  default_short_text_provider: string;
  default_skill_provider: string;
}
```

### Settings UI (`src/features/settings/`)

- New "Translation APIs" section
- Per-provider API key inputs (password masked)
- Default provider selectors for short text and SKILL.md
- Per-translation override in My Skills / Marketplace modals

## Phase 6 — Integration with Existing Code

### Short text translation (`commands/ai/translate.rs`)
- Keep `ai_translate_short_text_stream_with_source` 
- Replace internal `ai_provider::translate_short_text_streaming_*` with `TranslationRegistry`
- MyMemory stays as additional fallback option

### SKILL.md translation (`commands/ai/translate.rs` + `mdtx_bridge.rs`)
- `ai_translate_skill` / `ai_translate_skill_stream` — add `provider` param
- If provider is LLM → use existing `mdtx_bridge` pipeline (segment-based, cache, validation)
- If provider is traditional API → call `TranslationRegistry` directly (no markdown pipeline needed)
- Traditional APIs don't need the markdown-translator crate for SKILL.md; they translate raw text

### Note on `markdown-translator` crate
- The crate stays for its **markdown-aware LLM pipeline** (segment parsing, placeholder protection, format validation)
- It is NOT used for traditional API calls
- It is triggered when the user selects an **LLM provider** (deepseek, openai, etc.)

## Key Design Decisions

1. **Per-request provider selection** — not global; user picks provider per translation
2. **API keys encrypted** — same AES-256-GCM pattern as existing `api_key` in `AiConfig`
3. **Traditional APIs bypass markdown-translator** — they translate raw text directly; markdown structure is preserved by nature of being plain text translation
4. **Streaming** — only for LLM providers and APIs that support it (DeepL Pro, Google, etc.)
5. **Cache** — existing SQLite cache (`~/.skillstar/db/translation.db`) keyed by `provider + text_hash + target_lang`

## File Map (changes)

| File | Action |
|------|--------|
| `src-tauri/src/core/translation_api/mod.rs` | NEW — service registry + types |
| `src-tauri/src/core/translation_api/services/mod.rs` | NEW — factory + trait |
| `src-tauri/src/core/translation_api/services/deepl.rs` | NEW |
| `src-tauri/src/core/translation_api/services/google.rs` | NEW |
| `src-tauri/src/core/translation_api/services/azure.rs` | NEW |
| `src-tauri/src/core/translation_api/services/gtx.rs` | NEW |
| `src-tauri/src/core/translation_api/services/deepseek.rs` | NEW |
| `src-tauri/src/core/translation_api/services/openai_compat.rs` | NEW |
| `src-tauri/src/core/translation_api/services/claude.rs` | NEW |
| `src-tauri/src/core/translation_api/services/gemini.rs` | NEW |
| `src-tauri/src/core/translation_api/services/azure_openai.rs` | NEW |
| `src-tauri/src/core/translation_api/services/nvidia.rs` | NEW |
| `src-tauri/src/core/translation_api/services/custom_llm.rs` | NEW |
| `src-tauri/src/core/ai_provider/config.rs` | MODIFY — add `TranslationApiConfig` to `AiConfig` |
| `src-tauri/src/core/ai_provider/mod.rs` | MODIFY — import translation_api |
| `src-tauri/src/commands/ai/translate.rs` | MODIFY — wire new services |
| `src-tauri/src/lib.rs` | MODIFY — add `translation_api` module |
| `src/types/` | MODIFY — add `TranslationApiConfig` TypeScript type |
| `src/features/settings/` | MODIFY — add Translation API settings section |
| `AGENTS.md` | MODIFY — update AI Integration section |
