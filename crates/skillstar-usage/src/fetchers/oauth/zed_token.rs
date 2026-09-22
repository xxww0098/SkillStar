//! Zed OAuth callback token decrypt. Not a fetcher and not in `dispatch`.
//!
//! Cockpit tries OAEP-SHA256 first, then PKCS#1 v1.5. The server picks the
//! padding. `priv_der` is PKCS#1 DER, not PKCS#8. The ciphertext is base64
//! (URL-safe without padding, matching cockpit, plus the padded variants).

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPrivateKey};
use sha2::Sha256;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZedTokenError {
    #[error("解析 Zed RSA 私钥失败")]
    PrivateKey,
    #[error("解析 Zed access_token 密文失败")]
    Encoding,
    #[error("解密 Zed access_token 失败")]
    Decrypt,
    #[error("Zed access_token 不是有效 UTF-8")]
    Utf8,
}

/// Decrypt the callback `access_token`. OAEP-SHA256 first, then PKCS#1 v1.5.
pub fn decrypt_zed_token(priv_der: &[u8], ciphertext_b64: &str) -> Result<String, ZedTokenError> {
    let private_key =
        RsaPrivateKey::from_pkcs1_der(priv_der).map_err(|_| ZedTokenError::PrivateKey)?;
    let encrypted = decode_ciphertext(ciphertext_b64)?;
    let plain = private_key
        .decrypt(Oaep::<Sha256>::new(), &encrypted)
        .or_else(|_| private_key.decrypt(Pkcs1v15Encrypt, &encrypted))
        .map_err(|_| ZedTokenError::Decrypt)?;
    String::from_utf8(plain).map_err(|_| ZedTokenError::Utf8)
}

fn decode_ciphertext(value: &str) -> Result<Vec<u8>, ZedTokenError> {
    let trimmed = value.trim();
    URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| URL_SAFE.decode(trimmed))
        .or_else(|_| STANDARD.decode(trimmed))
        .map_err(|_| ZedTokenError::Encoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::RsaPublicKey;
    use rsa::pkcs1::EncodeRsaPrivateKey;

    #[test]
    fn oaep_sha256_and_pkcs1_round_trip() {
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa-2048");
        let public_key = RsaPublicKey::from(&private_key);
        let der = private_key.to_pkcs1_der().expect("pkcs1");
        let token = "zed-access-token";

        let oaep = public_key
            .encrypt(&mut rng, Oaep::<Sha256>::new(), token.as_bytes())
            .expect("oaep encrypt");
        let oaep_b64 = URL_SAFE_NO_PAD.encode(&oaep);
        assert_eq!(
            decrypt_zed_token(der.as_bytes(), &oaep_b64).expect("oaep"),
            token
        );
        let oaep_standard = STANDARD.encode(&oaep);
        assert_eq!(
            decrypt_zed_token(der.as_bytes(), &oaep_standard).expect("oaep standard"),
            token
        );

        let pkcs1 = public_key
            .encrypt(&mut rng, Pkcs1v15Encrypt, token.as_bytes())
            .expect("pkcs1 encrypt");
        let pkcs1_b64 = URL_SAFE_NO_PAD.encode(&pkcs1);
        assert_eq!(
            decrypt_zed_token(der.as_bytes(), &pkcs1_b64).expect("pkcs1"),
            token
        );
    }

    #[test]
    fn bad_key_and_bad_ciphertext_error() {
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa-2048");
        let der = private_key.to_pkcs1_der().expect("pkcs1");
        assert_eq!(
            decrypt_zed_token(b"not-a-key", "AAAA").expect_err("der"),
            ZedTokenError::PrivateKey
        );
        assert_eq!(
            decrypt_zed_token(der.as_bytes(), "!!!").expect_err("b64"),
            ZedTokenError::Encoding
        );
        let garbage = URL_SAFE_NO_PAD.encode([0u8; 32]);
        assert_eq!(
            decrypt_zed_token(der.as_bytes(), &garbage).expect_err("padding"),
            ZedTokenError::Decrypt
        );
    }
}
