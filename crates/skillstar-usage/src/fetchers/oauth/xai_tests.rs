use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

fn jwt_with_claims(claims: Value) -> String {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    format!("e30.{payload}.")
}

#[test]
fn oauth_scopes_include_conversations_for_grok_cli() {
    let authorize_url = build_authorize_url(
        "https://auth.x.ai/oauth2/authorize",
        "http://127.0.0.1:56121/callback",
        "challenge",
        "state",
        "nonce",
    )
    .unwrap();
    let parsed = url::Url::parse(&authorize_url).unwrap();
    let scopes = parsed
        .query_pairs()
        .find_map(|(key, value)| (key == "scope").then(|| value.into_owned()))
        .expect("Grok authorize URL must carry OAuth scopes");
    let scopes: std::collections::HashSet<_> = scopes.split_whitespace().collect();

    assert!(
        scopes.contains("conversations:read"),
        "without conversations:read the Grok CLI rejects the switched token and opens browser login"
    );
    assert!(
        scopes.contains("conversations:write"),
        "without conversations:write the Grok CLI rejects the switched token and opens browser login"
    );
}

#[test]
fn reset_token_selection_ignores_expired_and_empty_tokens() {
    let tokens = vec![
        GrokResetToken {
            token_id: "expired".into(),
            validity_end: 99,
        },
        GrokResetToken {
            token_id: "later".into(),
            validity_end: 300,
        },
        GrokResetToken {
            token_id: "earlier".into(),
            validity_end: 200,
        },
        GrokResetToken {
            token_id: String::new(),
            validity_end: 150,
        },
    ];

    assert_eq!(
        select_reset_token(tokens, 100),
        Some(GrokResetToken {
            token_id: "earlier".into(),
            validity_end: 200,
        })
    );
}

#[test]
fn grok_reset_wire_format_round_trips_token_response() {
    let token_id = "reset-token-1";
    let mut timestamp = Vec::new();
    timestamp.push(0x08); // Timestamp.seconds
    encode_varint(1_800_000_000, &mut timestamp);

    let mut token = Vec::new();
    token.push(0x0a); // ConsumerResetToken.token_id
    encode_varint(token_id.len() as u64, &mut token);
    token.extend_from_slice(token_id.as_bytes());
    token.extend_from_slice(&[0xa2, 0x01]); // ConsumerResetToken.validity_end
    encode_varint(timestamp.len() as u64, &mut token);
    token.extend_from_slice(&timestamp);

    let mut response = Vec::new();
    response.push(0x0a); // response.tokens
    encode_varint(token.len() as u64, &mut response);
    response.extend_from_slice(&token);

    let messages = decode_grpc_web_frames(&encode_grpc_web_frame(&response)).unwrap();
    let decoded = decode_remaining_resets_response(&messages).unwrap();

    assert_eq!(
        decoded,
        vec![GrokResetToken {
            token_id: token_id.into(),
            validity_end: 1_800_000_000,
        }]
    );
    assert_eq!(encode_redeem_reset_request(token_id), {
        let mut expected = vec![0x0a, token_id.len() as u8];
        expected.extend_from_slice(token_id.as_bytes());
        expected
    });
}

#[test]
fn grok_pending_login_keeps_the_target_subscription_id() {
    let pending_id = register_pending_login(
        "https://auth.example.test".to_string(),
        Some("sub-xai-existing"),
    );

    assert_eq!(
        crate::oauth::pending_state::target_subscription_id(&pending_id).as_deref(),
        Some("sub-xai-existing")
    );
    crate::oauth::pending_state::remove(&pending_id);
}

#[tokio::test]
async fn revoked_refresh_token_requires_reauthentication() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let address = format!("http://{}", server.server_addr());
    let responder = std::thread::spawn(move || {
        let request = server.recv().unwrap();
        request
            .respond(
                tiny_http::Response::from_string(
                    r#"{"error":"invalid_grant","error_description":"Refresh token has been revoked"}"#,
                )
                .with_status_code(400),
            )
            .unwrap();
    });

    // Same two steps `token_endpoint::post_token` performs after the send;
    // going through plain `reqwest` here keeps the test off the proxy-aware
    // shared client (which reads the developer's real config).
    let response = reqwest::get(address).await.unwrap();
    let status = response.status().as_u16();
    let body = response.text().await.unwrap();
    let error = crate::oauth::token_endpoint::parse_token_body(status, &body, "Grok refresh")
        .expect_err("revoked refresh tokens must require a fresh login");

    assert!(matches!(error, UsageError::AuthRequired), "{error}");
    responder.join().unwrap();
}

/// The companion to the test above: Grok's endpoint hiccuping must **not**
/// look like a revoked grant, or one 5xx would demand a fresh login and blank
/// the card.
#[tokio::test]
async fn grok_server_errors_stay_retryable() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let address = format!("http://{}", server.server_addr());
    let responder = std::thread::spawn(move || {
        let request = server.recv().unwrap();
        request
            .respond(tiny_http::Response::from_string("upstream unavailable").with_status_code(503))
            .unwrap();
    });

    let response = reqwest::get(address).await.unwrap();
    let status = response.status().as_u16();
    let body = response.text().await.unwrap();
    let error = crate::oauth::token_endpoint::parse_token_body(status, &body, "Grok refresh")
        .expect_err("503 is still a failure");

    assert!(matches!(error, UsageError::Transient(_)), "{error}");
    assert!(error.is_transient());
    responder.join().unwrap();
}

#[test]
fn reauthorization_replaces_the_target_grok_subscription() {
    let mut existing = SubscriptionBuilder::new(
        "xai",
        "Grok · old@example.com",
        "USD",
        "old-access-token",
        Some(1),
    )
    .refresh_token(Some("old-refresh-token".to_string()))
    .build();
    existing.id = "sub-xai-existing".to_string();
    existing.monthly_price = Some(199.0);
    existing.billing_cycle = crate::subscription::BillingCycle::Annual;
    existing.note = Some("keep me".to_string());
    existing.sort_index = 7;

    let updated = build_subscription(
        TokenResponse {
            access_token: Some("new-access-token".to_string()),
            refresh_token: Some("new-refresh-token".to_string()),
            expires_in: Some(3600),
            ..Default::default()
        },
        Some(&existing),
    )
    .unwrap();

    assert_eq!(updated.id, existing.id);
    assert_eq!(updated.monthly_price, existing.monthly_price);
    assert_eq!(updated.billing_cycle, existing.billing_cycle);
    assert_eq!(updated.note, existing.note);
    assert_eq!(updated.sort_index, existing.sort_index);
    assert!(!updated.requires_reauth);
    assert_ne!(
        updated.access_token_encrypted,
        existing.access_token_encrypted
    );
}

#[test]
fn reauthorization_keeps_existing_refresh_token_when_response_omits_one() {
    let old_access = jwt_with_claims(json!({ "sub": "uid-same", "exp": 1 }));
    let new_access = jwt_with_claims(json!({
        "sub": "uid-same",
        "exp": 1_999_999_999_i64
    }));
    let existing = SubscriptionBuilder::new("xai", "old@example.com", "USD", old_access, Some(1))
        .refresh_token(Some("still-valid-refresh-token".to_string()))
        .oauth_account_id(Some("uid-same".to_string()))
        .build();

    let updated = build_subscription(
        TokenResponse {
            access_token: Some(new_access),
            refresh_token: None,
            expires_in: Some(3600),
            ..Default::default()
        },
        Some(&existing),
    )
    .unwrap();

    assert_eq!(
        crypto::decrypt(updated.refresh_token_encrypted.as_deref().unwrap()),
        "still-valid-refresh-token"
    );
}

#[test]
fn cross_account_reauthorization_drops_old_account_refresh_and_id_tokens() {
    let old_id_token = jwt_with_claims(json!({
        "sub": "uid-old",
        "email": "old@example.com"
    }));
    let existing =
        SubscriptionBuilder::new("xai", "old@example.com", "USD", "old-access-token", Some(1))
            .refresh_token(Some("old-refresh-token".to_string()))
            .id_token(Some(old_id_token.clone()))
            .oauth_account_id(Some("uid-old".to_string()))
            .build();
    let new_access_token = jwt_with_claims(json!({
        "sub": "uid-new",
        "email": "new@example.com",
        "exp": 1_999_999_999_i64
    }));

    let updated = build_subscription(
        TokenResponse {
            access_token: Some(new_access_token),
            refresh_token: None,
            id_token: None,
            expires_in: None,
        },
        Some(&existing),
    )
    .unwrap();

    assert_eq!(updated.oauth_account_id.as_deref(), Some("uid-new"));
    assert_eq!(updated.display_name, "new@example.com");
    assert!(updated.refresh_token_encrypted.is_none());
    assert!(updated.id_token_encrypted.is_none());
}

#[test]
fn parses_root_shape_with_usage_total_used() {
    // Real xAI shape: fields at root, used under usage.totalUsed.
    let payload = json!({
        "billingCycle": {
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z"
        },
        "monthlyLimit": { "val": 99900 },
        "onDemandCap": { "val": 2000 },
        "usage": {
            "includedUsed": { "val": 12345 },
            "onDemandUsed": { "val": 0 },
            "totalUsed": { "val": 12345 }
        }
    });

    let usage = build_subscription_usage("sub-xai", &payload, None).unwrap();
    let monthly = usage.monthly.unwrap();

    assert_eq!(monthly.used, 12345);
    assert_eq!(monthly.total, Some(99900));
    assert_eq!(monthly.percent, Some(12));
    assert_eq!(usage.credits.len(), 1);
    assert_eq!(usage.credits[0].credit_amount.as_deref(), Some("$20"));
}

#[test]
fn parses_legacy_config_wrapper() {
    // Older fixtures / proxy mirrors wrap fields under `config`.
    let payload = json!({
        "config": {
            "monthlyLimit": { "val": "5000" },
            "used": { "val": 1250 },
            "onDemandCap": { "val": 2000 },
            "billingPeriodEnd": "2026-06-30T00:00:00Z"
        }
    });

    let usage = build_subscription_usage("sub-xai", &payload, None).unwrap();
    let monthly = usage.monthly.unwrap();

    assert_eq!(monthly.used, 1250);
    assert_eq!(monthly.total, Some(5000));
    assert_eq!(monthly.percent, Some(25));
    assert_eq!(usage.credits[0].credit_amount.as_deref(), Some("$20"));
}

#[test]
fn prefers_root_usage_over_config_used() {
    // When both root (real) and config (legacy) shapes are present,
    // root usage.totalUsed must win over a stray config.used.
    let payload = json!({
        "config": { "used": { "val": 999 } },
        "monthlyLimit": { "val": 10000 },
        "usage": { "totalUsed": { "val": 2500 } }
    });
    let usage = build_subscription_usage("sub-xai", &payload, None).unwrap();
    assert_eq!(usage.monthly.unwrap().used, 2500);
}

#[test]
fn no_weekly_window_without_current_period() {
    // Without the credits view (period=None) we only know the monthly
    // numbers: one "Monthly credits" bar, no weekly bar (no heuristic).
    let soon = Utc::now().timestamp() + 6 * 86_400;
    let payload = json!({
        "billingCycle": { "billingPeriodEnd": soon },
        "monthlyLimit": { "val": 5000 },
        "usage": { "totalUsed": { "val": 1000 } }
    });
    let usage = build_subscription_usage("sub-xai", &payload, None).unwrap();
    assert_eq!(usage.monthly.unwrap().label, "Monthly credits");
    assert!(
        usage.weekly.is_none(),
        "no weekly bar without currentPeriod"
    );
}

#[test]
fn monthly_only_for_monthly_plan() {
    // A monthly-plan currentPeriod yields the monthly bar only.
    let payload = json!({
        "billingCycle": { "billingPeriodEnd": "2026-07-01T00:00:00Z" },
        "monthlyLimit": { "val": 5000 },
        "usage": { "totalUsed": { "val": 1000 } }
    });
    let period = CurrentPeriod {
        weekly: Some(false),
        end: Some(1782864000),
        usage_percent: None,
    };
    let usage = build_subscription_usage("sub-xai", &payload, Some(period)).unwrap();
    assert_eq!(usage.monthly.unwrap().label, "Monthly credits");
    assert!(usage.weekly.is_none(), "monthly plan has no weekly bar");
}

#[test]
fn defaults_monthly_label_without_reset() {
    // No billingPeriodEnd → still parses, defaults to Monthly label.
    let payload = json!({
        "monthlyLimit": { "val": 5000 },
        "usage": { "totalUsed": { "val": 1000 } }
    });
    let usage = build_subscription_usage("sub-xai", &payload, None).unwrap();
    assert_eq!(usage.monthly.unwrap().label, "Monthly credits");
}

#[test]
fn rejects_empty_billing_config() {
    let payload = json!({ "config": {} });
    assert!(build_subscription_usage("sub-xai", &payload, None).is_err());
}

#[test]
fn rejects_garbage_payload() {
    let payload = json!({ "randomField": "value" });
    assert!(build_subscription_usage("sub-xai", &payload, None).is_err());
}

#[test]
fn parses_current_period_weekly_from_credits_view() {
    // Real `?format=credits` shape (config-wrapped), weekly plan.
    let payload = json!({
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-06-27T05:57:15.869945+00:00",
                "end": "2026-07-04T05:57:15.869945+00:00"
            },
            "billingPeriodStart": "2026-06-27T05:57:15.869945+00:00",
            "billingPeriodEnd": "2026-07-04T05:57:15.869945+00:00"
        }
    });
    let cp = parse_current_period(&payload).expect("currentPeriod parsed");
    assert_eq!(cp.weekly, Some(true));
    // 2026-07-04T05:57:15Z
    assert_eq!(cp.end, Some(1783144635));
}

#[test]
fn parses_current_period_monthly() {
    let payload = json!({
        "currentPeriod": {
            "type": "USAGE_PERIOD_TYPE_MONTHLY",
            "end": "2026-07-01T00:00:00Z"
        }
    });
    let cp = parse_current_period(&payload).unwrap();
    assert_eq!(cp.weekly, Some(false));
    assert_eq!(cp.end, Some(1782864000));
}

#[test]
fn weekly_plan_builds_two_bars() {
    // Weekly plan: monthly numeric quota (from the default view, resetting
    // on the calendar-month billingPeriodEnd) AND a separate weekly
    // progress bar (percent-only, resetting on currentPeriod.end).
    let weekly_end_ts = 1783144635; // 2026-07-04T05:57:15Z
    let month_end_ts = 1782864000; // 2026-07-01T00:00:00Z
    let payload = json!({
        "monthlyLimit": { "val": 20000 },
        "used": { "val": 7006 },
        "billingPeriodEnd": "2026-07-01T00:00:00Z"
    });
    let period = CurrentPeriod {
        weekly: Some(true),
        end: Some(weekly_end_ts),
        usage_percent: Some(13.0),
    };
    let usage = build_subscription_usage("sub-xai", &payload, Some(period)).unwrap();

    // Monthly: absolute numbers, monthly reset (NOT the weekly instant).
    let monthly = usage.monthly.unwrap();
    assert_eq!(monthly.label, "Monthly credits");
    assert_eq!(monthly.used, 7006);
    assert_eq!(monthly.total, Some(20000));
    assert_eq!(monthly.percent, Some(35));
    assert_eq!(monthly.reset_at, Some(month_end_ts));

    // Weekly: percent-only progress bar, weekly reset.
    let weekly = usage.weekly.expect("weekly bar present");
    assert_eq!(weekly.label, "Weekly credits");
    assert_eq!(weekly.percent, Some(13));
    assert_eq!(weekly.total, None, "weekly bar carries no absolute number");
    assert_eq!(weekly.reset_at, Some(weekly_end_ts));
}

#[test]
fn weekly_bar_zero_percent_when_omitted() {
    // proto3 omits creditUsagePercent at 0% → weekly bar shows 0, not gone.
    let payload = json!({ "monthlyLimit": { "val": 20000 }, "used": { "val": 7006 } });
    let period = CurrentPeriod {
        weekly: Some(true),
        end: Some(1783144635),
        usage_percent: None,
    };
    let usage = build_subscription_usage("sub-xai", &payload, Some(period)).unwrap();
    assert_eq!(usage.weekly.unwrap().percent, Some(0));
}

#[test]
fn parses_credit_usage_percent_from_credits_view() {
    // The `?format=credits` view carries creditUsagePercent next to
    // currentPeriod (a plain float), the weekly soft-limit usage.
    let payload = json!({
        "config": {
            "creditUsagePercent": 13.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "end": "2026-07-04T05:57:15.869945+00:00"
            }
        }
    });
    let cp = parse_current_period(&payload).expect("currentPeriod parsed");
    assert_eq!(cp.weekly, Some(true));
    assert_eq!(cp.usage_percent, Some(13.0));
}

#[test]
fn rejects_current_period_without_type_or_end() {
    let payload = json!({ "currentPeriod": { "type": "USAGE_PERIOD_TYPE_UNSPECIFIED" } });
    assert!(parse_current_period(&payload).is_none());
}
