//! Kiro 在盘上的凭据文件：只负责把 JSON 拼成客户端认的形状。
//!
//! 三份文件，各有各的作用：
//!
//! 1. ~/.aws/sso/cache/kiro-auth-token.json —— 真正的登录（accessToken /
//!    refreshToken / expiresAt / profileArn，IdC 还要 region / issuerUrl /
//!    clientId / clientIdHash）。
//! 2. ~/.aws/sso/cache/<sha1(startUrl)>.json —— IdC 客户端注册（clientSecret 只
//!    写这里；AWS 的公开 client 注册就是按 startUrl 的 SHA-1 命名的）。
//! 3. <Kiro app data>/profile.json —— 客户端窗口显示的身份。
//!
//! SHA-1 自己写：这个仓库没有 sha1 依赖，而它只在这里用一次（AWS 的命名约定），
//! 为一行摘要引一个 crate 不划算（也守住"不引重库"）。

use serde_json::{Map, Value};

/// AWS 的 clientIdHash 长度：40 个小写十六进制字符（SHA-1 十六进制）。
pub(crate) const HASH_LEN: usize = 40;

/// SHA-1 摘要的小写十六进制。只用于给 AWS 的注册文件名做哈希，不作安全用途。
pub(crate) fn client_id_hash(start_url: &str) -> String {
    sha1_hex(start_url.as_bytes())
}

/// 一个已经是 clientIdHash 形状的串（40 位小写十六进制）。
pub(crate) fn is_client_hash(value: &str) -> bool {
    value.len() == HASH_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// SHA-1 (FIPS 180-4) 的十六进制实现。输入只有 startUrl，长度远小于一页。
pub(crate) fn sha1_hex(input: &[u8]) -> String {
    let digest = sha1_digest(input);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    // 补位：0x80，补零到 56 mod 64，再放 64 位大端长度。
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                .rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999_u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0_u8; 20];
    for (index, word) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// 要写进客户端的这一份会话（从记录 + provider_state 拼出来）。
pub(crate) struct Material<'a> {
    pub(crate) access_token: &'a str,
    pub(crate) refresh_token: &'a str,
    pub(crate) expires_at: Option<i64>,
    pub(crate) email: Option<&'a str>,
    pub(crate) user_id: Option<&'a str>,
    pub(crate) profile_arn: Option<&'a str>,
    pub(crate) region: Option<&'a str>,
    pub(crate) start_url: Option<&'a str>,
    pub(crate) client_id: Option<&'a str>,
    pub(crate) client_secret: Option<&'a str>,
    pub(crate) idc: bool,
}

fn insert(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

/// kiro-auth-token.json。clientSecret 绝不进这个文件（它只属于注册文件）。
pub(crate) fn token_document(material: &Material<'_>) -> Value {
    let mut map = Map::new();
    insert(&mut map, "accessToken", Some(material.access_token));
    insert(&mut map, "refreshToken", Some(material.refresh_token));
    if let Some(expires_at) = material.expires_at.and_then(expires_rfc3339) {
        map.insert("expiresAt".to_string(), Value::String(expires_at));
    }
    insert(&mut map, "profileArn", material.profile_arn);
    if material.idc {
        insert(&mut map, "provider", Some("Enterprise"));
        insert(&mut map, "loginProvider", Some("Enterprise"));
        insert(&mut map, "authMethod", Some("IdC"));
        insert(&mut map, "region", material.region);
        insert(&mut map, "idcRegion", material.region);
        insert(&mut map, "issuerUrl", material.start_url);
        insert(&mut map, "clientId", material.client_id);
        if let Some(start_url) = material.start_url {
            map.insert(
                "clientIdHash".to_string(),
                Value::String(client_id_hash(start_url)),
            );
        }
        // 注册信息只属于注册文件。
        map.remove("clientSecret");
    } else {
        insert(&mut map, "provider", Some("BuilderId"));
        insert(&mut map, "loginProvider", Some("BuilderId"));
        insert(&mut map, "authMethod", Some("social"));
        insert(&mut map, "region", material.region);
    }
    insert(&mut map, "tokenType", Some("Bearer"));
    insert(&mut map, "email", material.email);
    insert(&mut map, "userId", material.user_id);
    Value::Object(map)
}

/// 注册文件：clientId 变了就整体重写（旧 secret 属于旧 client），否则保留未知字段。
pub(crate) fn registration_document(existing: Option<&Value>, client_id: &str, client_secret: &str) -> Value {
    let mut map = existing
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let same_client = map
        .get("clientId")
        .or_else(|| map.get("client_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        == Some(client_id);
    if !same_client {
        map.clear();
    }
    map.insert("clientId".to_string(), Value::String(client_id.to_string()));
    map.insert(
        "clientSecret".to_string(),
        Value::String(client_secret.to_string()),
    );
    Value::Object(map)
}

/// profile.json：客户端窗口显示的身份。
pub(crate) fn profile_document(material: &Material<'_>) -> Option<Value> {
    // 没有任何身份字段时不写 profile.json：一份只有 loginProvider 的文件会让
    // 客户端显示一个空账号。
    let arn = material
        .profile_arn
        .or(material.user_id)
        .or(material.email);
    if arn.is_none() && material.email.is_none() && material.user_id.is_none() {
        return None;
    }
    let mut map = Map::new();
    insert(&mut map, "arn", arn);
    insert(&mut map, "name", material.email);
    insert(&mut map, "email", material.email);
    insert(&mut map, "userId", material.user_id);
    insert(
        &mut map,
        "loginProvider",
        Some(if material.idc { "Enterprise" } else { "BuilderId" }),
    );
    (!map.is_empty()).then(|| Value::Object(map))
}

/// 用量缓存（state.vscdb 的 kiro.kiroAgent）：IDE 用它先画一版，随后自己刷新。
pub(crate) fn usage_document(material: &Material<'_>) -> Option<String> {
    let mut user = Map::new();
    insert(&mut user, "email", material.email);
    insert(&mut user, "userId", material.user_id);
    let mut root = Map::new();
    if !user.is_empty() {
        root.insert("userInfo".to_string(), Value::Object(user));
    }
    insert(&mut root, "profileArn", material.profile_arn);
    (!root.is_empty()).then(|| Value::Object(root).to_string())
}

/// 过期时间写成 RFC3339：Kiro 的文件里是字符串，不是 epoch。
pub(crate) fn expires_rfc3339(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|at| at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// 读回一个 RFC3339 或 epoch 的过期时间。
pub(crate) fn parse_expires(value: &Value) -> Option<i64> {
    if let Some(raw) = value.as_i64() {
        return normalize_epoch(raw);
    }
    let text = value.as_str()?.trim();
    if let Ok(raw) = text.parse::<i64>() {
        return normalize_epoch(raw);
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.timestamp())
}

fn normalize_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    Some(if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha1_matches_the_published_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha1_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        // 跨块边界（55/56/64 字节）不能错位。
        for length in [54_usize, 55, 56, 57, 63, 64, 65, 127, 128] {
            let input = vec![b'a'; length];
            assert_eq!(sha1_hex(&input).len(), 40, "{length}");
        }
    }

    #[test]
    fn the_client_id_hash_is_forty_lowercase_hex_characters() {
        let hash = client_id_hash("https://view.awsapps.com/start");
        assert_eq!(hash.len(), HASH_LEN);
        assert!(is_client_hash(&hash), "{hash}");
        assert_eq!(hash, client_id_hash("https://view.awsapps.com/start"));
        assert_ne!(hash, client_id_hash("https://view.awsapps.com/start/"));
        assert!(!is_client_hash("ABC"));
        assert!(!is_client_hash(&hash.to_uppercase()));
    }

    fn material() -> Material<'static> {
        Material {
            access_token: "at-1",
            refresh_token: "rt-1",
            expires_at: Some(1_800_000_000),
            email: Some("dev@example.com"),
            user_id: Some("user-1"),
            profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC"),
            region: Some("us-east-1"),
            start_url: Some("https://view.awsapps.com/start"),
            client_id: Some("cid"),
            client_secret: Some("csec"),
            idc: true,
        }
    }

    #[test]
    fn an_idc_token_document_carries_the_registration_but_never_the_secret() {
        let document = token_document(&material());
        assert_eq!(document["accessToken"], "at-1");
        assert_eq!(document["refreshToken"], "rt-1");
        assert_eq!(document["expiresAt"], "2027-01-15T08:00:00.000Z");
        assert_eq!(document["clientId"], "cid");
        assert_eq!(
            document["clientIdHash"],
            client_id_hash("https://view.awsapps.com/start")
        );
        assert_eq!(document["authMethod"], "IdC");
        assert!(document.get("clientSecret").is_none(), "the secret stays in the registration file");
    }

    #[test]
    fn a_portal_token_document_says_social_and_has_no_registration() {
        let portal = Material {
            idc: false,
            client_id: None,
            client_secret: None,
            start_url: None,
            ..material()
        };
        let document = token_document(&portal);
        assert_eq!(document["authMethod"], "social");
        assert_eq!(document["loginProvider"], "BuilderId");
        assert!(document.get("clientId").is_none());
        assert!(document.get("clientIdHash").is_none());
        assert_eq!(document["region"], "us-east-1");
    }

    #[test]
    fn a_registration_for_another_client_is_rewritten_from_scratch() {
        let existing = json!({ "clientId": "old", "clientSecret": "old-secret", "hint": "keep" });
        let rewritten = registration_document(Some(&existing), "cid", "csec");
        assert_eq!(rewritten["clientId"], "cid");
        assert_eq!(rewritten["clientSecret"], "csec");
        assert!(rewritten.get("hint").is_none(), "a new client must not keep the old hints");

        let same = registration_document(
            Some(&json!({ "clientId": "cid", "clientSecret": "old", "hint": "keep" })),
            "cid",
            "csec",
        );
        assert_eq!(same["hint"], "keep", "same client keeps what AWS put there");
        assert_eq!(same["clientSecret"], "csec");
    }

    #[test]
    fn the_profile_and_the_usage_cache_carry_the_same_identity() {
        let profile = profile_document(&material()).expect("profile");
        assert_eq!(profile["email"], "dev@example.com");
        assert_eq!(profile["userId"], "user-1");
        assert_eq!(
            profile["arn"],
            "arn:aws:codewhisperer:us-east-1:1:profile/ABC"
        );
        let usage = usage_document(&material()).expect("usage");
        assert!(usage.contains("dev@example.com"), "{usage}");

        let empty = Material {
            email: None,
            user_id: None,
            profile_arn: None,
            ..material()
        };
        assert!(profile_document(&empty).is_none(), "no identity, no profile document");
        assert!(usage_document(&empty).is_none());
    }

    #[test]
    fn expiry_times_round_trip_as_rfc3339_and_as_epochs() {
        assert_eq!(
            expires_rfc3339(1_800_000_000).as_deref(),
            Some("2027-01-15T08:00:00.000Z")
        );
        assert_eq!(parse_expires(&json!("2027-01-15T08:00:00.000Z")), Some(1_800_000_000));
        assert_eq!(parse_expires(&json!(1_800_000_000_i64)), Some(1_800_000_000));
        assert_eq!(parse_expires(&json!(1_800_000_000_000_i64)), Some(1_800_000_000));
        assert_eq!(parse_expires(&json!("not a time")), None);
        assert_eq!(parse_expires(&json!(0)), None);
    }
}
