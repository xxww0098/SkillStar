//! iCube `byte_crypto` blobs from Trae `storage.json`.
//!
//! Ported from cockpit `trae_account_core_platform_storage.rs`. The on-disk
//! value is standard base64 of this blob; encode/decode here are the raw bytes.
//! Layout: 6-byte header || 32-byte random key || AES-128-CBC(SHA-512(plain) || plain).
//! The AES key and IV are SHA-512(SHA-512(random key) || salt) split into
//! 16 + 16. A flipped ciphertext byte fails the SHA-512 check or PKCS7.

#![cfg_attr(not(test), allow(dead_code))]

use aes::Aes128;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use sha2::{Digest, Sha512};

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

const BLOCK: usize = 16;
const HEADER_LEN: usize = 6;
const SHA512_LEN: usize = 64;
const RANDOM_KEY_LEN: usize = 32;
const PREFIX_AES: [u8; HEADER_LEN] = [116, 99, 5, 16, 0, 0];
const PREFIX_AES_PRIVATE: [u8; HEADER_LEN] = [18, 57, 32, 32, 2, 3];

const AES_PRIVATE_A: [u8; SHA512_LEN] = [
    191, 192, 216, 250, 122, 246, 220, 97, 31, 254, 98, 27, 8, 72, 71, 176, 135, 99, 96, 18, 127,
    101, 203, 104, 211, 102, 191, 125, 37, 72, 150, 156, 51, 229, 121, 35, 17, 153, 141, 177, 110,
    131, 150, 128, 172, 255, 254, 6, 18, 140, 55, 62, 236, 249, 135, 64, 135, 12, 117, 4, 89, 149,
    168, 209,
];
const AES_PRIVATE_B: [u8; SHA512_LEN] = [
    246, 204, 26, 232, 232, 70, 129, 109, 223, 146, 169, 242, 23, 241, 105, 145, 50, 196, 165, 42,
    254, 120, 3, 54, 244, 207, 209, 85, 53, 6, 138, 106, 175, 148, 31, 204, 186, 186, 165, 182, 87,
    142, 49, 10, 39, 110, 26, 154, 86, 56, 173, 125, 18, 64, 198, 225, 99, 99, 83, 82, 191, 134,
    76, 170,
];
const AES_A: [u8; SHA512_LEN] = [
    82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
    155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
    238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73, 109,
    139, 209, 37,
];
const AES_B: [u8; SHA512_LEN] = [
    31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39, 128, 236, 95, 96, 81, 127, 169, 25,
    181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160, 224, 59, 77, 174, 42, 245, 176, 200,
    235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119, 214, 38, 225, 105, 20, 99, 85, 33,
    12, 125,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Version {
    Aes,
    AesPrivate,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ByteCryptoError {
    #[error("byte_crypto 密文格式无效")]
    Format,
    #[error("byte_crypto 完整性校验失败")]
    Integrity,
    #[error("byte_crypto 加密失败")]
    Encrypt,
}

/// Encrypt with the public iCube header (`tc` / version 1).
pub fn encode(plaintext: &[u8]) -> Result<Vec<u8>, ByteCryptoError> {
    encode_with(plaintext, Version::Aes)
}

/// Decrypt an iCube blob. Accepts both the public and private headers.
pub fn decode(raw: &[u8]) -> Result<Vec<u8>, ByteCryptoError> {
    if raw.len() <= HEADER_LEN + RANDOM_KEY_LEN {
        return Err(ByteCryptoError::Format);
    }
    let version = version_from_header(&raw[..HEADER_LEN]).ok_or(ByteCryptoError::Format)?;
    let key_end = HEADER_LEN + RANDOM_KEY_LEN;
    let key_material = &raw[HEADER_LEN..key_end];
    let ciphertext = &raw[key_end..];
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(BLOCK) {
        return Err(ByteCryptoError::Format);
    }
    let (aes_key, iv) = derive_key_iv(key_material, version).ok_or(ByteCryptoError::Format)?;
    let decrypted = aes128_cbc_decrypt(&aes_key, &iv, ciphertext)?;
    if decrypted.len() < SHA512_LEN {
        return Err(ByteCryptoError::Integrity);
    }
    let digest = sha512(&decrypted[SHA512_LEN..]);
    if digest.as_slice() != &decrypted[..SHA512_LEN] {
        return Err(ByteCryptoError::Integrity);
    }
    Ok(decrypted[SHA512_LEN..].to_vec())
}

fn encode_with(plaintext: &[u8], version: Version) -> Result<Vec<u8>, ByteCryptoError> {
    let mut random_key = [0u8; RANDOM_KEY_LEN];
    for byte in &mut random_key {
        *byte = rand::random::<u8>();
    }
    let (aes_key, iv) = derive_key_iv(&random_key, version).ok_or(ByteCryptoError::Encrypt)?;
    let mut payload = Vec::with_capacity(SHA512_LEN + plaintext.len());
    payload.extend_from_slice(&sha512(plaintext));
    payload.extend_from_slice(plaintext);
    let encrypted = aes128_cbc_encrypt(&aes_key, &iv, &payload)?;
    let mut out = Vec::with_capacity(HEADER_LEN + RANDOM_KEY_LEN + encrypted.len());
    out.extend_from_slice(header(version));
    out.extend_from_slice(&random_key);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

fn version_from_header(header: &[u8]) -> Option<Version> {
    if header == PREFIX_AES {
        Some(Version::Aes)
    } else if header == PREFIX_AES_PRIVATE {
        Some(Version::AesPrivate)
    } else {
        None
    }
}

fn header(version: Version) -> &'static [u8; HEADER_LEN] {
    match version {
        Version::Aes => &PREFIX_AES,
        Version::AesPrivate => &PREFIX_AES_PRIVATE,
    }
}

fn salt(version: Version) -> [u8; SHA512_LEN] {
    let (left, right) = match version {
        Version::AesPrivate => (AES_PRIVATE_A, AES_PRIVATE_B),
        Version::Aes => (AES_A, AES_B),
    };
    let mut out = [0u8; SHA512_LEN];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = left[index] ^ right[index];
    }
    out
}

fn derive_key_iv(key_material: &[u8], version: Version) -> Option<([u8; 16], [u8; 16])> {
    if key_material.len() != RANDOM_KEY_LEN {
        return None;
    }
    let mut merge = [0u8; SHA512_LEN * 2];
    merge[..SHA512_LEN].copy_from_slice(&sha512(key_material));
    merge[SHA512_LEN..].copy_from_slice(&salt(version));
    let merged = sha512(&merge);
    let mut aes_key = [0u8; 16];
    let mut iv = [0u8; 16];
    aes_key.copy_from_slice(&merged[..16]);
    iv.copy_from_slice(&merged[16..32]);
    Some((aes_key, iv))
}

fn sha512(data: &[u8]) -> [u8; SHA512_LEN] {
    let digest = Sha512::digest(data);
    let mut out = [0u8; SHA512_LEN];
    out.copy_from_slice(&digest);
    out
}

fn aes128_cbc_encrypt(
    key: &[u8; 16],
    iv: &[u8; 16],
    plaintext: &[u8],
) -> Result<Vec<u8>, ByteCryptoError> {
    let cipher = Aes128CbcEnc::new_from_slices(key, iv).map_err(|_| ByteCryptoError::Encrypt)?;
    let mut buf = plaintext.to_vec();
    let len = buf.len();
    buf.resize(len + BLOCK - (len % BLOCK), 0);
    let encrypted = cipher
        .encrypt_padded_mut::<Pkcs7>(&mut buf, len)
        .map_err(|_| ByteCryptoError::Encrypt)?;
    Ok(encrypted.to_vec())
}

fn aes128_cbc_decrypt(
    key: &[u8; 16],
    iv: &[u8; 16],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ByteCryptoError> {
    let cipher = Aes128CbcDec::new_from_slices(key, iv).map_err(|_| ByteCryptoError::Integrity)?;
    let mut buf = ciphertext.to_vec();
    let plain = cipher
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| ByteCryptoError::Integrity)?;
    Ok(plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_flipped_byte_fails_integrity() {
        let plain = br#"{"user":"tag"}"#;
        let blob = encode(plain).expect("encode");
        assert!(blob.starts_with(&PREFIX_AES));
        assert_eq!(decode(&blob).expect("decode"), plain);

        let mut tampered = blob.clone();
        let index = tampered.len() - 1;
        tampered[index] ^= 0x01;
        assert_eq!(
            decode(&tampered).expect_err("flip"),
            ByteCryptoError::Integrity
        );
    }

    #[test]
    fn private_header_round_trip_and_bad_header_is_format() {
        let plain = b"private-header";
        let blob = encode_with(plain, Version::AesPrivate).expect("encode");
        assert!(blob.starts_with(&PREFIX_AES_PRIVATE));
        assert_eq!(decode(&blob).expect("decode"), plain);

        let mut bad = blob.clone();
        bad[0] ^= 0xff;
        assert_eq!(decode(&bad).expect_err("header"), ByteCryptoError::Format);
        assert_eq!(
            decode(&[0, 1, 2]).expect_err("short"),
            ByteCryptoError::Format
        );
    }
}
