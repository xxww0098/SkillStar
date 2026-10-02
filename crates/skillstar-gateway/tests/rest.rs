use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use skillstar_gateway::{
    AllowanceSnapshot, HoldWriter, Rest, RestSeat, UpstreamFailure, next_candidate, rest_after,
    verify_held,
};

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn plain(status: u16, body: &str) -> Rest {
    rest_after(&UpstreamFailure {
        status,
        body: body.as_bytes(),
        headers: &[],
        now: now(),
        failures: 1,
        snapshot: None,
    })
}

fn spell(rest: &Rest) -> Duration {
    rest.until.duration_since(now()).unwrap()
}

fn expect_spell(status: u16, body: &str, why: &str, by: &str, secs: u64) {
    let got = plain(status, body);
    assert_eq!(got.why, why, "{status} {body}");
    assert_eq!(got.by, by, "{status} {body}");
    assert_eq!(spell(&got), Duration::from_secs(secs), "{status} {body}");
    assert!(got.hold.is_none(), "{status} {body}");
    assert!(got.link.is_empty(), "{status} {body}");
}

#[test]
fn rest_credit_sits_out_for_30_minutes() {
    let minute = 60;
    expect_spell(402, "{}", "credit", "credit", 30 * minute);
    expect_spell(
        400,
        r#"{"error":{"message":"Your credit balance is too low"}}"#,
        "credit",
        "credit",
        30 * minute,
    );
    expect_spell(403, "账户余额不足", "credit", "credit", 30 * minute);
    expect_spell(402, "rate limit exceeded", "credit", "credit", 30 * minute);
    expect_spell(
        429,
        r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota"}}"#,
        "credit",
        "credit",
        30 * minute,
    );
    expect_spell(
        429,
        r#"{"error":{"code":"insufficient_quota"}}"#,
        "credit",
        "credit",
        30 * minute,
    );
    let words_on_429 = plain(429, "insufficient credits");
    assert_eq!(words_on_429.why, "rate");
    assert_eq!(spell(&words_on_429), Duration::from_secs(minute));
}

#[test]
fn rest_quota_sits_out_for_15_minutes() {
    let quarter = 15 * 60;
    expect_spell(
        429,
        "You've hit your limit · resets 5pm (Asia/Shanghai)",
        "quota",
        "quota",
        quarter,
    );
    expect_spell(
        429,
        r#"{"error":{"message":"usage limit reached for your plan"}}"#,
        "quota",
        "quota",
        quarter,
    );
    expect_spell(
        429,
        r#"{"error":{"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED"}}"#,
        "quota",
        "quota",
        quarter,
    );
    expect_spell(
        429,
        r#"{"error":{"type":"usage_limit_reached","message":"You've hit your usage limit."}}"#,
        "quota",
        "quota",
        quarter,
    );
    // The same second the clock shows is not a future reset.
    expect_spell(
        502,
        "Claude AI usage limit reached|1700000000",
        "quota",
        "quota",
        quarter,
    );
    let early = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"message":"You've hit your usage limit."}}"#,
        headers: &[],
        now: now(),
        failures: 1,
        snapshot: Some(AllowanceSnapshot {
            percent: 97.0,
            renews_at: Some(now() + Duration::from_secs(50 * 60 * 60)),
        }),
    });
    assert_eq!(early.by, "quota");
    assert_eq!(spell(&early), Duration::from_secs(quarter));
}

#[test]
fn rest_rate_limit_is_not_quota() {
    let minute = 60;
    let quarter = 15 * minute;
    for body in [
        r#"{"error":{"code":null,"message":"ORF: Rate limit exceeded: free-models-per-min","param":null,"type":"rate_limit_error"}}"#,
        r#"{"error":{"message":"Rate limit reached for gpt-4o in organization org-x on tokens per min (TPM): Limit 30000, Used 29000"}}"#,
        r#"{"type":"error","error":{"type":"rate_limit_error","message":"This request would exceed your organization's rate limit of 50,000 input tokens per minute"}}"#,
        r#"{"error":{"message":"Too Many Requests"}}"#,
    ] {
        let got = plain(429, body);
        assert_eq!(got.why, "rate", "{body}");
        assert_eq!(got.by, "cooldown", "{body}");
        assert_eq!(spell(&got), Duration::from_secs(minute), "{body}");
        assert_ne!(spell(&got), Duration::from_secs(quarter), "{body}");
    }
    expect_spell(
        429,
        r#"{"error":{"message":"Rate limit exceeded: free-models-per-day. Add 10 credits to unlock 1000 free model requests per day"}}"#,
        "quota",
        "quota",
        quarter,
    );
    expect_spell(
        403,
        r#"{"error":{"message":"rate limit exceeded for plan"}}"#,
        "quota",
        "quota",
        quarter,
    );
    let noted = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"message":"Too many requests"}}"#,
        headers: &[("X-Skillstar-Resets-At", "1700013500")],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(noted.why, "rate");
    assert_eq!(noted.by, "cooldown");
    assert_eq!(spell(&noted), Duration::from_secs(minute));
}

#[test]
fn rest_retry_after_is_capped_at_one_hour() {
    let hour = 60 * 60;
    let rate = |headers: &[(&str, &str)]| {
        rest_after(&UpstreamFailure {
            status: 429,
            body: br#"{"error":{"message":"Too many requests"}}"#,
            headers,
            now: now(),
            failures: 1,
            snapshot: None,
        })
    };
    let long = rate(&[("Retry-After", "36000")]);
    assert_eq!(long.by, "retry-after");
    assert_eq!(spell(&long), Duration::from_secs(hour));
    let exact = rate(&[("Retry-After", "3600")]);
    assert_eq!(spell(&exact), Duration::from_secs(hour));
    let over = rate(&[("Retry-After", "3601")]);
    assert_eq!(spell(&over), Duration::from_secs(hour));
    let five = rate(&[("Retry-After", "300")]);
    assert_eq!(five.why, "rate");
    assert_eq!(spell(&five), Duration::from_secs(300));
    let fraction = rate(&[("Retry-After", "1.5")]);
    assert_eq!(spell(&fraction), Duration::from_millis(1500));

    let dated = rate(&[("Retry-After", "Tue, 14 Nov 2023 22:14:50 GMT")]);
    assert_eq!(dated.by, "retry-after");
    assert_eq!(spell(&dated), Duration::from_secs(90));
    let past = rate(&[("Retry-After", "Tue, 14 Nov 2023 22:13:19 GMT")]);
    assert_eq!(past.by, "cooldown");
    assert_eq!(spell(&past), Duration::from_secs(60));

    let header = rate(&[("x-ratelimit-reset-requests", "90s")]);
    assert_eq!(header.by, "retry-after");
    assert_eq!(spell(&header), Duration::from_secs(90));
    let named = rate(&[("anthropic-ratelimit-reset", "2023-11-14T23:13:20Z")]);
    assert_eq!(spell(&named), Duration::from_secs(hour));
    let first = rate(&[
        ("Retry-After", "300"),
        ("x-ratelimit-reset-requests", "90s"),
    ]);
    assert_eq!(spell(&first), Duration::from_secs(300));
    let negative = rate(&[("Retry-After", "-1"), ("x-ratelimit-reset-requests", "90s")]);
    assert_eq!(negative.by, "retry-after");
    assert_eq!(spell(&negative), Duration::from_secs(90));

    let quota = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"message":"usage limit reached"}}"#,
        headers: &[("Retry-After", "36000")],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(quota.why, "quota");
    assert_eq!(quota.by, "retry-after");
    assert_eq!(spell(&quota), Duration::from_secs(hour));
}

#[test]
fn rest_quota_reset_is_capped_at_eight_days() {
    let day = 24 * 60 * 60;
    let eight = 8 * day;
    let month = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"type":"usage_limit_reached","resets_in_seconds":2592000}}"#,
        headers: &[],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(month.why, "quota");
    assert_eq!(month.by, "resets");
    assert_eq!(spell(&month), Duration::from_secs(eight));

    let exact =
        format!(r#"{{"error":{{"type":"usage_limit_reached","resets_in_seconds":{eight}}}}}"#);
    let kept = plain(429, &exact);
    assert_eq!(kept.by, "resets");
    assert_eq!(spell(&kept), Duration::from_secs(eight));
    let over = format!(
        r#"{{"error":{{"type":"usage_limit_reached","resets_in_seconds":{}}}}}"#,
        eight + 1
    );
    let cut = plain(429, &over);
    assert_eq!(cut.by, "resets");
    assert_eq!(spell(&cut), Duration::from_secs(eight));

    let pipe = plain(502, "Claude AI usage limit reached|1790000000");
    assert_eq!(pipe.why, "quota");
    assert_eq!(pipe.by, "resets");
    assert_eq!(spell(&pipe), Duration::from_secs(eight));

    let later = 1_700_000_000 + 3 * 60 * 60 + 45 * 60;
    let body = format!(
        r#"{{"error":{{"type":"usage_limit_reached","message":"The usage limit has been reached","resets_at":{later},"resets_in_seconds":13500}}}}"#
    );
    let chatgpt = plain(429, &body);
    assert_eq!(chatgpt.by, "resets");
    assert_eq!(spell(&chatgpt), Duration::from_secs(3 * 60 * 60 + 45 * 60));
    assert_ne!(spell(&chatgpt), Duration::from_secs(60 * 60));

    let noted = rest_after(&UpstreamFailure {
        status: 429,
        body: b"Codex: The usage limit has been reached",
        headers: &[
            ("X-Skillstar-Resets-At", "1700013500"),
            ("Retry-After", "13500"),
        ],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(noted.why, "quota");
    assert_eq!(noted.by, "resets");
    assert_eq!(spell(&noted), Duration::from_secs(3 * 60 * 60 + 45 * 60));

    let magpie = rest_after(&UpstreamFailure {
        status: 429,
        body: b"Codex: The usage limit has been reached",
        headers: &[("X-Magpie-Resets-At", "1700013500")],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(magpie.by, "quota");
    assert_eq!(spell(&magpie), Duration::from_secs(15 * 60));

    let window = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"message":"You've hit your usage limit"}}"#,
        headers: &[("Retry-After", "600")],
        now: now(),
        failures: 1,
        snapshot: Some(AllowanceSnapshot {
            percent: 100.0,
            renews_at: Some(now() + Duration::from_secs(50 * 60 * 60)),
        }),
    });
    assert_eq!(window.by, "window");
    assert_eq!(spell(&window), Duration::from_secs(50 * 60 * 60));

    let long_window = rest_after(&UpstreamFailure {
        status: 429,
        body: br#"{"error":{"message":"You've hit your usage limit"}}"#,
        headers: &[],
        now: now(),
        failures: 1,
        snapshot: Some(AllowanceSnapshot {
            percent: 98.0,
            renews_at: Some(now() + Duration::from_secs(10 * day)),
        }),
    });
    assert_eq!(long_window.by, "window");
    assert_eq!(spell(&long_window), Duration::from_secs(eight));

    let in_body = rest_after(&UpstreamFailure {
        status: 429,
        body: body.as_bytes(),
        headers: &[("X-Skillstar-Resets-At", "1700003600")],
        now: now(),
        failures: 1,
        snapshot: Some(AllowanceSnapshot {
            percent: 100.0,
            renews_at: Some(now() + Duration::from_secs(50 * 60 * 60)),
        }),
    });
    assert_eq!(in_body.by, "resets");
    assert_eq!(spell(&in_body), Duration::from_secs(3 * 60 * 60 + 45 * 60));
}

#[test]
fn rest_backoff_is_capped_at_ten_minutes() {
    let ten = 10 * 60;
    let failed = |failures: u32| {
        rest_after(&UpstreamFailure {
            status: 503,
            body: b"overloaded",
            headers: &[],
            now: now(),
            failures,
            snapshot: None,
        })
    };
    let first = failed(1);
    assert_eq!(first.why, "other");
    assert_eq!(first.by, "backoff");
    assert_eq!(first.failures, 1);
    assert_eq!(spell(&first), Duration::from_secs(60));
    assert_ne!(spell(&first), Duration::from_secs(30));
    let second = failed(2);
    assert_eq!(second.failures, 2);
    assert_eq!(spell(&second), Duration::from_secs(120));
    assert_eq!(spell(&failed(4)), Duration::from_secs(8 * 60));
    assert_eq!(failed(5).failures, 5);
    assert_eq!(spell(&failed(5)), Duration::from_secs(ten));
    assert_eq!(spell(&failed(12)), Duration::from_secs(ten));
    let zero = failed(0);
    assert_eq!(zero.failures, 1);
    assert_eq!(spell(&zero), Duration::from_secs(60));

    let noted = rest_after(&UpstreamFailure {
        status: 503,
        body: b"overloaded",
        headers: &[("X-Skillstar-Resets-At", "1700013500")],
        now: now(),
        failures: 1,
        snapshot: None,
    });
    assert_eq!(noted.by, "backoff");
    assert_eq!(spell(&noted), Duration::from_secs(60));

    let window = rest_after(&UpstreamFailure {
        status: 502,
        body: b"bad gateway",
        headers: &[],
        now: now(),
        failures: 1,
        snapshot: Some(AllowanceSnapshot {
            percent: 100.0,
            renews_at: Some(now() + Duration::from_secs(3 * 60 * 60)),
        }),
    });
    assert_eq!(window.why, "other");
    assert_eq!(window.by, "window");
    assert_eq!(window.failures, 1);
    assert_eq!(spell(&window), Duration::from_secs(3 * 60 * 60));

    let nine_days = 9 * 24 * 60 * 60;
    let uncapped = rest_after(&UpstreamFailure {
        status: 502,
        body: b"bad gateway",
        headers: &[],
        now: now(),
        failures: 3,
        snapshot: Some(AllowanceSnapshot {
            percent: 100.0,
            renews_at: Some(now() + Duration::from_secs(nine_days)),
        }),
    });
    assert_eq!(uncapped.by, "window");
    assert_eq!(uncapped.failures, 3);
    assert_eq!(spell(&uncapped), Duration::from_secs(nine_days));
}

#[test]
fn rest_verify_sits_out_for_30_minutes() {
    let linked = r#"{"error":{"code":403,"message":"Verify your account to continue.","status":"PERMISSION_DENIED","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"VALIDATION_REQUIRED","metadata":{"validation_url":"https://accounts.google.com/signin/continue?sarp=1&x=2"}}]}}"#;
    let got = plain(403, linked);
    assert_eq!(got.why, "verify");
    assert_eq!(got.by, "verify");
    assert_eq!(spell(&got), Duration::from_secs(30 * 60));
    assert_eq!(
        got.link,
        "https://accounts.google.com/signin/continue?sarp=1&x=2"
    );
    assert!(verify_held(&got, now()));
    assert!(!verify_held(
        &plain(
            403,
            r#"{"error":{"message":"The caller does not have permission"}}"#
        ),
        now()
    ));

    let camel = plain(
        401,
        r#"{"error":{"details":[{"reason":"VALIDATION_REQUIRED","metadata":{"validationUrl":"https://accounts.google.com/v"}}]}}"#,
    );
    assert_eq!(camel.why, "verify");
    assert_eq!(camel.link, "https://accounts.google.com/v");

    let insecure = plain(
        403,
        r#"{"error":{"details":[{"reason":"VALIDATION_REQUIRED","metadata":{"validation_url":"http://accounts.google.com/v"}}]}}"#,
    );
    assert_eq!(insecure.why, "verify");
    assert!(insecure.link.is_empty());
    assert_eq!(spell(&insecure), Duration::from_secs(30 * 60));

    let helped = plain(
        403,
        r#"{"error":{"details":[{"reason":"VALIDATION_REQUIRED","links":[{"url":"https://accounts.google.com/help"}]}]}}"#,
    );
    assert_eq!(helped.link, "https://accounts.google.com/help");

    let said = plain(
        403,
        "this Google account needs to be verified: open https://accounts.google.com/x in a browser",
    );
    assert_eq!(said.why, "verify");
    assert_eq!(said.link, "https://accounts.google.com/x");

    let not_verify = plain(429, "verify your account");
    assert_eq!(not_verify.why, "rate");
    assert_eq!(spell(&not_verify), Duration::from_secs(60));
}

#[test]
fn rest_verify_hold_lasts_one_minute() {
    let got = plain(403, "verify your account");
    let hold = got.hold.expect("verification keeps a hold");
    assert_eq!(hold.duration_since(now()).unwrap(), Duration::from_secs(60));
    assert!(verify_held(&got, now() + Duration::from_secs(59)));
    assert!(!verify_held(&got, now() + Duration::from_secs(60)));
    assert_eq!(spell(&got), Duration::from_secs(30 * 60));
    let seats = [RestSeat {
        id: "a",
        until: Some(got.until),
    }];
    assert_eq!(
        next_candidate(false, &seats, now() + Duration::from_secs(60)),
        None
    );
    assert_eq!(
        next_candidate(false, &seats, now() + Duration::from_secs(30 * 60)),
        Some("a")
    );
}

#[test]
fn rest_chinese_words_follow_the_same_lists() {
    expect_spell(500, "余额不足", "credit", "credit", 30 * 60);
    expect_spell(429, "余额不足", "rate", "cooldown", 60);
    expect_spell(429, "额度用完", "quota", "quota", 15 * 60);
    expect_spell(429, "请求太频繁", "rate", "cooldown", 60);
    expect_spell(429, "频率超了，额度没了", "quota", "quota", 15 * 60);
    expect_spell(500, "已到上限", "quota", "quota", 15 * 60);
}

#[test]
fn rest_plan_drops_a_seat_until_the_deadline() {
    let seats = [
        RestSeat {
            id: "a",
            until: Some(now() + Duration::from_secs(1)),
        },
        RestSeat {
            id: "b",
            until: None,
        },
    ];
    assert_eq!(next_candidate(false, &seats, now()), Some("b"));
    assert_eq!(
        next_candidate(false, &seats, now() + Duration::from_secs(1)),
        Some("a")
    );
    let both = [
        RestSeat {
            id: "a",
            until: Some(now() + Duration::from_secs(1)),
        },
        RestSeat {
            id: "b",
            until: Some(now() + Duration::from_secs(1)),
        },
    ];
    assert_eq!(next_candidate(false, &both, now()), None);
}

#[test]
fn rest_after_content_byte_does_not_call_second_upstream() {
    let seats = [
        RestSeat {
            id: "a",
            until: Some(now() + Duration::from_secs(15 * 60)),
        },
        RestSeat {
            id: "b",
            until: None,
        },
    ];
    assert_eq!(next_candidate(false, &seats, now()), Some("b"));

    let mut downstream = Vec::new();
    let mut hold = HoldWriter::new(&mut downstream, || Duration::ZERO);
    hold.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n")
        .unwrap();
    assert!(hold.committed());
    hold.write_all(b"data: {\"type\":\"error\"}\n\n").unwrap();
    assert!(hold.committed());
    assert!(!hold.failed_before_content());

    let mut asked = vec!["a"];
    if let Some(id) = next_candidate(hold.committed(), &seats, now()) {
        asked.push(id);
    }
    assert_eq!(asked, vec!["a"]);

    let mut early_downstream = Vec::new();
    let mut early = HoldWriter::new(&mut early_downstream, || Duration::ZERO);
    early.write_all(b"data: {\"type\":\"error\"}\n\n").unwrap();
    assert!(early.failed_before_content());
    assert!(!early.committed());
    let mut swapped = vec!["a"];
    if let Some(id) = next_candidate(early.committed(), &seats, now()) {
        swapped.push(id);
    }
    assert_eq!(swapped, vec!["a", "b"]);
}
