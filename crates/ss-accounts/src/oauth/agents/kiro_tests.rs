//! Kiro 登录切片的测试：登录腿选择、区域、门户 URL、IdC 轮询与身份。
//!
//! 不打网络：全部驱动纯函数。
use super::*;
    use serde_json::json;

    #[test]
    fn the_default_leg_is_the_portal_and_idc_needs_an_explicit_choice() {
        assert_eq!(login_leg(None), LoginLeg::Portal);
        assert_eq!(login_leg(Some("portal")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("social")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("builder-id")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("idc")), LoginLeg::Idc { region: None });
        assert_eq!(login_leg(Some("AWS-IDC")), LoginLeg::Idc { region: None });
        assert_eq!(
            login_leg(Some("eu-central-1")),
            LoginLeg::Idc {
                region: Some("eu-central-1".to_string())
            }
        );
    }

    #[test]
    fn an_aws_region_is_geography_then_a_number() {
        for region in ["us-east-1", "eu-central-1", "us-gov-west-1", "us-isof-south-1"] {
            assert!(is_aws_region(region), "{region}");
        }
        for not_region in ["", "builder-id", "us-east", "U-east-1", "useast1", "us-east-"] {
            assert!(!is_aws_region(not_region), "{not_region:?}");
        }
    }

    #[test]
    fn the_region_falls_back_through_the_profile_arn() {
        assert_eq!(resolve_region(None, None), "us-east-1");
        assert_eq!(resolve_region(Some("eu-central-1"), None), "eu-central-1");
        assert_eq!(
            resolve_region(
                None,
                Some("arn:aws:codewhisperer:eu-west-1:123456789012:profile/ABC")
            ),
            "eu-west-1"
        );
        assert_eq!(
            profile_arn_region("arn:aws:codewhisperer:us-east-1:1:profile/X").as_deref(),
            Some("us-east-1")
        );
        assert_eq!(profile_arn_region("not-an-arn"), None);
    }

    #[test]
    fn the_runtime_endpoint_follows_the_region_and_unknown_ones_use_us_east_1() {
        assert_eq!(
            runtime_endpoint("eu-central-1"),
            "https://q.eu-central-1.amazonaws.com"
        );
        assert_eq!(
            runtime_endpoint("us-gov-west-1"),
            "https://q-fips.us-gov-west-1.amazonaws.com"
        );
        assert_eq!(runtime_endpoint("ap-southeast-2"), "https://q.us-east-1.amazonaws.com");
    }

    #[test]
    fn the_portal_authorization_url_is_pkce_on_the_signin_page() {
        let url = portal_auth_url("http://localhost:5000/oauth/callback", "state-1", "challenge-1");
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.host_str(), Some("app.kiro.dev"));
        assert_eq!(parsed.path(), "/signin");
        let query: std::collections::BTreeMap<String, String> = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(query.get("state").map(String::as_str), Some("state-1"));
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(
            query.get("redirect_from").map(String::as_str),
            Some("KiroIDE")
        );
    }

    #[test]
    fn the_state_round_trips_through_provider_json() {
        let state = KiroState {
            client_id: Some("cid".to_string()),
            client_secret: Some("csec".to_string()),
            region: Some("us-east-1".to_string()),
            start_url: Some(BUILDER_ID_START_URL.to_string()),
            profile_arn: None,
        };
        let value = state.to_value().expect("json");
        assert_eq!(value["clientId"], "cid");
        assert_eq!(value["startUrl"], BUILDER_ID_START_URL);
        assert!(value.get("profileArn").is_none(), "empty fields are omitted");
        assert_eq!(KiroState::parse(Some(&value)), state);
        assert!(state.has_idc_client());

        // snake_case 也认（老记录的写法）。
        let snake = json!({ "client_id": "cid", "client_secret": "csec", "idc_region": "eu-west-1" });
        let parsed = KiroState::parse(Some(&snake));
        assert!(parsed.has_idc_client());
        assert_eq!(parsed.region.as_deref(), Some("eu-west-1"));

        // 只有一半注册信息 = 没有注册。
        let half = json!({ "clientId": "cid" });
        assert!(!KiroState::parse(Some(&half)).has_idc_client());
        assert_eq!(KiroState::parse(None).to_value(), None);
    }

    #[test]
    fn a_token_body_accepts_camel_case_snake_case_and_the_data_wrapper() {
        let camel = tokens_from_body(&json!({
            "accessToken": "at",
            "refreshToken": "rt",
            "expiresIn": 3600
        }))
        .expect("camel");
        assert_eq!(camel.access_token, "at");
        assert_eq!(camel.refresh_token.as_deref(), Some("rt"));
        assert!(camel.expires_at.is_some());

        let snake = tokens_from_body(&json!({
            "data": { "access_token": "at2", "refresh_token": "rt2" }
        }))
        .expect("wrapped");
        assert_eq!(snake.access_token, "at2");
        assert_eq!(snake.refresh_token.as_deref(), Some("rt2"));
        assert_eq!(snake.expires_at, None);

        let missing = tokens_from_body(&json!({ "refreshToken": "rt" })).expect_err("no access");
        assert!(matches!(missing, OAuthError::Rejected(_)), "got {missing:?}");
    }

    #[test]
    fn the_idc_poll_waits_slows_down_and_stops_on_a_terminal_error() {
        assert!(matches!(
            interpret_idc_poll(400, r#"{"error":"authorization_pending"}"#, 5).expect("pending"),
            IdcPoll::Pending { interval: 5 }
        ));
        assert!(matches!(
            interpret_idc_poll(400, r#"{"error":"slow_down"}"#, 5).expect("slowed"),
            IdcPoll::Pending { interval: 10 }
        ));
        for (body, needle) in [
            (r#"{"error":"expired_token"}"#, "已过期"),
            (r#"{"error":"access_denied"}"#, "拒绝"),
        ] {
            let error = interpret_idc_poll(400, body, 5).err().expect("terminal");
            assert!(error.to_string().contains(needle), "{body}: got {error}");
        }
        // 403 是拒绝，不是凭据失效。
        let forbidden = interpret_idc_poll(403, "{}", 5).err().expect("rejected");
        assert!(matches!(forbidden, OAuthError::Rejected(_)), "got {forbidden:?}");
        // 5xx 是暂时不可用。
        let busy = interpret_idc_poll(503, "{}", 5).err().expect("transient");
        assert!(matches!(busy, OAuthError::Transient(_)), "got {busy:?}");
    }

    #[test]
    fn identity_comes_from_the_access_token_claims() {
        let token = crate::test_support::jwt_with_claims(json!({
            "email": "dev@example.com",
            "sub": "user-1"
        }));
        let identity = identity_from_tokens(&TokenSet {
            access_token: token,
            ..TokenSet::default()
        });
        assert_eq!(identity.email.as_deref(), Some("dev@example.com"));
        assert_eq!(identity.account_id.as_deref(), Some("user-1"));

        let opaque = identity_from_tokens(&TokenSet {
            access_token: "opaque".to_string(),
            ..TokenSet::default()
        });
        assert_eq!(opaque.email, None);
        assert_eq!(opaque.account_id, None);
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default(), None).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn a_region_that_is_not_a_region_is_refused_by_the_device_leg() {
        assert!(normalize_region(Some("builder-id")).is_err());
        assert_eq!(normalize_region(None).expect("default"), "us-east-1");
        assert_eq!(normalize_region(Some(" eu-west-1 ")).expect("region"), "eu-west-1");
    }

    #[test]
    fn a_device_session_needs_a_code_and_an_https_verification_page() {
        let session = DeviceSession::parse(&json!({
            "deviceCode": "dev",
            "userCode": "ABCD-EFGH",
            "verificationUri": "https://device.example/verify",
            "expiresIn": 600,
            "interval": 5
        }))
        .expect("session");
        assert_eq!(session.user_code, "ABCD-EFGH");
        assert_eq!(session.interval_secs, 5);
        assert_eq!(session.expires_in, 600);

        let insecure = json!({
            "deviceCode": "dev",
            "userCode": "ABCD",
            "verificationUri": "http://device.example/verify"
        });
        assert!(DeviceSession::parse(&insecure).is_err());
        assert!(DeviceSession::parse(&json!({ "userCode": "X" })).is_err());
    }
