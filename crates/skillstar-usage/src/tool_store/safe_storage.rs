//! VS Code / Electron Safe Storage, ported from cockpit `vscode_inject.rs`.
//!
//! Callers inject key material. This module does not read the macOS keychain,
//! Linux `secret-tool`, or Windows DPAPI.
//!
//! Wire layout follows Chromium, not the slice's shorthand names:
//! - macOS `v10`: PBKDF2-HMAC-SHA1(password, `saltysalt`, 1003) then
//!   AES-128-CBC. The IV is sixteen ASCII spaces. The slice text says 1000
//!   rounds; Chromium and cockpit use 1003.
//! - Linux `v10`: the same CBC with 1 iteration. `peanuts` is only the
//!   basic-text password when the caller passes that literal.
//! - Linux `v11`: the same CBC and 1 iteration, with the `v11` prefix
//!   (secret-service password). This is not AES-GCM.
//! - Windows: `v10` || 12-byte nonce || AES-256-GCM (ciphertext || tag).
//!   The slice calls this scheme v11; the prefix bytes are still `v10`.
//!
//! Stored strings are standard base64 of those raw bytes.

#![cfg_attr(not(test), allow(dead_code))]

use aes::Aes128;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

/// Chromium `os_crypt_mac.mm` / cockpit macOS Safe Storage.
pub const MACOS_PBKDF2_ITERATIONS: u32 = 1003;
/// Linux basic-text (`peanuts`) and secret-service passwords.
pub const LINUX_PBKDF2_ITERATIONS: u32 = 1;
pub const V10_PREFIX: [u8; 3] = *b"v10";
pub const V11_PREFIX: [u8; 3] = *b"v11";

const SALT: &[u8] = b"saltysalt";
const CBC_IV: [u8; 16] = [b' '; 16];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyMaterial {
    /// PBKDF2-HMAC-SHA1(`password`, `saltysalt`, `iterations`) → AES-128-CBC.
    /// `prefix` is [`V10_PREFIX`] or [`V11_PREFIX`].
    Password {
        password: String,
        iterations: u32,
        prefix: [u8; 3],
    },
    /// Already-unwrapped 32-byte Windows `os_crypt` key. AES-256-GCM.
    OsCryptKey([u8; 32]),
}

impl KeyMaterial {
    pub fn macos_v10(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
            iterations: MACOS_PBKDF2_ITERATIONS,
            prefix: V10_PREFIX,
        }
    }

    pub fn linux_v10(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
            iterations: LINUX_PBKDF2_ITERATIONS,
            prefix: V10_PREFIX,
        }
    }

    pub fn linux_v11(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
            iterations: LINUX_PBKDF2_ITERATIONS,
            prefix: V11_PREFIX,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SafeStorageError {
    #[error("safe storage 密文不是合法 base64")]
    BadEncoding,
    #[error("safe storage 密文损坏或前缀不匹配")]
    Corrupt,
    #[error("safe storage 解密失败（密钥错误或密文被篡改）")]
    Decrypt,
    #[error("safe storage 加密失败")]
    Encrypt,
    #[error("safe storage 前缀必须是 v10 或 v11")]
    BadPrefix,
    #[error("safe storage PBKDF2 轮数必须大于 0")]
    BadIterations,
    #[error("Local State 缺少可解析的 os_crypt.encrypted_key")]
    LocalState,
    #[error("{0}")]
    Dpapi(String),
}

/// Encrypt `plaintext`. The returned string is standard base64 of the raw
/// Safe Storage payload (prefix included).
pub fn encrypt_secret(key: &KeyMaterial, plaintext: &[u8]) -> Result<String, SafeStorageError> {
    let raw = match key {
        KeyMaterial::Password {
            password,
            iterations,
            prefix,
        } => encrypt_password(password, *iterations, *prefix, plaintext)?,
        KeyMaterial::OsCryptKey(aes_key) => encrypt_gcm(aes_key, plaintext)?,
    };
    Ok(BASE64.encode(raw))
}

/// Decrypt a value produced by [`encrypt_secret`].
pub fn decrypt_secret(key: &KeyMaterial, encoded: &str) -> Result<Vec<u8>, SafeStorageError> {
    let raw = BASE64
        .decode(encoded.trim())
        .map_err(|_| SafeStorageError::BadEncoding)?;
    match key {
        KeyMaterial::Password {
            password,
            iterations,
            prefix,
        } => decrypt_password(password, *iterations, *prefix, &raw),
        KeyMaterial::OsCryptKey(aes_key) => decrypt_gcm(aes_key, &raw),
    }
}

/// Parse Chromium `Local State` and unwrap `os_crypt.encrypted_key`.
///
/// The DPAPI blob is recognized, then this build returns a clear error
/// without calling `CryptUnprotectData`. Tests inject [`KeyMaterial::OsCryptKey`].
pub fn unwrap_os_crypt_key_from_local_state(
    local_state_json: &str,
) -> Result<[u8; 32], SafeStorageError> {
    let blob = dpapi_blob_from_local_state(local_state_json)?;
    Err(SafeStorageError::Dpapi(dpapi_unavailable_message(
        blob.len(),
    )))
}

fn encrypt_password(
    password: &str,
    iterations: u32,
    prefix: [u8; 3],
    plaintext: &[u8],
) -> Result<Vec<u8>, SafeStorageError> {
    check_cbc_prefix(prefix)?;
    let derived = pbkdf2_sha1_key(password, iterations)?;
    let body = aes128_cbc_encrypt(&derived, plaintext)?;
    let mut out = Vec::with_capacity(prefix.len() + body.len());
    out.extend_from_slice(&prefix);
    out.extend_from_slice(&body);
    Ok(out)
}

fn decrypt_password(
    password: &str,
    iterations: u32,
    prefix: [u8; 3],
    raw: &[u8],
) -> Result<Vec<u8>, SafeStorageError> {
    check_cbc_prefix(prefix)?;
    if raw.len() < prefix.len() + 16 || !raw.starts_with(&prefix) {
        return Err(SafeStorageError::Corrupt);
    }
    let derived = pbkdf2_sha1_key(password, iterations)?;
    aes128_cbc_decrypt(&derived, &raw[prefix.len()..])
}

fn check_cbc_prefix(prefix: [u8; 3]) -> Result<(), SafeStorageError> {
    if prefix == V10_PREFIX || prefix == V11_PREFIX {
        Ok(())
    } else {
        Err(SafeStorageError::BadPrefix)
    }
}

fn pbkdf2_sha1_key(password: &str, iterations: u32) -> Result<[u8; 16], SafeStorageError> {
    if iterations == 0 {
        return Err(SafeStorageError::BadIterations);
    }
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), SALT, iterations, &mut key);
    Ok(key)
}

fn aes128_cbc_encrypt(key: &[u8; 16], plaintext: &[u8]) -> Result<Vec<u8>, SafeStorageError> {
    let cipher =
        Aes128CbcEnc::new_from_slices(key, &CBC_IV).map_err(|_| SafeStorageError::Encrypt)?;
    let mut buf = plaintext.to_vec();
    let len = buf.len();
    buf.resize(len + 16 - (len % 16), 0);
    let encrypted = cipher
        .encrypt_padded_mut::<Pkcs7>(&mut buf, len)
        .map_err(|_| SafeStorageError::Encrypt)?;
    Ok(encrypted.to_vec())
}

fn aes128_cbc_decrypt(key: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>, SafeStorageError> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return Err(SafeStorageError::Corrupt);
    }
    let cipher =
        Aes128CbcDec::new_from_slices(key, &CBC_IV).map_err(|_| SafeStorageError::Decrypt)?;
    let mut buf = ciphertext.to_vec();
    let plain = cipher
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| SafeStorageError::Decrypt)?;
    Ok(plain.to_vec())
}

fn encrypt_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, SafeStorageError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SafeStorageError::Encrypt)?;
    let mut nonce_bytes = [0u8; 12];
    for byte in &mut nonce_bytes {
        *byte = rand::random::<u8>();
    }
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| SafeStorageError::Encrypt)?;
    let mut out = Vec::with_capacity(3 + nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&V10_PREFIX);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_gcm(key: &[u8; 32], raw: &[u8]) -> Result<Vec<u8>, SafeStorageError> {
    if raw.len() < 3 + 12 + 16 || !raw.starts_with(&V10_PREFIX) {
        return Err(SafeStorageError::Corrupt);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SafeStorageError::Decrypt)?;
    cipher
        .decrypt(Nonce::from_slice(&raw[3..15]), &raw[15..])
        .map_err(|_| SafeStorageError::Decrypt)
}

fn dpapi_blob_from_local_state(local_state_json: &str) -> Result<Vec<u8>, SafeStorageError> {
    let value: serde_json::Value =
        serde_json::from_str(local_state_json).map_err(|_| SafeStorageError::LocalState)?;
    let encoded = value
        .get("os_crypt")
        .and_then(|entry| entry.get("encrypted_key"))
        .and_then(|entry| entry.as_str())
        .ok_or(SafeStorageError::LocalState)?;
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| SafeStorageError::LocalState)?;
    let Some(blob) = bytes.strip_prefix(b"DPAPI") else {
        return Err(SafeStorageError::LocalState);
    };
    if blob.is_empty() {
        return Err(SafeStorageError::LocalState);
    }
    Ok(blob.to_vec())
}

fn dpapi_unavailable_message(blob_len: usize) -> String {
    #[cfg(target_os = "windows")]
    {
        format!(
            "本构建未链接 CryptUnprotectData，未调用 DPAPI（blob {blob_len} 字节）。请注入已解开的 os_crypt key"
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!(
            "DPAPI 只能在 Windows 上解开 os_crypt key，当前平台未调用 DPAPI（blob {blob_len} 字节）"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEANUTS_VECTOR: &str = "djEwvyM1jxWVtc0IGWYGQEzc1g==";
    const MACOS_VECTOR: &str = "djEwTKbjKQSqB1hY6RayJZ8jfQ==";
    /// Node `aes-256-gcm`, key `0x11` × 32, nonce `00..=0b`, plaintext
    /// `windows-secret`, payload `v10 || nonce || ciphertext || tag`.
    const GCM_VECTOR: &str = "djEwAAECAwQFBgcICQoLZIrYdxTURtln3bDtodsKtTb9rbOD3pZ52Annd18x";

    #[test]
    fn pbkdf2_matches_chromium_vectors() {
        assert_eq!(
            pbkdf2_sha1_key("peanuts", 1).expect("peanuts"),
            [
                0xfd, 0x62, 0x1f, 0xe5, 0xa2, 0xb4, 0x02, 0x53, 0x9d, 0xfa, 0x14, 0x7c, 0xa9, 0x27,
                0x27, 0x78,
            ]
        );
        assert_eq!(
            pbkdf2_sha1_key("", 1).expect("empty"),
            [
                0xd0, 0xd0, 0xec, 0x9c, 0x7d, 0x77, 0xd4, 0x3a, 0xc5, 0x41, 0x87, 0xfa, 0x48, 0x18,
                0xd1, 0x7f,
            ]
        );
        assert_eq!(
            pbkdf2_sha1_key("test-password", MACOS_PBKDF2_ITERATIONS).expect("macos"),
            [
                0xc0, 0xff, 0xe4, 0xc2, 0x5f, 0x07, 0xf6, 0x2b, 0xfc, 0x6a, 0xb0, 0x11, 0xd9, 0xef,
                0xa5, 0x4e,
            ]
        );
        assert_ne!(
            pbkdf2_sha1_key("test-password", MACOS_PBKDF2_ITERATIONS).expect("1003"),
            pbkdf2_sha1_key("test-password", 1000).expect("1000")
        );
        assert_eq!(
            pbkdf2_sha1_key("x", 0).expect_err("zero").to_string(),
            SafeStorageError::BadIterations.to_string()
        );
    }

    #[test]
    fn v10_round_trip_matches_fixed_vectors() {
        let peanuts = KeyMaterial::linux_v10("peanuts");
        assert_eq!(
            encrypt_secret(&peanuts, b"peanuts-round").expect("encrypt peanuts"),
            PEANUTS_VECTOR
        );
        assert_eq!(
            decrypt_secret(&peanuts, PEANUTS_VECTOR).expect("decrypt peanuts"),
            b"peanuts-round"
        );

        let macos = KeyMaterial::macos_v10("test-password");
        assert_eq!(
            encrypt_secret(&macos, b"macos-secret").expect("encrypt macos"),
            MACOS_VECTOR
        );
        assert_eq!(
            decrypt_secret(&macos, MACOS_VECTOR).expect("decrypt macos"),
            b"macos-secret"
        );
    }

    #[test]
    fn linux_v11_cbc_round_trip_uses_v11_prefix() {
        let key = KeyMaterial::linux_v11("secret-tool-password");
        let encoded = encrypt_secret(&key, b"v11-body").expect("encrypt");
        assert!(
            encoded.starts_with("djEx"),
            "v11 prefix base64, got {encoded}"
        );
        assert_eq!(
            decrypt_secret(&key, &encoded).expect("decrypt"),
            b"v11-body"
        );
        let wrong_prefix =
            decrypt_secret(&KeyMaterial::linux_v10("secret-tool-password"), &encoded)
                .expect_err("v10 key rejects v11 prefix");
        assert_eq!(wrong_prefix, SafeStorageError::Corrupt);
    }

    #[test]
    fn gcm_known_vector_round_trip_and_wrong_key() {
        let key = KeyMaterial::OsCryptKey([0x11; 32]);
        assert_eq!(
            decrypt_secret(&key, GCM_VECTOR).expect("known"),
            b"windows-secret"
        );
        let encoded = encrypt_secret(&key, b"windows-secret").expect("encrypt");
        assert!(
            encoded.starts_with("djEw"),
            "Windows GCM keeps the v10 prefix, got {encoded}"
        );
        assert_eq!(
            decrypt_secret(&key, &encoded).expect("round trip"),
            b"windows-secret"
        );
        let wrong = decrypt_secret(&KeyMaterial::OsCryptKey([0x22; 32]), GCM_VECTOR)
            .expect_err("wrong key");
        assert_eq!(wrong, SafeStorageError::Decrypt);
    }

    #[test]
    fn wrong_password_and_corrupt_blob_error() {
        let err = decrypt_secret(&KeyMaterial::linux_v10("wrong-password"), PEANUTS_VECTOR)
            .expect_err("wrong password");
        assert_eq!(err, SafeStorageError::Decrypt);

        assert_eq!(
            decrypt_secret(&KeyMaterial::linux_v10("peanuts"), "!!!").expect_err("base64"),
            SafeStorageError::BadEncoding
        );
        assert_eq!(
            decrypt_secret(&KeyMaterial::linux_v10("peanuts"), "AAAA").expect_err("short"),
            SafeStorageError::Corrupt
        );

        let mut raw = BASE64.decode(GCM_VECTOR).expect("vector");
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        let flipped = BASE64.encode(raw);
        assert_eq!(
            decrypt_secret(&KeyMaterial::OsCryptKey([0x11; 32]), &flipped).expect_err("flip"),
            SafeStorageError::Decrypt
        );

        let bad_prefix = KeyMaterial::Password {
            password: "peanuts".into(),
            iterations: 1,
            prefix: *b"v12",
        };
        assert_eq!(
            encrypt_secret(&bad_prefix, b"x").expect_err("prefix"),
            SafeStorageError::BadPrefix
        );
    }

    #[test]
    fn local_state_parse_does_not_call_dpapi() {
        assert_eq!(
            unwrap_os_crypt_key_from_local_state("{").expect_err("json"),
            SafeStorageError::LocalState
        );
        assert_eq!(
            unwrap_os_crypt_key_from_local_state(r#"{"os_crypt":{}}"#).expect_err("missing"),
            SafeStorageError::LocalState
        );
        let not_dpapi = BASE64.encode(b"NOTDPAPI-blob");
        let wrong_prefix = format!(r#"{{"os_crypt":{{"encrypted_key":"{not_dpapi}"}}}}"#);
        assert_eq!(
            unwrap_os_crypt_key_from_local_state(&wrong_prefix).expect_err("prefix"),
            SafeStorageError::LocalState
        );

        let blob = BASE64.encode(b"DPAPI\x01\x02\x03\x04");
        let json = format!(r#"{{"os_crypt":{{"encrypted_key":"{blob}"}}}}"#);
        let err = unwrap_os_crypt_key_from_local_state(&json).expect_err("dpapi");
        let text = err.to_string();
        assert!(text.contains("DPAPI"), "{text}");
        assert!(text.contains("4"), "{text}");
        assert!(!text.contains("CryptUnprotectData 调用失败"), "{text}");
    }

    #[test]
    fn secret_row_round_trip_keeps_unrelated_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("table");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('unrelated', 'keep')",
            [],
        )
        .expect("sibling");
        drop(conn);

        let key_material = KeyMaterial::macos_v10("injected-password");
        let secret_key = r#"secret://{"extensionId":"example.ext","key":"token"}"#;
        let encoded = encrypt_secret(&key_material, b"alpha").expect("encrypt");
        crate::tool_store::vscdb_ext::upsert_item(&path, "safe-storage", secret_key, &encoded)
            .expect("write");
        crate::tool_store::vscdb_ext::upsert_item(
            &path,
            "safe-storage",
            "secret://other",
            "leave-this",
        )
        .expect("other");

        let stored = crate::vscdb::read_item_string(&path, secret_key)
            .expect("read")
            .expect("row");
        assert_eq!(
            decrypt_secret(&key_material, &stored).expect("decrypt"),
            b"alpha"
        );

        let updated = encrypt_secret(&key_material, b"beta").expect("re-encrypt");
        crate::tool_store::vscdb_ext::upsert_item(&path, "safe-storage", secret_key, &updated)
            .expect("rewrite");
        let rewritten = crate::vscdb::read_item_string(&path, secret_key)
            .expect("reread")
            .expect("row");
        assert_eq!(
            decrypt_secret(&key_material, &rewritten).expect("decrypt again"),
            b"beta"
        );
        assert_eq!(
            crate::vscdb::read_item_string(&path, "unrelated")
                .expect("unrelated")
                .as_deref(),
            Some("keep")
        );
        assert_eq!(
            crate::vscdb::read_item_string(&path, "secret://other")
                .expect("other")
                .as_deref(),
            Some("leave-this")
        );
    }
}
