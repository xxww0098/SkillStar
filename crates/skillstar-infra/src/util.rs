use sha2::{Digest, Sha256};

/// Compute the SHA-256 hash of `input` and return its lowercase hex string.
pub fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    let mut hex = String::with_capacity(64);
    const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";
    for &byte in &digest {
        hex.push(HEX_TABLE[(byte >> 4) as usize] as char);
        hex.push(HEX_TABLE[(byte & 0xf) as usize] as char);
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn known_hash() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn deterministic() {
        let a = sha256_hex(b"hello world");
        let b = sha256_hex(b"hello world");
        assert_eq!(a, b);
    }
}
