//! OAuth login kickoff metadata returned to the desktop shell.
//!
//! `flow` is how this login finishes. The dialog branches on it, not on the
//! catalog id. There is no session-expiry field here: a poll countdown, when
//! one exists, is `interval_secs` only.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How the user finishes a login that [`OAuthStartInfo`] just started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "OAuthFlow.ts")]
pub enum OAuthFlow {
    /// Loopback listener is up. A pasted callback is replayed over HTTP.
    LocalCallback,
    /// The backend polls the provider. The user has nothing to paste.
    RemotePoll,
    /// Custom-scheme callback, parsed in-process. Not replayed over HTTP.
    SchemePaste { scheme_prefix: String },
    /// Already finished in place (local credential adoption).
    Immediate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthStartInfo {
    pub auth_url: String,
    pub pending_id: String,
    pub flow: OAuthFlow,
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    pub interval_secs: Option<u32>,
}

impl OAuthStartInfo {
    pub fn browser(auth_url: String, pending_id: String) -> Self {
        Self::bare(auth_url, pending_id, OAuthFlow::LocalCallback)
    }

    /// Device-code login. `verification_uri` is also `auth_url`, so a caller
    /// that only knows how to open the link still lands on the right page.
    pub fn device(
        pending_id: String,
        verification_uri: String,
        user_code: String,
        interval_secs: Option<u32>,
    ) -> Self {
        Self {
            auth_url: verification_uri.clone(),
            pending_id,
            flow: OAuthFlow::RemotePoll,
            user_code: Some(user_code),
            verification_uri: Some(verification_uri),
            interval_secs,
        }
    }

    /// Backend poll with nothing for the user to paste. `auth_url` may still
    /// be a page to open; there is no user code.
    pub fn remote_poll(auth_url: String, pending_id: String, interval_secs: Option<u32>) -> Self {
        let mut info = Self::bare(auth_url, pending_id, OAuthFlow::RemotePoll);
        info.interval_secs = interval_secs;
        info
    }

    pub fn scheme_paste(auth_url: String, pending_id: String, scheme_prefix: String) -> Self {
        Self::bare(
            auth_url,
            pending_id,
            OAuthFlow::SchemePaste { scheme_prefix },
        )
    }

    pub fn immediate(auth_url: String, pending_id: String) -> Self {
        Self::bare(auth_url, pending_id, OAuthFlow::Immediate)
    }

    fn bare(auth_url: String, pending_id: String, flow: OAuthFlow) -> Self {
        Self {
            auth_url,
            pending_id,
            flow,
            user_code: None,
            verification_uri: None,
            interval_secs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_stays_local_callback() {
        let info = OAuthStartInfo::browser("https://auth.example".into(), "p1".into());
        assert_eq!(info.flow, OAuthFlow::LocalCallback);
        assert_eq!(info.user_code, None);
        assert_eq!(info.verification_uri, None);
        assert_eq!(info.interval_secs, None);
    }

    #[test]
    fn device_is_remote_poll_with_a_user_code() {
        let info = OAuthStartInfo::device(
            "p2".into(),
            "https://example.test/device".into(),
            "ABCD-EFGH".into(),
            Some(5),
        );
        assert_eq!(info.flow, OAuthFlow::RemotePoll);
        assert_eq!(info.auth_url, "https://example.test/device");
        assert_eq!(info.user_code.as_deref(), Some("ABCD-EFGH"));
        assert_eq!(
            info.verification_uri.as_deref(),
            Some("https://example.test/device")
        );
        assert_eq!(info.interval_secs, Some(5));
    }

    #[test]
    fn remote_poll_has_no_user_code() {
        let info =
            OAuthStartInfo::remote_poll("https://example.test/poll".into(), "p3".into(), Some(2));
        assert_eq!(info.flow, OAuthFlow::RemotePoll);
        assert_eq!(info.user_code, None);
        assert_eq!(info.verification_uri, None);
        assert_eq!(info.interval_secs, Some(2));
    }

    #[test]
    fn scheme_paste_records_the_prefix() {
        let info =
            OAuthStartInfo::scheme_paste("zcode://login".into(), "p4".into(), "zcode://".into());
        assert_eq!(
            info.flow,
            OAuthFlow::SchemePaste {
                scheme_prefix: "zcode://".into()
            }
        );
    }

    #[test]
    fn immediate_carries_no_paste_fields() {
        let info = OAuthStartInfo::immediate("https://claude.ai".into(), "p5".into());
        assert_eq!(info.flow, OAuthFlow::Immediate);
        assert_eq!(info.user_code, None);
        assert_eq!(info.verification_uri, None);
        assert_eq!(info.interval_secs, None);
    }

    #[test]
    fn flow_serializes_as_kebab_case_strings() {
        assert_eq!(
            serde_json::to_value(OAuthFlow::LocalCallback).unwrap(),
            serde_json::json!("local-callback")
        );
        assert_eq!(
            serde_json::to_value(OAuthFlow::RemotePoll).unwrap(),
            serde_json::json!("remote-poll")
        );
        assert_eq!(
            serde_json::to_value(OAuthFlow::Immediate).unwrap(),
            serde_json::json!("immediate")
        );
        assert_eq!(
            serde_json::to_value(OAuthFlow::SchemePaste {
                scheme_prefix: "zcode://".into()
            })
            .unwrap(),
            serde_json::json!({ "scheme-paste": { "scheme_prefix": "zcode://" } })
        );
    }
}
