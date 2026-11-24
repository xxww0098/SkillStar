//! Antigravity 额度解析与路由：夹具驱动纯解析，路由与分类各自成测。

use serde_json::json;

use super::*;
use crate::oauth::TokenSet;
use crate::usage::credentials::Credentials;
use crate::usage::error::UsageError;
use crate::usage::snapshot::UsageUnit;

const LOAD_ASSIST: &str =
    include_str!("../../../tests/fixtures/antigravity/load_code_assist.json");
const LOAD_ASSIST_NO_PROJECT: &str =
    include_str!("../../../tests/fixtures/antigravity/load_code_assist_no_project.json");
const QUOTA_SUMMARY: &str = include_str!("../../../tests/fixtures/antigravity/quota_summary.json");
const AVAILABLE_MODELS: &str =
    include_str!("../../../tests/fixtures/antigravity/available_models.json");

fn json_of(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("fixture must be JSON")
}

fn windows_of(body: &str) -> Vec<crate::usage::UsageWindow> {
    parse_ag::usage_windows(&parse_ag::summary_windows(&json_of(body)))
}

#[test]
fn load_code_assist_gives_the_plan_credits_and_project() {
    let load = interpret_load(200, LOAD_ASSIST).expect("fixture must parse");
    assert_eq!(load.plan, "PRO", "paidTier.id wins and keeps its case");
    assert_eq!(load.gcp_tos, Some(false));
    assert_eq!(load.project.as_deref(), Some("projects/antigravity-demo"));
    assert_eq!(load.onboard_tier.as_deref(), Some("free-tier"));

    let credits = load.credits.expect("two credited entries");
    assert_eq!(credits.remaining, Some(30.0), "25 + 5, the amount-less entry is not a zero");
    assert_eq!(credits.label, "积分", "several credit types collapse to one label");
    assert_eq!(credits.unit, UsageUnit::Count);
    assert!(!credits.unlimited);
}

#[test]
fn a_single_credit_type_names_the_credits_row() {
    let value = json!({
        "paidTier": {
            "id": "ULTRA",
            "availableCredits": [{ "creditType": "GOOGLE_ONE_AI", "creditAmount": "12.5" }]
        }
    });
    let credits = parse_ag::credits(&value).expect("one credited entry");
    assert_eq!(credits.label, "GOOGLE_ONE_AI");
    assert_eq!(credits.remaining, Some(12.5));

    let none = json!({ "paidTier": { "id": "ULTRA", "availableCredits": [] } });
    assert!(parse_ag::credits(&none).is_none(), "no credited entry means no row");
}

#[test]
fn a_gcp_tos_account_without_a_project_onboards_first() {
    let load = interpret_load(200, LOAD_ASSIST_NO_PROJECT).expect("fixture must parse");
    assert_eq!(load.project, None);
    assert_eq!(load.gcp_tos, Some(true), "currentTier.usesGcpTos is the ToS marker");
    assert_eq!(load.onboard_tier.as_deref(), Some("standard-tier"));
    assert_eq!(load.plan, "standard-tier", "currentTier.id stands in for the plan");
}

#[test]
fn project_ids_are_strings_or_named_objects() {
    for value in [
        json!("projects/alpha"),
        json!({ "id": "projects/alpha" }),
        json!({ "projectId": "projects/alpha" }),
        json!({ "project_id": "projects/alpha" }),
        json!({ "name": "projects/alpha" }),
    ] {
        assert_eq!(
            parse_ag::project_value(&value).as_deref(),
            Some("projects/alpha"),
            "{value}"
        );
    }
    assert_eq!(parse_ag::project_value(&json!("  ")), None);
    assert_eq!(
        parse_ag::project_value(&json!("aicode-consumers")),
        None,
        "the internal project is not the account's project"
    );
    assert_eq!(parse_ag::project_value(&json!(7)), None);

    // The three response slots, in priority order.
    let response = json!({
        "project": { "id": "projects/from-project" },
        "cloudaicompanionProjectId": "projects/from-id"
    });
    assert_eq!(
        parse_ag::response_project(&response).as_deref(),
        Some("projects/from-project")
    );
    let object_form = json!({ "cloudaicompanionProject": { "projectId": "projects/object" } });
    assert_eq!(
        parse_ag::response_project(&object_form).as_deref(),
        Some("projects/object")
    );
}

#[test]
fn the_plan_falls_back_through_the_tier_chain() {
    assert_eq!(
        parse_ag::plan_name(&json!({
            "paidTier": { "id": "ULTRA" },
            "currentTier": { "id": "free-tier" },
            "allowedTiers": [{ "id": "legacy-tier", "isDefault": true }]
        })),
        "ULTRA"
    );
    assert_eq!(
        parse_ag::plan_name(&json!({
            "currentTier": { "id": "g1-pro-tier" },
            "allowedTiers": [{ "id": "legacy-tier", "isDefault": true }]
        })),
        "g1-pro-tier"
    );
    assert_eq!(
        parse_ag::plan_name(&json!({
            "allowedTiers": [{ "id": "legacy-tier" }, { "id": "default-tier", "isDefault": true }]
        })),
        "default-tier"
    );
    assert_eq!(
        parse_ag::plan_name(&json!({ "allowedTiers": [{ "id": "first-tier" }] })),
        "first-tier"
    );
    assert_eq!(parse_ag::plan_name(&json!({})), "FREE");
}

#[test]
fn summary_buckets_become_percent_windows_with_grouped_labels() {
    let windows = windows_of(QUOTA_SUMMARY);
    assert_eq!(windows.len(), 3, "the fraction-less hint bucket is not a window");

    let five_hour = &windows[0];
    assert_eq!(five_hour.key, "gemini-5h");
    assert_eq!(five_hour.label, "Gemini Models · 5h", "the suffix is not a name");
    assert_eq!(five_hour.remaining_percent, 75.0);
    assert_eq!(five_hour.used, Some(25.0), "remainingFraction is what is left");
    assert_eq!(five_hour.total, Some(100.0));
    assert_eq!(five_hour.unit, UsageUnit::Percent);
    assert_eq!(five_hour.resets_at, Some(1_767_243_600), "2026-01-01T05:00:00Z");

    assert_eq!(windows[1].label, "Gemini Models · Weekly");
    assert_eq!(windows[1].remaining_percent, 40.0);

    assert_eq!(windows[2].label, "Claude and GPT models · 5h");
    assert_eq!(windows[2].remaining_percent, 25.0, "\"25%\" is a fraction too");
    assert_eq!(windows[2].resets_at, None);
}

#[test]
fn summary_buckets_strip_limit_suffix_and_compact_period_names() {
    let payload = json!({
        "groups": [
            {
                "displayName": "Gemini Models",
                "buckets": [
                    { "bucketId": "gemini-weekly", "displayName": "Weekly Limit", "remainingFraction": 0.63 },
                    { "bucketId": "gemini-5h", "displayName": "Five Hour Limit", "remainingFraction": 0.84 }
                ]
            },
            {
                "displayName": "Claude and GPT models",
                "buckets": [
                    { "bucketId": "claude-weekly", "displayName": "Weekly Limit", "remainingFraction": 0.99 },
                    { "bucketId": "claude-5h", "displayName": "Five Hour Limit", "remainingFraction": 1.0 }
                ]
            }
        ]
    });
    let windows = parse_ag::usage_windows(&parse_ag::summary_windows(&payload));
    let labels: Vec<&str> = windows.iter().map(|window| window.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Gemini Models · Weekly",
            "Gemini Models · 5h",
            "Claude and GPT models · Weekly",
            "Claude and GPT models · 5h",
        ]
    );
}

#[test]
fn model_rows_need_a_displayable_name_and_a_fraction() {
    let windows = parse_ag::usage_windows(&parse_ag::model_windows(&json_of(AVAILABLE_MODELS)));
    // serde_json keeps object keys sorted, so the rows arrive in model-id order.
    let keys: Vec<&str> = windows.iter().map(|window| window.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "claude-sonnet-4-6",
            "gemini-3-pro-high",
            "gpt-oss-120b-medium",
            "imagen-4"
        ],
        "chat_editor is not a model and a reset time alone is not a quota"
    );
    let by_key = |key: &str| {
        windows
            .iter()
            .find(|window| window.key == key)
            .expect("window")
    };

    let gemini = by_key("gemini-3-pro-high");
    assert_eq!(gemini.label, "Gemini 3 Pro (high)");
    assert_eq!(gemini.remaining_percent, 50.0);
    assert_eq!(gemini.used, Some(50.0));
    assert_eq!(gemini.resets_at, Some(1_767_243_600));

    assert_eq!(
        by_key("claude-sonnet-4-6").remaining_percent,
        13.0,
        "0.125 rounds to 13% remaining"
    );
    assert_eq!(
        by_key("gpt-oss-120b-medium").label, "gpt-oss-120b-medium",
        "no display name means the model id is the label"
    );
    assert_eq!(by_key("gpt-oss-120b-medium").remaining_percent, 100.0);
    assert_eq!(by_key("imagen-4").label, "Imagen 4");
}

#[test]
fn routing_prefers_the_override_then_prod_only_for_a_project_account() {
    assert_eq!(
        select_base_url(Some(" https://proxy.example/ "), true, Some("projects/alpha")),
        "https://proxy.example"
    );
    assert_eq!(
        select_base_url(Some("   "), true, Some("projects/alpha")),
        PROD_BASE_URL
    );
    assert_eq!(select_base_url(None, true, Some("projects/alpha")), PROD_BASE_URL);
    assert_eq!(
        select_base_url(None, true, None),
        DAILY_BASE_URL,
        "a GCP ToS account without a project still reads from daily"
    );
    assert_eq!(select_base_url(None, false, Some("projects/alpha")), DAILY_BASE_URL);
}

#[test]
fn load_failures_use_the_shared_classification() {
    assert!(matches!(
        interpret_load(401, ""),
        Err(UsageError::AuthRequired(_))
    ));
    for status in [429, 500, 502, 503] {
        let error = interpret_load(status, "").expect_err("must fail");
        assert!(error.is_transient(), "{status} must stay retryable");
    }
    let rejected = interpret_load(400, "bad request").expect_err("must fail");
    assert!(matches!(rejected, UsageError::Unreadable(_)), "{rejected:?}");
    assert!(is_bad_request(&rejected));

    let missing = interpret_load(404, "no such method").expect_err("must fail");
    assert!(is_endpoint_missing(&missing), "{missing:?}");

    let junk = interpret_load(200, "<html></html>").expect_err("HTML is not loadCodeAssist");
    assert!(matches!(junk, UsageError::Unreadable(_)));
    assert!(!junk.is_transient());
}

#[test]
fn only_a_bad_request_clears_the_cached_project() {
    assert!(is_bad_request(&UsageError::Unreadable(
        "antigravity loadCodeAssist: HTTP 400 {\"error\":\"Bad Request\"}".to_string()
    )));
    assert!(!is_bad_request(&UsageError::Transient("boom".to_string())));
    assert!(!is_bad_request(&UsageError::AuthRequired("gone".to_string())));
    assert!(!is_bad_request(&UsageError::Unreadable(
        "antigravity loadCodeAssist: HTTP 403 forbidden".to_string()
    )));
}

#[test]
fn the_metadata_reports_the_ide_identity() {
    let identity = metadata(Some("projects/alpha"));
    assert_eq!(identity["ideName"], json!("antigravity"));
    assert_eq!(identity["ideType"], json!("ANTIGRAVITY"));
    assert_eq!(identity["ideVersion"], json!(IDE_VERSION));
    assert_eq!(identity["pluginVersion"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(identity["updateChannel"], json!("stable"));
    assert_eq!(identity["pluginType"], json!("GEMINI"));
    assert_eq!(identity["duetProject"], json!("projects/alpha"));
    assert!(
        identity["platform"]
            .as_str()
            .expect("platform is a string")
            .contains('_'),
        "{identity}"
    );
    assert!(metadata(None).get("duetProject").is_none());

    let body = load_code_assist_body(Some("projects/alpha"));
    assert_eq!(body["mode"], json!("FULL_ELIGIBILITY_CHECK"));
    assert_eq!(body["cloudaicompanionProject"], json!("projects/alpha"));
    assert!(load_code_assist_body(None).get("cloudaicompanionProject").is_none());
}

#[test]
fn provider_state_caches_the_project_and_the_tos_flag() {
    let state = provider_state(Some("projects/alpha"), Some(true));
    assert_eq!(cached_project(&state).as_deref(), Some("projects/alpha"));
    assert_eq!(cached_gcp_tos(&state), Some(true));

    let without_tos = provider_state(Some("projects/alpha"), Some(false));
    assert_eq!(without_tos, json!({ "project_id": "projects/alpha" }));

    let empty = provider_state(None, None);
    assert_eq!(empty, json!({}), "an empty state clears a stale project");
    assert_eq!(cached_project(&empty), None);
}

#[test]
fn onboard_user_steps_are_read_to_the_operation_name_and_the_project() {
    assert_eq!(
        onboard_step(&json!({ "name": "operations/onboard-1", "done": false })).expect("pending"),
        OnboardStep::Pending("operations/onboard-1".to_string())
    );
    assert_eq!(
        onboard_step(&json!({
            "done": true,
            "response": { "cloudaicompanionProject": { "id": "projects/onboarded" } }
        }))
        .expect("done"),
        OnboardStep::Done(Some("projects/onboarded".to_string()))
    );
    assert_eq!(
        onboard_step(&json!({ "done": true })).expect("done"),
        OnboardStep::Done(None)
    );
    let error = onboard_step(&json!({ "done": false })).expect_err("no operation name");
    assert!(matches!(error, UsageError::Unreadable(_)), "{error:?}");
}

#[test]
fn an_expired_access_token_is_never_traded_for_a_new_one() {
    let creds = Credentials {
        tokens: TokenSet {
            access_token: "at-expired".to_string(),
            refresh_token: Some("rt-1".to_string()),
            expires_at: Some(super::super::unix_now() - 60),
            token_type: "Bearer".to_string(),
            ..TokenSet::default()
        },
        account_id: None,
        provider_state: None,
        needs_identity: false,
    };
    let error = fetch(&creds).expect_err("no quota read without a live token");
    // 不是 AuthRequired：凭据还在，只是这里不兑换 refresh token。
    assert!(matches!(error, UsageError::Unreadable(_)), "{error:?}");
    assert!(error.to_string().contains("已过期"), "{error}");
}

#[test]
fn quota_read_enriches_missing_identity_without_making_it_required() {
    use std::io::{BufRead, BufReader, Read, Write};

    let _sandbox = crate::test_support::Sandbox::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("local server");
    unsafe {
        std::env::set_var(BASE_URL_ENV, format!("http://{}", listener.local_addr().unwrap()))
    };
    let server = std::thread::spawn(move || {
        for _ in 0..6 {
            let (stream, _) = listener.accept().expect("request");
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let body = if line.contains(LOAD_CODE_ASSIST_PATH) {
                LOAD_ASSIST
            } else {
                assert!(line.contains(QUOTA_SUMMARY_PATH), "{line}");
                QUOTA_SUMMARY
            };
            let mut content_length = 0;
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap();
                }
            }
            reader.read_exact(&mut vec![0; content_length]).unwrap();
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            )
            .unwrap();
        }
    });
    let mut creds = Credentials {
        tokens: TokenSet {
            access_token: "at-test".to_string(),
            ..TokenSet::default()
        },
        account_id: None,
        provider_state: None,
        needs_identity: true,
    };
    let result = fetch_with_identity(&creds, |token| {
        assert_eq!(token, "at-test");
        crate::oauth::agents::antigravity::identity_from_userinfo(&json!({
            "name": "Google User", "email": "user@example.com"
        }))
        .ok()
    })
    .expect("quota and identity");
    assert_eq!(
        result.identity.unwrap().display_name.as_deref(),
        Some("Google User")
    );
    assert!(!result.snapshot.windows.is_empty());

    creds.needs_identity = false;
    let known = fetch_with_identity(&creds, |_| panic!("identity is already known"))
        .expect("quota without lookup");
    assert!(known.identity.is_none());

    creds.needs_identity = true;
    let unavailable = fetch_with_identity(&creds, |_| None).expect("quota survives failed userinfo");
    assert!(unavailable.identity.is_none());
    assert!(!unavailable.snapshot.windows.is_empty());
    server.join().expect("server");
}

#[test]
fn the_user_agent_names_the_ide_platform_and_arch() {
    let agent = user_agent();
    assert!(agent.starts_with(&format!("antigravity/{IDE_VERSION} ")), "{agent}");
    assert!(agent.contains('/'), "{agent}");
    assert_eq!(headers("at-1").len(), 4);
    assert_eq!(headers("at-1")[0].1, "Bearer at-1");
}

#[test]
fn a_junk_summary_body_is_unreadable_not_transient() {
    let summary = parse_ag::summary_windows(&json_of(r#"{"groups":"not a list"}"#));
    assert!(summary.is_empty());
    let models = parse_ag::model_windows(&json_of(r#"{"models":[1,2]}"#));
    assert!(models.is_empty());
}
