//! Recent calls for the Gateway column.
//!
//! The ring lives in `skillstar-gateway`. This projection copies the fields the
//! page draws and nothing else: no secret, no upstream URL. A missing usage
//! count stays an empty string rather than a zero.

use serde::{Deserialize, Serialize};
use skillstar_gateway::RecentCall;
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
}

/// The in-memory ring, oldest first. This does not open Usage or the provider store.
pub fn load_recent_calls() -> Vec<RecentCallDto> {
    skillstar_gateway::recent_calls()
        .into_iter()
        .map(project_call)
        .collect()
}

fn project_call(call: RecentCall) -> RecentCallDto {
    RecentCallDto {
        at: call.at,
        agent: call.agent,
        model: call.model,
        status: call.status,
        completion_tokens: call
            .completion_tokens
            .map(|count| count.to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_gateway::{RecentCall, clear_recent_calls, note_recent_call};

    #[test]
    fn recent_call_dto_omits_secret_and_upstream_url() {
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
        let text = serde_json::to_string(&[five.clone(), blank.clone()]).unwrap();
        assert!(text.contains("\"completion_tokens\":\"5\""), "{text}");
        assert!(text.contains("\"completion_tokens\":\"\""), "{text}");
        assert!(!text.contains("\"completion_tokens\":\"0\""), "{text}");
        assert!(!text.contains("https://"), "{text}");
        assert!(!text.contains("sk-"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");
        clear_recent_calls();
    }
}
