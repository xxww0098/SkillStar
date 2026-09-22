use super::*;
use crate::fetchers::oauth::common::SubscriptionBuilder;
use serde_json::json;

fn prompt_plan() -> Value {
    json!({
        "planStatus": {
            "planInfo": {
                "planName": "Pro",
                "monthlyPromptCredits": 500,
                "monthlyFlexCreditPurchaseAmount": 80
            },
            "availablePromptCredits": 350,
            "usedPromptCredits": 150,
            "availableFlexCredits": 20,
            "usedFlexCredits": 60,
            "planEnd": { "seconds": 1_700_000_000 }
        },
        "user": { "email": "ada@wind.dev" }
    })
}

#[test]
fn json_maps_prompt_add_on_and_period_without_inventing_zeros() {
    let snapshot = snapshot_from_json(&prompt_plan());
    assert_eq!(snapshot.plan_name.as_deref(), Some("Pro"));
    assert_eq!(snapshot.email.as_deref(), Some("ada@wind.dev"));
    assert_eq!(
        snapshot.prompt,
        Some(CreditAmounts {
            used: 150,
            total: 500
        })
    );
    assert_eq!(
        snapshot.add_on,
        Some(CreditAmounts {
            used: 60,
            total: 80
        })
    );
    assert_eq!(snapshot.period_end, Some(1_700_000_000));

    let usage = snapshot.into_usage("sub");
    let monthly = usage.monthly.expect("prompt window");
    assert_eq!(monthly.label, PROMPT_LABEL);
    assert_eq!(monthly.used, 150);
    assert_eq!(monthly.total, Some(500));
    assert_eq!(monthly.reset_at, Some(1_700_000_000));
    assert_eq!(monthly.breakdown.len(), 1);
    assert_eq!(monthly.breakdown[0].label, ADD_ON_LABEL);
    assert_eq!(monthly.breakdown[0].used, 60);
    assert!(usage.hourly.is_none());
    assert!(usage.weekly.is_none());
}

#[test]
fn missing_add_on_and_missing_prompt_omit_those_windows() {
    let mut body = prompt_plan();
    body["planStatus"]
        .as_object_mut()
        .unwrap()
        .remove("availableFlexCredits");
    body["planStatus"]
        .as_object_mut()
        .unwrap()
        .remove("usedFlexCredits");
    let usage = snapshot_from_json(&body).into_usage("sub");
    assert!(usage.monthly.unwrap().breakdown.is_empty());

    let only_plan = json!({
        "planStatus": {
            "planInfo": { "planName": "Free" },
            "planEnd": { "seconds": 42 }
        }
    });
    let usage = snapshot_from_json(&only_plan).into_usage("sub");
    assert_eq!(usage.plan_name.as_deref(), Some("Free"));
    assert!(usage.monthly.is_none(), "period alone must not zero a bar");
    assert!(usage.hourly.is_none());
    assert!(usage.weekly.is_none());
}

#[test]
fn available_alone_does_not_become_a_zero_used_bar() {
    assert!(credit_amounts(Some(40), None, None).is_none());
    assert_eq!(
        credit_amounts(Some(40), None, Some(100)).unwrap(),
        CreditAmounts {
            used: 60,
            total: 100
        }
    );
}

#[test]
fn proto_plan_status_maps_daily_and_weekly_percent() {
    let plan_info = field_bytes(2, b"Pro");
    let plan_end = field_varint(1, 1_700_000_000);
    let mut plan_status = field_bytes(1, &plan_info);
    plan_status.extend(field_bytes(3, &plan_end));
    plan_status.extend(field_varint(14, 80));
    plan_status.extend(field_varint(15, 40));
    let root = field_bytes(1, &plan_status);

    let parsed = parse_plan_status_proto(&root).expect("proto");
    let usage = snapshot_from_json(&parsed).into_usage("sub");
    assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
    assert!(usage.monthly.is_none());
    let daily = usage.hourly.expect("daily");
    assert_eq!(daily.label, "Daily");
    assert_eq!(daily.used, 20);
    assert_eq!(daily.total, Some(100));
    assert_eq!(daily.reset_at, Some(1_700_000_000));
    let weekly = usage.weekly.expect("weekly");
    assert_eq!(weekly.used, 60);
    assert_eq!(weekly.percent, Some(60));
}

#[test]
fn proto_without_plan_name_does_not_invent_unknown() {
    let root = field_bytes(1, &field_varint(15, 10));
    let parsed = parse_plan_status_proto(&root).expect("proto");
    let snapshot = snapshot_from_json(&parsed);
    assert!(snapshot.plan_name.is_none());
    assert_eq!(snapshot.weekly_remaining_percent, Some(10));
}

#[test]
fn http_status_classes_match_the_table() {
    assert!(matches!(
        decode_seat_body("GetPlanStatus", 401, b"{}").unwrap_err(),
        UsageError::AuthRequired
    ));
    assert!(matches!(
        decode_seat_body("GetPlanStatus", 400, br#"{"error":"invalid_grant"}"#).unwrap_err(),
        UsageError::AuthRequired
    ));
    let forbidden = decode_seat_body("GetPlanStatus", 403, b"nope").unwrap_err();
    assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
    let limited = decode_seat_body("GetPlanStatus", 429, b"slow").unwrap_err();
    assert!(limited.is_transient(), "{limited:?}");
    let broken = decode_seat_body("GetPlanStatus", 503, b"down").unwrap_err();
    assert!(matches!(broken, UsageError::Transient(_)), "{broken:?}");
}

#[tokio::test]
async fn session_quota_reads_plan_and_user_over_the_mock() {
    let mock = super::super::test_support::MockSeat::start(|_base, url| {
        if url.contains("GetPlanStatus") {
            (200, serde_json::to_string(&prompt_plan()).unwrap())
        } else if url.contains("GetCurrentUser") {
            (200, r#"{"user":{"email":"ada@wind.dev"}}"#.to_string())
        } else {
            (404, "unexpected".to_string())
        }
    });
    let mut sub = SubscriptionBuilder::new("windsurf", "Windsurf", "USD", "session-token", None)
        .provider_state(
            json!({
                "apiKey": "sk-ws-testkey12",
                "apiServerUrl": mock.base,
            })
            .to_string(),
        )
        .build();

    let usage = fetch_quota(&mut sub).await.expect("quota");
    assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
    assert_eq!(usage.monthly.unwrap().used, 150);
    assert_eq!(sub.oauth_account_id.as_deref(), Some("ada@wind.dev"));
    assert_eq!(sub.display_name, "ada@wind.dev");
}

#[tokio::test]
async fn plan_status_401_is_auth_required_and_403_is_not() {
    let unauthorized = super::super::test_support::MockSeat::start(|_base, url| {
        assert!(url.contains("GetPlanStatus"), "{url}");
        (401, r#"{"error":"invalid_grant"}"#.to_string())
    });
    let mut sub = SubscriptionBuilder::new("windsurf", "Windsurf", "USD", "session-token", None)
        .provider_state(json!({"apiServerUrl": unauthorized.base}).to_string())
        .build();
    let error = fetch_quota(&mut sub).await.expect_err("401");
    assert!(matches!(error, UsageError::AuthRequired), "{error:?}");

    let forbidden =
        super::super::test_support::MockSeat::start(|_base, _url| (403, "blocked".into()));
    let mut sub = SubscriptionBuilder::new("windsurf", "Windsurf", "USD", "session-token", None)
        .provider_state(json!({"apiServerUrl": forbidden.base}).to_string())
        .build();
    let error = fetch_quota(&mut sub).await.expect_err("403");
    assert!(matches!(error, UsageError::Fetcher(_)), "{error:?}");
    assert!(!matches!(error, UsageError::AuthRequired));
}

#[tokio::test]
async fn api_key_quota_uses_get_user_status() {
    let mock = super::super::test_support::MockSeat::start(|_base, url| {
        assert!(url.contains("GetUserStatus"), "{url}");
        (
                200,
                r#"{"userStatus":{"email":"key@wind.dev","planStatus":{"planInfo":{"planName":"Teams","monthlyPromptCredits":10},"usedPromptCredits":4}}}"#
                    .to_string(),
            )
    });
    let mut sub = SubscriptionBuilder::new("windsurf", "Windsurf", "USD", "", None)
        .provider_state(json!({"apiKey":"sk-ws-testkey12","apiServerUrl": mock.base}).to_string())
        .build();
    let usage = fetch_quota(&mut sub).await.expect("api key quota");
    assert_eq!(usage.plan_name.as_deref(), Some("Teams"));
    let monthly = usage.monthly.expect("prompt window");
    assert_eq!(monthly.used, 4);
    assert_eq!(monthly.total, Some(10));
}

#[tokio::test]
async fn transport_failure_is_transient() {
    let error = seat_call("http://127.0.0.1:1", "GetPlanStatus", json!({}))
        .await
        .expect_err("closed port");
    assert!(error.is_transient(), "{error:?}");
}
