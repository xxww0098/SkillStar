//! Portable Tauri command handlers for SkillStar.
//!
//! This crate contains self-contained `#[tauri::command]` handlers that do not
//! depend on Tauri-specific app state or complex async runtime coupling.

pub mod acp;
pub mod agents;
pub mod launch;
pub mod marketplace;
pub mod network;
pub mod projects;
pub mod shell;
