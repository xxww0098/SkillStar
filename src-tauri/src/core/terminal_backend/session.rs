use sha2::{Digest, Sha256};

/// Generate a deterministic session/script identifier for a project.
pub(crate) fn session_name(project_name: &str) -> String {
    let sanitized: String = project_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(project_name.as_bytes());
    let hash_bytes = hasher.finalize();
    let hash: String = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let short_hash = &hash[..6];
    format!("ss-{}-{}", sanitized, short_hash)
}
