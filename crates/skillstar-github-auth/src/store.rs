use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CredentialStore, GitHubAuthError, StoredCredential};

const KEYRING_SERVICE: &str = "skillstar-github-auth";
const KEYRING_ACCOUNT: &str = "github.com";
const CREDENTIAL_SCHEMA_VERSION: u32 = 1;

pub struct KeyringCredentialStore;

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

impl KeyringCredentialStore {
    fn entry() -> Result<keyring::Entry, GitHubAuthError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|_| GitHubAuthError::credential_store())
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn load(&self) -> Result<Option<StoredCredential>, GitHubAuthError> {
        let encoded = match Self::entry()?.get_password() {
            Ok(encoded) => encoded,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(_) => return Err(GitHubAuthError::credential_store()),
        };
        let persisted: PersistedCredential =
            serde_json::from_str(&encoded).map_err(|_| GitHubAuthError::credential_store())?;
        StoredCredential::try_from(persisted).map(Some)
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError> {
        let encoded = serde_json::to_string(&PersistedCredential::from(credential))
            .map_err(|_| GitHubAuthError::credential_store())?;
        Self::entry()?
            .set_password(&encoded)
            .map_err(|_| GitHubAuthError::credential_store())
    }

    fn delete(&self) -> Result<(), GitHubAuthError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(GitHubAuthError::credential_store()),
        }
    }
}
