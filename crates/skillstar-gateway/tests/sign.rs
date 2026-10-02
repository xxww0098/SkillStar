//! Header fixtures copied from the magpie test named beside each case.
//! A header that test does not read is not set here.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use skillstar_gateway::{
    AccountBook, AccountSnapshot, AllowanceSnapshot, ProviderSnapshot, RouteCandidate, SignInput,
    SignedUpstream, route_smart, sign_upstream,
};

struct Fixed {
    account: AccountSnapshot,
    used: Option<f64>,
}

impl AccountBook for Fixed {
    fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
        Some(self.account.clone())
    }

    fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
        self.used.map(|used| AllowanceSnapshot {
            percent: used,
            renews_at: None,
        })
    }
}

fn token(access_token: &str) -> Fixed {
    Fixed {
        account: AccountSnapshot {
            access_token: Some(access_token.to_string()),
            ..AccountSnapshot::default()
        },
        used: Some(10.0),
    }
}

fn sign(book: &impl AccountBook, catalog_id: &str, body: &[u8]) -> (SignedUpstream, usize) {
    let asked = AtomicUsize::new(0);
    let mut quota = |_url: &str| {
        asked.fetch_add(1, Ordering::SeqCst);
    };
    let signed = sign_upstream(
        book,
        &SignInput {
            catalog_id,
            provider: None,
            body,
        },
        &mut quota,
    );
    (signed, asked.load(Ordering::SeqCst))
}

#[test]
fn sign_codex_sends_bearer_without_an_account_header() {
    let book = Fixed {
        account: AccountSnapshot {
            access_token: Some("fresh-old".to_string()),
            account_id: Some("acct-old".to_string()),
            ..AccountSnapshot::default()
        },
        used: Some(12.0),
    };
    let (signed, asked) = sign(&book, "codex", b"{}");
    assert_eq!(asked, 0);
    assert!(!signed.bridge);
    assert_eq!(
        signed.allowance,
        Some(AllowanceSnapshot {
            percent: 12.0,
            renews_at: None
        })
    );
    assert_eq!(
        signed.headers,
        vec![
            ("Authorization".to_string(), "Bearer fresh-old".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ]
    );
}

#[test]
fn sign_github_copilot_sets_session_headers() {
    // internal/provider/account_test.go TestCopilotSignAndModels
    let (signed, asked) = sign(&token("sess"), "github-copilot", b"{}");
    assert_eq!(asked, 0);
    assert_eq!(
        signed.headers,
        vec![
            ("Authorization".to_string(), "Bearer sess".to_string()),
            (
                "Copilot-Integration-Id".to_string(),
                "vscode-chat".to_string()
            ),
        ]
    );
}

#[test]
fn sign_cursor_does_not_modify_cursor_rs() {
    // internal/gateway/cursor_test.go TestServeCursor
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../skillstar-usage/src/fetchers/oauth/cursor.rs");
    let before = fs::read(&path).expect("cursor.rs stays readable");
    let (signed, asked) = sign(&token("tok"), "cursor", b"{}");
    let after = fs::read(&path).expect("cursor.rs stays readable");
    assert_eq!(before, after);
    assert_eq!(asked, 0);
    assert_eq!(
        signed.headers,
        vec![
            ("Authorization".to_string(), "Bearer tok".to_string()),
            ("x-cursor-client-type".to_string(), "cli".to_string()),
        ]
    );
}

#[test]
fn sign_xai_sets_grok_headers() {
    // internal/provider/grok_test.go TestGrokSigns
    let body = br#"{"model":"grok-4.7","prompt_cache_key":"c1"}"#;
    let (signed, asked) = sign(&token("k-me@x.ai"), "xai", body);
    assert_eq!(asked, 0);
    assert_eq!(
        signed.headers,
        vec![
            (
                "Authorization".to_string(),
                "Bearer k-me@x.ai".to_string()
            ),
            ("x-grok-client-version".to_string(), "1.0.41".to_string()),
            (
                "x-grok-model-override".to_string(),
                "grok-4.7".to_string()
            ),
            ("x-grok-conv-id".to_string(), "c1".to_string()),
            ("User-Agent".to_string(), "grok-shell/1.0.41".to_string()),
        ]
    );
}

#[test]
fn sign_kiro_sets_bearer_only() {
    // internal/gateway/kiro_test.go — the generate request is `Bearer old`.
    // The refresh that test uses to obtain `new` is not repeated.
    let (signed, asked) = sign(&token("old"), "kiro", b"{}");
    assert_eq!(asked, 0);
    assert_eq!(
        signed.headers,
        vec![("Authorization".to_string(), "Bearer old".to_string())]
    );
}

#[test]
fn sign_zcode_sets_its_own_key() {
    // internal/provider/zcode_test.go
    let book = Fixed {
        account: AccountSnapshot {
            api_key: Some("two.secret2".to_string()),
            access_token: Some("not-the-key".to_string()),
            ..AccountSnapshot::default()
        },
        used: None,
    };
    let (signed, asked) = sign(&book, "zcode", b"{}");
    assert_eq!(asked, 0);
    assert_eq!(
        signed.headers,
        vec![
            ("x-api-key".to_string(), "two.secret2".to_string()),
            (
                "Authorization".to_string(),
                "Bearer two.secret2".to_string()
            ),
        ]
    );
}

#[test]
fn sign_antigravity_sets_hub_headers() {
    // internal/provider/google_test.go
    let (signed, asked) = sign(&token("tok"), "antigravity", b"{}");
    assert_eq!(asked, 0);
    let user_agent = format!(
        "antigravity/hub/3.1.4 {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    assert!(user_agent.starts_with("antigravity/hub/3.1.4 "));
    assert_eq!(
        signed.headers,
        vec![
            ("Authorization".to_string(), "Bearer tok".to_string()),
            ("User-Agent".to_string(), user_agent),
        ]
    );
}

#[test]
fn sign_anthropic_does_not_open_http() {
    let book = Fixed {
        account: AccountSnapshot {
            access_token: Some("sk-ant-should-not-leak".to_string()),
            ..AccountSnapshot::default()
        },
        used: Some(80.0),
    };
    let (signed, asked) = sign(&book, "anthropic", b"{}");
    assert_eq!(asked, 0);
    assert!(signed.bridge);
    assert!(signed.headers.is_empty());
    assert!(signed.allowance.is_none());
    let rendered = format!("{:?}", signed.headers);
    assert!(!rendered.contains("sk-ant-should-not-leak"));
}

#[test]
fn sign_missing_snapshot_makes_no_quota_request() {
    struct Missing;
    impl AccountBook for Missing {
        fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
            None
        }

        fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
            Some(AllowanceSnapshot {
                percent: 100.0,
                renews_at: None,
            })
        }
    }

    let (signed, asked) = sign(&Missing, "codex", b"{}");
    assert_eq!(asked, 0);
    assert!(signed.headers.is_empty());
    assert!(signed.allowance.is_none());
    let order = route_smart(&[
        RouteCandidate {
            id: "room",
            allowance: Some(AllowanceSnapshot {
                percent: 1.0,
                renews_at: None,
            }),
        },
        RouteCandidate {
            id: "codex",
            allowance: signed.allowance,
        },
        RouteCandidate {
            id: "spent",
            allowance: Some(AllowanceSnapshot {
                percent: 99.0,
                renews_at: None,
            }),
        },
    ]);
    assert_eq!(
        order,
        vec![
            "room".to_string(),
            "codex".to_string(),
            "spent".to_string()
        ]
    );
}

#[test]
fn sign_gemini_cli_has_no_account_path() {
    struct StoredSecret;
    impl AccountBook for StoredSecret {
        fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
            Some(AccountSnapshot {
                access_token: Some("account-secret".to_string()),
                ..AccountSnapshot::default()
            })
        }

        fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
            Some(AllowanceSnapshot {
                percent: 3.0,
                renews_at: None,
            })
        }
    }

    let provider = ProviderSnapshot {
        api_key: Some("sk-provider".to_string()),
    };
    for catalog_id in ["gemini-cli", "gemini", "devin", "workbuddy", "commandcode"] {
        let asked = AtomicUsize::new(0);
        let mut quota = |_url: &str| {
            asked.fetch_add(1, Ordering::SeqCst);
        };
        let signed = sign_upstream(
            &StoredSecret,
            &SignInput {
                catalog_id,
                provider: Some(&provider),
                body: b"{}",
            },
            &mut quota,
        );
        assert_eq!(asked.load(Ordering::SeqCst), 0, "{catalog_id}");
        assert!(!signed.bridge, "{catalog_id}");
        assert!(signed.allowance.is_none(), "{catalog_id}");
        assert_eq!(
            signed.headers,
            vec![(
                "Authorization".to_string(),
                "Bearer sk-provider".to_string()
            )],
            "{catalog_id}"
        );
    }
}
