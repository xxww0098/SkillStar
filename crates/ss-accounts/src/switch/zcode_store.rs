//! ZCode 的 enc:v1 字段：格式、密钥与读写。
//!
//! ZCode 的 credentials.json 里每个值都是
//!
//!   enc:v1:{nonce}.{tag}.{ciphertext}
//!
//! 三段都是 URL-safe base64（无填充）：12 字节 nonce、16 字节 GCM tag、密文。
//! AES-256-GCM 本身封的是 ciphertext||tag，这里按客户端的方式把 tag 单独摊成
//! 一段。密钥是那串 secret 的 SHA-256：
//!
//! * env ZCODE_CREDENTIAL_SECRET 非空 → 它的摘要；
//! * 否则 "zcode-credential-fallback:{platform}:{home}:{username}" 的摘要，
//!   与官方客户端在没有密钥文件时用的派生一致。
//!
//! 不可解析的值一律报错，不原样返回：原样返回会让"写进去了"变成假象。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:v1:";

/// 加解密失败的原因。凭据进了这个类型就不再是明文，也不打印明文。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum EncV1Error {
    #[error("ZCode 凭据密文格式无效")]
    Format,
    #[error("ZCode 凭据解密失败，密钥与写入环境不一致")]
    Decrypt,
    #[error("加密 ZCode 凭据失败")]
    Encrypt,
    #[error("ZCode 凭据不是有效 UTF-8")]
    Utf8,
}

/// 封锁一段明文。
pub(crate) fn seal(key: &[u8; 32], plaintext: &str) -> Result<String, EncV1Error> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncV1Error::Encrypt)?;
    let mut nonce = [0_u8; 12];
    for byte in &mut nonce {
        *byte = rand::random::<u8>();
    }
    let mut sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| EncV1Error::Encrypt)?;
    if sealed.len() < 16 {
        return Err(EncV1Error::Encrypt);
    }
    let tag = sealed.split_off(sealed.len() - 16);
    Ok(format!(
        "{PREFIX}{}.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(tag),
        URL_SAFE_NO_PAD.encode(sealed)
    ))
}

/// 解封一段密文。
pub(crate) fn open(key: &[u8; 32], value: &str) -> Result<String, EncV1Error> {
    let Some(rest) = value.strip_prefix(PREFIX) else {
        return Err(EncV1Error::Format);
    };
    let mut parts = rest.split('.');
    let (Some(nonce_b64), Some(tag_b64), Some(ciphertext_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(EncV1Error::Format);
    };
    let nonce = URL_SAFE_NO_PAD
        .decode(nonce_b64)
        .map_err(|_| EncV1Error::Format)?;
    let tag = URL_SAFE_NO_PAD
        .decode(tag_b64)
        .map_err(|_| EncV1Error::Format)?;
    let mut ciphertext = URL_SAFE_NO_PAD
        .decode(ciphertext_b64)
        .map_err(|_| EncV1Error::Format)?;
    if nonce.len() != 12 || tag.len() != 16 {
        return Err(EncV1Error::Format);
    }
    ciphertext.extend_from_slice(&tag);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncV1Error::Decrypt)?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| EncV1Error::Decrypt)?;
    String::from_utf8(plain).map_err(|_| EncV1Error::Utf8)
}

/// 这个平台该用哪个名字拼回退密钥（与官方客户端的命名一致）。
pub(crate) fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

/// 回退密钥的材料：平台 + home + 用户名。
pub(crate) fn fallback_secret(platform: &str, home: &std::path::Path, username: &str) -> String {
    format!(
        "zcode-credential-fallback:{platform}:{}:{username}",
        home.to_string_lossy()
    )
}

/// 这次进程该用哪把密钥：env 优先，否则按官方客户端的回退材料派生。
pub(crate) fn credential_key(home: &std::path::Path, username: &str) -> [u8; 32] {
    let secret = match std::env::var("ZCODE_CREDENTIAL_SECRET") {
        Ok(secret) if !secret.is_empty() => secret,
        _ => fallback_secret(platform_name(), home, username),
    };
    let digest = Sha256::digest(secret.as_bytes());
    let mut key = [0_u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// 当前用户名（平台环境变量各有各的写法）。
pub(crate) fn current_username() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_sealed_value_round_trips_and_has_the_client_format() {
        let key = [7_u8; 32];
        let sealed = seal(&key, "access-token-1").expect("seal");
        assert!(sealed.starts_with("enc:v1:"), "{sealed}");
        assert_eq!(sealed.matches('.').count(), 2, "{sealed}");
        assert!(!sealed.contains("access-token-1"), "the plaintext must not travel");
        assert_eq!(open(&key, &sealed).expect("open"), "access-token-1");
    }

    #[test]
    fn another_key_cannot_open_it() {
        let sealed = seal(&[1_u8; 32], "secret").expect("seal");
        assert_eq!(open(&[2_u8; 32], &sealed), Err(EncV1Error::Decrypt));
    }

    #[test]
    fn a_malformed_value_is_an_error_not_a_plaintext() {
        for value in [
            "plain-token",
            "enc:v1:",
            "enc:v1:aa.bb",
            "enc:v1:aa.bb.cc.dd",
            "enc:v1:@@@.@@@.@@@",
        ] {
            assert!(open(&[1_u8; 32], value).is_err(), "{value}");
        }
        // 段长度不对（nonce 不是 12 / tag 不是 16）。
        let short = format!(
            "enc:v1:{}.{}.{}",
            URL_SAFE_NO_PAD.encode([0_u8; 8]),
            URL_SAFE_NO_PAD.encode([0_u8; 16]),
            URL_SAFE_NO_PAD.encode(b"x")
        );
        assert_eq!(open(&[1_u8; 32], &short), Err(EncV1Error::Format));
    }

    #[test]
    fn the_fallback_key_material_names_platform_home_and_user() {
        let secret = fallback_secret("darwin", Path::new("/Users/dev"), "dev");
        assert_eq!(secret, "zcode-credential-fallback:darwin:/Users/dev:dev");
        // 同一个输入给同一把钥匙，换一个输入就换一把。
        let first = credential_key(Path::new("/Users/dev"), "dev");
        assert_eq!(first, credential_key(Path::new("/Users/dev"), "dev"));
        assert_ne!(first, credential_key(Path::new("/Users/other"), "dev"));
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn the_platform_name_matches_the_client_spelling() {
        let name = platform_name();
        assert!(matches!(name, "darwin" | "win32" | "linux"), "{name}");
    }
}
