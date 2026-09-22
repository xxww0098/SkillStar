//! P-256 device key and `DeviceProof` signature for Trae `ExchangeToken`.
//!
//! Ported from cockpit `trae_oauth.rs` / `trae_account_core_refresh.rs`.
//! No network. The signature string is standard base64 of the ASN.1 ECDSA
//! signature (cockpit's `DeviceProof.Signature`). `ring` draws a fresh nonce
//! inside `EcdsaKeyPair::sign`, so the same message does not produce the
//! same signature bytes. Callers verify; they do not compare signatures.
//!
//! Message bytes are exactly
//! `{method}\n{path}\n{client_id}\n{refresh_token}\n{ts}\n{nonce}`.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};

const P256_SPKI_PREFIX: &[u8] = &[
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKeyPair {
    pub private_pem: String,
    pub public_pem: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceProofError {
    #[error("生成 Trae 设备密钥失败")]
    Generate,
    #[error("解析 Trae 设备私钥 PEM 失败")]
    PrivateKey,
    #[error("生成 Trae 设备签名失败")]
    Sign,
    #[error("Trae 设备公钥格式无效")]
    PublicKey,
}

pub fn generate_device_keypair() -> Result<DeviceKeyPair, DeviceProofError> {
    let rng = SystemRandom::new();
    let private_pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
        .map_err(|_| DeviceProofError::Generate)?;
    let pair = EcdsaKeyPair::from_pkcs8(
        &ECDSA_P256_SHA256_ASN1_SIGNING,
        private_pkcs8.as_ref(),
        &rng,
    )
    .map_err(|_| DeviceProofError::Generate)?;
    let public_der = p256_spki_der(pair.public_key().as_ref())?;
    Ok(DeviceKeyPair {
        private_pem: pem_wrap("PRIVATE KEY", private_pkcs8.as_ref()),
        public_pem: pem_wrap("PUBLIC KEY", &public_der),
    })
}

/// Sign `device_proof_message(...)`. Returns standard base64 of the ASN.1 signature.
pub fn sign_device_proof(
    priv_pem: &str,
    method: &str,
    path: &str,
    client_id: &str,
    refresh_token: &str,
    ts: i64,
    nonce: &str,
) -> Result<String, DeviceProofError> {
    let message = device_proof_message(method, path, client_id, refresh_token, ts, nonce);
    let private_der = pem_to_der(priv_pem)?;
    let rng = SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &private_der, &rng)
        .map_err(|_| DeviceProofError::PrivateKey)?;
    let signature = pair
        .sign(&rng, message.as_bytes())
        .map_err(|_| DeviceProofError::Sign)?;
    Ok(BASE64.encode(signature.as_ref()))
}

pub fn device_proof_message(
    method: &str,
    path: &str,
    client_id: &str,
    refresh_token: &str,
    ts: i64,
    nonce: &str,
) -> String {
    format!("{method}\n{path}\n{client_id}\n{refresh_token}\n{ts}\n{nonce}")
}

fn p256_spki_der(public_key: &[u8]) -> Result<Vec<u8>, DeviceProofError> {
    if public_key.len() != 65 || public_key.first().copied() != Some(0x04) {
        return Err(DeviceProofError::PublicKey);
    }
    let mut der = Vec::with_capacity(P256_SPKI_PREFIX.len() + public_key.len());
    der.extend_from_slice(P256_SPKI_PREFIX);
    der.extend_from_slice(public_key);
    Ok(der)
}

fn pem_wrap(label: &str, der: &[u8]) -> String {
    let encoded = BASE64.encode(der);
    let mut pem = String::new();
    pem.push_str("-----BEGIN ");
    pem.push_str(label);
    pem.push_str("-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
        pem.push('\n');
    }
    pem.push_str("-----END ");
    pem.push_str(label);
    pem.push_str("-----\n");
    pem
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>, DeviceProofError> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("-----"))
        .collect();
    BASE64
        .decode(body)
        .map_err(|_| DeviceProofError::PrivateKey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};

    fn public_point(pem: &str) -> Vec<u8> {
        let der = pem_to_der(pem).expect("pem");
        let point = der
            .strip_prefix(P256_SPKI_PREFIX)
            .expect("spki prefix")
            .to_vec();
        assert_eq!(point.len(), 65);
        point
    }

    #[test]
    fn fixed_nonce_and_timestamp_verify_but_a_different_message_does_not() {
        let pair = generate_device_keypair().expect("keypair");
        assert!(pair.private_pem.contains("BEGIN PRIVATE KEY"));
        assert!(pair.public_pem.contains("BEGIN PUBLIC KEY"));

        let method = "POST";
        let path = "/trae/api/v3/oauth/ExchangeToken";
        let client_id = "client-1";
        let refresh_token = "refresh-token";
        let ts = 1_700_000_000;
        let nonce = "00112233445566778899aabbccddeeff";
        let message = device_proof_message(method, path, client_id, refresh_token, ts, nonce);
        assert_eq!(
            message,
            "POST\n/trae/api/v3/oauth/ExchangeToken\nclient-1\nrefresh-token\n1700000000\n00112233445566778899aabbccddeeff"
        );

        let signature = sign_device_proof(
            &pair.private_pem,
            method,
            path,
            client_id,
            refresh_token,
            ts,
            nonce,
        )
        .expect("sign");
        let signature_bytes = BASE64.decode(&signature).expect("sig b64");
        let point = public_point(&pair.public_pem);
        let public_key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &point);
        public_key
            .verify(message.as_bytes(), &signature_bytes)
            .expect("verify");

        let tampered = message.replacen("client-1", "client-2", 1);
        assert!(
            public_key
                .verify(tampered.as_bytes(), &signature_bytes)
                .is_err()
        );
    }

    #[test]
    fn bad_private_pem_is_rejected() {
        let err = sign_device_proof("not a pem", "POST", "/p", "c", "r", 1, "n").expect_err("pem");
        assert_eq!(err, DeviceProofError::PrivateKey);
    }
}
