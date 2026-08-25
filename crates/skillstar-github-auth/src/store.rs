use std::fs::{self, OpenOptions};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CredentialStore, GitHubAuthError, StoredCredential};
use skillstar_core::infra::{fs_ops, paths};

const CREDENTIAL_SCHEMA_VERSION: u32 = 1;

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

impl From<&StoredCredential> for PersistedCredential {
    fn from(value: &StoredCredential) -> Self {
        Self {
            schema_version: CREDENTIAL_SCHEMA_VERSION,
            access_token: value.access_token.clone(),
            refresh_token: value.refresh_token.clone(),
            access_expires_at: value.access_expires_at,
            refresh_expires_at: value.refresh_expires_at,
        }
    }
}

impl TryFrom<PersistedCredential> for StoredCredential {
    type Error = GitHubAuthError;

    fn try_from(value: PersistedCredential) -> Result<Self, Self::Error> {
        if value.schema_version != CREDENTIAL_SCHEMA_VERSION {
            return Err(GitHubAuthError::credential_store());
        }
        Ok(Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            access_expires_at: value.access_expires_at,
            refresh_expires_at: value.refresh_expires_at,
        })
    }
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
        StoredCredential::try_from(persisted).map(Some)
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError> {
        let encoded = serde_json::to_string(&PersistedCredential::from(credential))
            .map_err(|_| GitHubAuthError::credential_store())?;
        self.prepare_path()?;
        fs_ops::atomic_write(&self.path, encoded.as_bytes())
            .map_err(|_| GitHubAuthError::credential_store())
    }

    fn delete(&self) -> Result<(), GitHubAuthError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(GitHubAuthError::credential_store()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FileCredentialStore;
    use crate::{CredentialStore, StoredCredential};
    use chrono::Utc;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn file_store_round_trips_credentials_without_keychain() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-github-auth-file-{stamp}"));
        let path = root.join("state").join("github_auth.json");
        let store = FileCredentialStore::at(path.clone());
        let credential = StoredCredential {
            access_token: "access-secret".into(),
            refresh_token: Some("refresh-secret".into()),
            access_expires_at: Some(Utc::now()),
            refresh_expires_at: None,
        };

        store.save(&credential).expect("save file credential");
        let loaded = store
            .load()
            .expect("load file credential")
            .expect("credential exists");
        assert_eq!(loaded.access_token(), "access-secret");
        assert_eq!(loaded.refresh_token(), Some("refresh-secret"));
        assert!(path.is_file());
        #[cfg(unix)]
        assert_eq!(fs::metadata(&path).expect("metadata").permissions().mode() & 0o777, 0o600);

        store.delete().expect("delete file credential");
        assert!(store.load().expect("load deleted credential").is_none());
        let _ = fs::remove_dir_all(root);
    }
}
