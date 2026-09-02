//! Closed Block JSON vocabulary for Guides and Drafts.

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;

pub const BLOCK_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalloutTone {
    Info,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GuideBlock {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    List { ordered: bool, items: Vec<String> },
    Code { language: String, code: String },
    Callout { tone: CalloutTone, text: String },
}

impl GuideBlock {
    pub fn verified(self) -> Result<Self, AppError> {
        match &self {
            Self::Heading { level, text } => {
                if !(1..=3).contains(level) {
                    return Err(block_error(format!(
                        "heading level {level} is outside the closed set 1..=3"
                    )));
                }
                reject_empty("heading", text)?;
            }
            Self::Paragraph { text } => reject_empty("paragraph", text)?,
            Self::List { items, .. } => {
                if items.is_empty() {
                    return Err(block_error("list must contain at least one item"));
                }
                for item in items {
                    reject_empty("list item", item)?;
                }
            }
            Self::Code { code, .. } => reject_empty("code", code)?,
            Self::Callout { text, .. } => reject_empty("callout", text)?,
        }
        Ok(self)
    }
}

pub fn verify_blocks(blocks: Vec<GuideBlock>) -> Result<Vec<GuideBlock>, AppError> {
    if blocks.is_empty() {
        return Err(block_error("a step must contain at least one block"));
    }
    blocks.into_iter().map(GuideBlock::verified).collect()
}

fn reject_empty(kind: &str, text: &str) -> Result<(), AppError> {
    if text.trim().is_empty() {
        return Err(block_error(format!("{kind} text is empty")));
    }
    if text.contains('\0') {
        return Err(block_error(format!("{kind} text contains a NUL byte")));
    }
    Ok(())
}

fn block_error(message: impl Into<String>) -> AppError {
    AppError::Other(message.into())
}
