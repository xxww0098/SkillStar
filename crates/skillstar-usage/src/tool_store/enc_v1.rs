//! ZCode `enc:v1` credential values.
//!
//! Cockpit `zcode_account.rs` does not concatenate nonce and
//! `ciphertext || tag` into two base64 blobs. The wire value is
//!
//! `enc:v1:{nonce}.{tag}.{ciphertext}`
//!
//! each component URL-safe base64 without padding. The nonce is 12 bytes.
//! AES-256-GCM still seals `ciphertext || tag`; the tag is the last 16 bytes,
//! stored in its own field. Unlike cockpit, a value without the prefix is an
//! error rather than returned unchanged.
//!
//! The key is SHA-256 of `ZCODE_CREDENTIAL_SECRET` when that variable is
//! non-empty, otherwise
//! `zcode-credential-fallback:{platform}:{home}:{username}`.

#![cfg_attr(not(test), allow(dead_code))]

use std::path::{Path, PathBuf};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:v1:";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncV1Error {
    #[error("ZCode 凭据密文格式无效")]
    Format,
    #[error("ZCode 凭据解密失败，密钥与写入环境不一致")]
    Decrypt,
    #[error("加密 ZCode 凭据失败")]
    Encrypt,
    #[error("ZCode 凭据不是有效 UTF-8")]
    Utf8,
}

pub fn encrypt_enc_v1(key: &[u8; 32], plaintext: &str) -> Result<String, EncV1Error> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncV1Error::Encrypt)?;
    let mut nonce = [0u8; 12];
    for byte in &mut nonce {
        *byte = rand::random::<u8>();
    }
    let mut encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| EncV1Error::Encrypt)?;
    if encrypted.len() < 16 {
        return Err(EncV1Error::Encrypt);
    }
    let tag = encrypted.split_off(encrypted.len() - 16);
    Ok(format!(
        "{PREFIX}{}.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(tag),
        URL_SAFE_NO_PAD.encode(encrypted)
    ))
}

pub fn decrypt_enc_v1(key: &[u8; 32], value: &str) -> Result<String, EncV1Error> {
    let Some(rest) = value.strip_prefix(PREFIX) else {
        return Err(EncV1Error::Format);
    };
    let mut parts = rest.split('.');
    let (Some(nonce_b64), Some(tag_b64), Some(ciphertext_b64)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(EncV1Error::Format);
    };
    if parts.next().is_some() {
        return Err(EncV1Error::Format);
    }
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

/// SHA-256 of the env secret, or of the platform fallback.
pub fn zcode_credential_key(home: &Path, username: &str) -> [u8; 32] {
    sha256(credential_secret(home, username).as_bytes())
}

pub fn zcode_platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

pub fn fallback_credential_secret(platform: &str, home: &Path, username: &str) -> String {
    format!(
        "zcode-credential-fallback:{platform}:{}:{username}",
        home.to_string_lossy()
    )
}

/// `{zcode_home()}/v2/credentials.json`, after `setting.json`'s `dataBaseDir`.
pub fn zcode_credentials_path() -> PathBuf {
    crate::tool_paths::zcode_home()
        .join("v2")
        .join("credentials.json")
}

fn credential_secret(home: &Path, username: &str) -> String {
    if let Ok(secret) = std::env::var("ZCODE_CREDENTIAL_SECRET")
        && !secret.is_empty()
    {
        return secret;
    }
    fallback_credential_secret(zcode_platform_name(), home, username)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cockpit's fixed vector. Node AES-256-GCM, key = SHA-256 of the darwin fallback.
    const FIXTURE: &str =
        "enc:v1:AAECAwQFBgcICQoL.NTIF8rgqI66J7hvPIwTD8g.QTtgwDlfAEvz72ttQggYC2KZyVwLVA";
    const DARWIN_MATERIAL: &str = "zcode-credential-fallback:darwin:/Users/zcode-test:test-user";
    const DARWIN_KEY: &str = "3ead18a3d8ab40c108ab7c53e698ef355b73fc2386174dff684c6e50f34cb80a";
    const ENV_KEY: &str = "86f3f00cb77b08bb8963beabc8e08a2213caf723cb092397446667f53888bee3";

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        tool_sync: Option<std::ffi::OsString>,
        secret: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox_without_secret(path: &Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let tool_sync = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let secret = std::env::var_os("ZCODE_CREDENTIAL_SECRET");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
                std::env::remove_var("ZCODE_CREDENTIAL_SECRET");
            }
            Self {
                _lock: lock,
                tool_sync,
                secret,
            }
        }

        fn set_secret(&self, value: &str) {
            // SAFETY: the guard still holds test_env_lock.
            unsafe {
                std::env::set_var("ZCODE_CREDENTIAL_SECRET", value);
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still serialized by the crate-wide test_env_lock.
            unsafe {
                match &self.tool_sync {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
                match &self.secret {
                    Some(value) => std::env::set_var("ZCODE_CREDENTIAL_SECRET", value),
                    None => std::env::remove_var("ZCODE_CREDENTIAL_SECRET"),
                }
            }
        }
    }

    fn key_from_hex(hex: &str) -> [u8; 32] {
        let mut key = [0u8; 32];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex");
        }
        key
    }

    #[test]
    fn credential_key_bytes_follow_env_then_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = EnvGuard::sandbox_without_secret(dir.path());
        let home = Path::new("/Users/zcode-test");
        let material = fallback_credential_secret(zcode_platform_name(), home, "test-user");
        assert_eq!(
            material,
            format!(
                "zcode-credential-fallback:{}:{}:test-user",
                zcode_platform_name(),
                home.to_string_lossy()
            )
        );
        assert_eq!(
            zcode_credential_key(home, "test-user"),
            sha256(material.as_bytes())
        );

        let darwin_key = key_from_hex(DARWIN_KEY);
        assert_eq!(sha256(DARWIN_MATERIAL.as_bytes()), darwin_key);
        assert_eq!(
            decrypt_enc_v1(&darwin_key, FIXTURE).expect("fixture"),
            "official-fixture-token"
        );
        let mut wrong = darwin_key;
        wrong[0] ^= 0xff;
        assert_eq!(
            decrypt_enc_v1(&wrong, FIXTURE).expect_err("wrong key"),
            EncV1Error::Decrypt
        );

        #[cfg(unix)]
        assert_eq!(
            fallback_credential_secret("darwin", home, "test-user"),
            DARWIN_MATERIAL
        );

        guard.set_secret("custom-secret");
        assert_eq!(
            zcode_credential_key(home, "someone-else"),
            key_from_hex(ENV_KEY)
        );
        guard.set_secret("");
        assert_eq!(
            zcode_credential_key(home, "test-user"),
            sha256(material.as_bytes())
        );
    }

    #[test]
    fn encrypt_round_trip_uses_three_components() {
        let key = [0x7u8; 32];
        let encoded = encrypt_enc_v1(&key, "secret-value").expect("encrypt");
        let rest = encoded.strip_prefix(PREFIX).expect("prefix");
        let parts: Vec<_> = rest.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(URL_SAFE_NO_PAD.decode(parts[0]).expect("nonce").len(), 12);
        assert_eq!(URL_SAFE_NO_PAD.decode(parts[1]).expect("tag").len(), 16);
        assert_eq!(
            decrypt_enc_v1(&key, &encoded).expect("decrypt"),
            "secret-value"
        );
        assert_eq!(
            decrypt_enc_v1(&key, "plain-value").expect_err("prefix"),
            EncV1Error::Format
        );
        assert_eq!(
            decrypt_enc_v1(&key, "enc:v1:only-one").expect_err("parts"),
            EncV1Error::Format
        );
    }

    #[test]
    fn overridden_credentials_round_trip_keeps_a_byte_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = EnvGuard::sandbox_without_secret(dir.path());
        let v2 = dir.path().join(".zcode").join("v2");
        std::fs::create_dir_all(&v2).expect("mkdir");
        std::fs::write(
            v2.join("settings.json"),
            r#"{"dataBaseDir":"/should/not/use"}"#,
        )
        .expect("wrong name");
        assert_eq!(
            zcode_credentials_path(),
            dir.path()
                .join(".zcode")
                .join("v2")
                .join("credentials.json"),
            "settings.json is not the cockpit filename"
        );

        let override_root = dir.path().join("override");
        std::fs::write(
            v2.join("setting.json"),
            serde_json::json!({ "dataBaseDir": override_root.to_string_lossy() }).to_string(),
        )
        .expect("setting");
        let path = zcode_credentials_path();
        assert_eq!(
            path,
            override_root
                .join(".zcode")
                .join("v2")
                .join("credentials.json")
        );
        assert!(path.starts_with(dir.path()));
        assert_ne!(
            crate::tool_paths::zcode_home(),
            skillstar_core::infra::paths::home_dir().join(".zcode")
        );

        let key = [0x42; 32];
        let original = serde_json::json!({
            "preserved": "keep",
            "accessToken": encrypt_enc_v1(&key, "old-token").expect("old"),
        });
        crate::tool_store::atomic_json::write(&path, &original).expect("write");
        let previous = std::fs::read(&path).expect("backup bytes");
        let backup = dir.path().join("credentials.prev");
        std::fs::write(&backup, &previous).expect("aside");

        let mut value: serde_json::Value = serde_json::from_slice(&previous).expect("parse");
        let current = value["accessToken"].as_str().expect("field");
        assert_eq!(
            decrypt_enc_v1(&key, current).expect("old plain"),
            "old-token"
        );
        value["accessToken"] =
            serde_json::Value::String(encrypt_enc_v1(&key, "new-token").expect("new"));
        crate::tool_store::atomic_json::write(&path, &value).expect("rewrite");

        let reread: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("reread")).expect("json");
        assert_eq!(reread["preserved"], "keep");
        assert_eq!(
            decrypt_enc_v1(&key, reread["accessToken"].as_str().expect("field")).expect("new"),
            "new-token"
        );
        let backed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&backup).expect("backup")).expect("backup json");
        assert_eq!(
            decrypt_enc_v1(&key, backed["accessToken"].as_str().expect("old field")).expect("old"),
            "old-token"
        );
        assert!(
            !dir.path()
                .join(".zcode")
                .join("v2")
                .join("credentials.json")
                .exists(),
            "default credentials file must stay absent when dataBaseDir overrides it"
        );
    }
}
