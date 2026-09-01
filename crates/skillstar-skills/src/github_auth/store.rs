use std::fs::{self, OpenOptions};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CredentialStore, GitHubAuthError, StoredCredential};
use skillstar_core::infra::{fs_ops, paths};

/// Current on-disk format: AES-256-GCM sealed tokens, no OS keychain.
const CREDENTIAL_SCHEMA_VERSION: u32 = 2;
/// First shipping format: plaintext tokens in the same JSON shape.
const LEGACY_PLAINTEXT_SCHEMA: u32 = 1;
const KEY_NAMESPACE: &[u8] = b"skillstar-github-auth";

pub struct FileCredentialStore {
    path: PathBuf,
}

impl Default for FileCredentialStore {
    fn default() -> Self {
        Self {
            path: paths::github_auth_path(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedCredential {
    schema_version: u32,
    access_token: String,
    refresh_token: Option<String>,
    access_expires_at: Option<DateTime<Utc>>,
    refresh_expires_at: Option<DateTime<Utc>>,
}

impl FileCredentialStore {
    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { path }
    }

    fn prepare_path(&self) -> Result<(), GitHubAuthError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| GitHubAuthError::credential_store())?;
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&self.path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(GitHubAuthError::credential_store()),
        }

        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
            .map_err(|_| GitHubAuthError::credential_store())?;
        Ok(())
    }

    fn persist(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError> {
        let encoded = serde_json::to_string(&PersistedCredential {
            schema_version: CREDENTIAL_SCHEMA_VERSION,
            access_token: seal(&credential.access_token)?,
            refresh_token: credential
                .refresh_token
                .as_deref()
                .filter(|token| !token.is_empty())
                .map(seal)
                .transpose()?,
            access_expires_at: credential.access_expires_at,
            refresh_expires_at: credential.refresh_expires_at,
        })
        .map_err(|_| GitHubAuthError::credential_store())?;
        self.prepare_path()?;
        fs_ops::atomic_write(&self.path, encoded.as_bytes())
            .map_err(|_| GitHubAuthError::credential_store())
    }
}

impl CredentialStore for FileCredentialStore {
    fn load(&self) -> Result<Option<StoredCredential>, GitHubAuthError> {
        let encoded = match fs::read_to_string(&self.path) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(GitHubAuthError::credential_store()),
        };
        let persisted: PersistedCredential =
            serde_json::from_str(&encoded).map_err(|_| GitHubAuthError::credential_store())?;
        let credential = match persisted.schema_version {
            CREDENTIAL_SCHEMA_VERSION => StoredCredential {
                access_token: open(&persisted.access_token)?,
                refresh_token: persisted.refresh_token.as_deref().map(open).transpose()?,
                access_expires_at: persisted.access_expires_at,
                refresh_expires_at: persisted.refresh_expires_at,
            },
            LEGACY_PLAINTEXT_SCHEMA => StoredCredential {
                access_token: persisted.access_token,
                refresh_token: persisted.refresh_token,
                access_expires_at: persisted.access_expires_at,
                refresh_expires_at: persisted.refresh_expires_at,
            },
            _ => return Err(GitHubAuthError::credential_store()),
        };
        if persisted.schema_version == LEGACY_PLAINTEXT_SCHEMA {
            // Best-effort upgrade so a leftover plaintext file does not stay
            // readable after the next successful load. Failure here still
            // returns the in-memory credential for this process.
            let _ = self.persist(&credential);
        }
        Ok(Some(credential))
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError> {
        self.persist(credential)
    }

    fn delete(&self) -> Result<(), GitHubAuthError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(GitHubAuthError::credential_store()),
        }
    }
}

fn encryption_key() -> [u8; 32] {
    let uid = machine_uid::get().unwrap_or_else(|_| "skillstar-fallback-id-123".into());
    let mut hash = Sha256::new();
    hash.update(KEY_NAMESPACE);
    hash.update(uid.as_bytes());
    hash.finalize().into()
}

fn seal(plaintext: &str) -> Result<String, GitHubAuthError> {
    if plaintext.is_empty() {
        return Err(GitHubAuthError::credential_store());
    }
    let key = encryption_key();
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| GitHubAuthError::credential_store())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| GitHubAuthError::credential_store())?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(combined))
}

fn open(encoded: &str) -> Result<String, GitHubAuthError> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|_| GitHubAuthError::credential_store())?;
    // 12-byte nonce + 16-byte GCM tag; anything shorter is not our ciphertext.
    if combined.len() < 28 {
        return Err(GitHubAuthError::credential_store());
    }
    let key = encryption_key();
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| GitHubAuthError::credential_store())?;
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| GitHubAuthError::credential_store())?;
    String::from_utf8(plaintext).map_err(|_| GitHubAuthError::credential_store())
}

#[cfg(test)]
mod tests {
    use super::{CredentialStore, FileCredentialStore, StoredCredential};
    use chrono::Utc;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-github-auth-{label}-{stamp}"));
        let path = root.join("state").join("github_auth.json");
        (root, path)
    }

    fn sample() -> StoredCredential {
        StoredCredential {
            access_token: "access-secret".into(),
            refresh_token: Some("refresh-secret".into()),
            access_expires_at: Some(Utc::now()),
            refresh_expires_at: None,
        }
    }

    #[test]
    fn file_store_round_trips_encrypted_json_without_keychain() {
        let (root, path) = unique_path("sealed");
        let store = FileCredentialStore::at(path.clone());
        let credential = sample();

        store.save(&credential).expect("save file credential");
        let loaded = store
            .load()
            .expect("load file credential")
            .expect("credential exists");
        assert_eq!(loaded.access_token(), "access-secret");
        assert_eq!(loaded.refresh_token(), Some("refresh-secret"));
        let on_disk = fs::read_to_string(&path).expect("read sealed file");
        assert!(!on_disk.contains("access-secret"), "{on_disk}");
        assert!(!on_disk.contains("refresh-secret"), "{on_disk}");
        assert!(on_disk.contains("\"schema_version\":2"));
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );

        store.delete().expect("delete file credential");
        assert!(store.load().expect("load deleted credential").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plaintext_v1_file_is_rewritten_as_encrypted_json() {
        let (root, path) = unique_path("legacy");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(
            &path,
            r#"{"schema_version":1,"access_token":"access-secret","refresh_token":"refresh-secret"}"#,
        )
        .expect("write legacy plaintext");

        let store = FileCredentialStore::at(path.clone());
        let loaded = store
            .load()
            .expect("load legacy credential")
            .expect("credential exists");
        assert_eq!(loaded.access_token(), "access-secret");
        let on_disk = fs::read_to_string(&path).expect("read upgraded file");
        assert!(!on_disk.contains("access-secret"), "{on_disk}");
        assert!(on_disk.contains("\"schema_version\":2"));
        let reloaded = store
            .load()
            .expect("reload upgraded credential")
            .expect("credential exists");
        assert_eq!(reloaded.access_token(), "access-secret");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_ciphertext_does_not_look_signed_in() {
        let (root, path) = unique_path("corrupt");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(
            &path,
            r#"{"schema_version":2,"access_token":"AAAA","refresh_token":null}"#,
        )
        .expect("write corrupt ciphertext");

        let store = FileCredentialStore::at(path);
        assert!(store.load().is_err());
        let _ = fs::remove_dir_all(root);
    }
}
