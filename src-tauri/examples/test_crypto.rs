use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use sha2::{Digest, Sha256};

fn get_encryption_key() -> Key<Aes256Gcm> {
    let machine_id = machine_uid::get().unwrap_or_else(|_| "skillstar-fallback-id-123".to_string());
    let mut hasher = Sha256::new();
    hasher.update(b"skillstar-ai-api-key");
    hasher.update(machine_id.as_bytes());
    let result = hasher.finalize();
    *Key::<Aes256Gcm>::from_slice(result.as_slice())
}

fn encrypt(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
    match cipher.encrypt(&nonce, plain.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(combined)
        }
        Err(_) => plain.to_string(), // fallback
    }
}

fn decrypt(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    let Ok(decoded) = BASE64.decode(encoded) else {
        // Assume it was not encrypted or legacy plaintext
        return encoded.to_string();
    };
    if decoded.len() < 12 {
        return encoded.to_string();
    }
    let (nonce_bytes, ciphertext) = decoded.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_else(|_| encoded.to_string()),
        Err(_) => {
            // Decryption failed. Possibly legacy unencrypted key or changed machine id.
            // Return original encoded string (it might be the raw old key).
            encoded.to_string()
        }
    }
}

fn main() {
    let plain = "sk-test-abcde-12345";
    let enc = encrypt(plain);
    let dec = decrypt(&enc);
    println!("Plain: {}", plain);
    println!("Enc: {}", enc);
    println!("Dec: {}", dec);
    assert_eq!(plain, dec);
}
