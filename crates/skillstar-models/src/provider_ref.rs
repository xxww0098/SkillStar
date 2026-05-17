//! Reference type pointing from `AiConfig` into the model provider store.
//!
//! `AiConfig` itself lives in `skillstar-ai` because it carries inference-time
//! state (api_format / model / preset etc.), but its provider pointer belongs
//! to the models domain, so we keep the type here and re-export it from
//! `skillstar-ai` for backward compatibility.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiProviderRef {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub provider_id: String,
}
