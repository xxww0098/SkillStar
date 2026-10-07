//! Shared serialization for MCP tests that redirect process-wide storage roots.
//! Each fixture saves/restores its own values while holding this lock.

pub(crate) static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
