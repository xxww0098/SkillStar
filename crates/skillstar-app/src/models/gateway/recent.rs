//! Recent calls for the Gateway column.
//!
//! The source is the ledger view in [`super::ledger`]: the persistent
//! ledger's newest page merged with the in-memory ring, so the column
//! survives a restart. This projection keeps only the row shape the page
//! draws: no secret, no upstream URL. A usage count the response did not
//! name stays an empty string rather than a zero.

use serde::{Deserialize, Serialize};
use skillstar_gateway::LedgerQuery;
use ts_rs::TS;

/// One row of the Gateway column.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "RecentCallDto.ts")]
pub struct RecentCallDto {
    pub at: String,
    pub agent: String,
    pub model: String,
    pub status: u16,
    /// Decimal completion-token count, or empty when the response had no usage.
    pub completion_tokens: String,
    /// Decimal input-token count, or empty when the response had no usage.
    /// Ring-sourced rows carry no input count and stay empty.
    pub in_tokens: String,
    /// The session the turn is affinitized to, or empty when unknown.
    pub session: String,
    /// Decimal turn latency in milliseconds, or empty when the source has
    /// none. Ring-sourced rows stay empty.
    pub latency: String,
}

/// The newest merged page, oldest last. This does not open Usage or the
/// provider store.
pub fn load_recent_calls() -> Vec<RecentCallDto> {
    super::load_ledger_page(LedgerQuery::tail(super::PAGE_KEEP))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::{RecentCall, clear_recent_calls, note_recent_call};

    #[tokio::test(flavor = "current_thread")]
    async fn recent_call_dto_omits_secret_and_upstream_url() {
        let _lock = ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("recent-dto");
        std::fs::create_dir_all(&data).unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_TOOL_SYNC_HOME", &data),
        ]);
        clear_recent_calls();
        note_recent_call(RecentCall {
            at: "2026-09-30 12:00:00".to_string(),
            agent: "codex".to_string(),
            model: "m1".to_string(),
            status: 200,
            completion_tokens: Some(5),
        });
        note_recent_call(RecentCall {
            at: "2026-09-30 12:00:01".to_string(),
            agent: "opencode".to_string(),
            model: "m2".to_string(),
            status: 200,
            completion_tokens: None,
        });
        let calls = load_recent_calls();
        let five = calls.iter().find(|call| call.model == "m1").unwrap();
        let blank = calls.iter().find(|call| call.model == "m2").unwrap();
        assert_eq!(five.completion_tokens, "5");
        assert_eq!(blank.completion_tokens, "");
        // Ring-sourced rows: the fields only the ledger carries stay empty.
        assert_eq!(five.in_tokens, "");
        assert_eq!(five.session, "");
        assert_eq!(five.latency, "");
        let text = serde_json::to_string(&[five.clone(), blank.clone()]).unwrap();
        assert!(text.contains("\"completion_tokens\":\"5\""), "{text}");
        assert!(text.contains("\"completion_tokens\":\"\""), "{text}");
        assert!(!text.contains("\"completion_tokens\":\"0\""), "{text}");
        assert!(!text.contains("\"in_tokens\":\"0\""), "{text}");
        assert!(!text.contains("https://"), "{text}");
        assert!(!text.contains("sk-"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");
        clear_recent_calls();
    }
}
