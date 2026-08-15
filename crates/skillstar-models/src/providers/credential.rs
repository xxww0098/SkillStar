//! v4 credentials: a discriminated union, not a string plus a mode enum.
//!
//! ## Why a union
//!
//! The four agents SkillStar writes to do not merely *store* keys differently,
//! they mean different things by the field:
//!
//! - Codex's `env_key` holds the **name of an environment variable**.
//! - OMP's `apiKey` is looked up as a variable name first and only then used as
//!   a literal.
//! - OpenCode accepts `{env:NAME}` and `{file:PATH}` interpolation.
//! - Claude's `apiKeyHelper` is a **shell command** that prints a key.
//!
//! v3 modelled all of this as `api_key: String` plus a Codex-only
//! `codex_auth_mode: String`. That can express exactly one of the four, and it
//! put a per-agent concern (`auth_mode`) on the provider row, where it applied
//! to agents that have no such concept.
//!
//! The union also makes "there is no credential" a first-class, *explained*
//! state: [`NoCredentialReason`] distinguishes a local service (still needs a
//! base URL) from a native browser login (needs nothing at all), which is the
//! difference between showing a host field and hiding it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How to authenticate to a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    /// No credential, plus the reason there isn't one.
    ///
    /// Not `Option<Credential>`: the reason is what lets the UI answer "why is
    /// there no key here" with something better than silence, and it drives
    /// whether the connection fields are shown at all.
    None { reason: NoCredentialReason },

    /// Literal keys. Several are allowed, and the semantics are a **failover
    /// chain** (primary then fallbacks) rather than round-robin load balancing
    /// — no desktop client in the surveyed set implements the latter, and
    /// pretending otherwise would produce a setting that does nothing.
    ApiKey { keys: Vec<ApiKeyEntry> },

    /// The *name* of an environment variable. Codex `env_key`, OpenCode
    /// `{env:NAME}`, Crush `$NAME`.
    EnvVar { name: String },

    /// A file path holding the key. OpenCode `{file:PATH}`.
    File { path: String },

    /// A command that prints the key. Codex `auth.command`, OMP `!cmd`,
    /// Claude `apiKeyHelper`.
    Command { command: String, args: Vec<String> },

    /// An external CLI is already logged in and owns the credential; SkillStar
    /// does not hold it.
    ///
    /// This is what "Official / native login" *is*. Syncing such a provider
    /// means clearing the fields SkillStar manages so the CLI's own login takes
    /// over — an operation on this variant, not a string comparison against a
    /// hardcoded list of tool ids.
    ExternalCli { surface: String },
}

impl Credential {
    /// The default for a row whose credential state is not yet known — a local
    /// service with no key. Used as the serde default so a hand-edited v4 file
    /// missing `credential` still parses.
    pub fn unknown_local() -> Self {
        Credential::None {
            reason: NoCredentialReason::LocalService,
        }
    }

    /// A single literal key, the shape v3 could express.
    pub fn single_key(id: impl Into<String>, secret: impl Into<String>) -> Self {
        Credential::ApiKey {
            keys: vec![ApiKeyEntry {
                id: id.into(),
                secret: secret.into(),
                label: None,
                enabled: true,
            }],
        }
    }

    /// The key to actually send, if this credential holds one literally.
    ///
    /// Returns the first enabled entry — the head of the failover chain.
    /// `EnvVar` / `File` / `Command` deliberately return `None`: resolving
    /// those is the writer's job at sync time, and doing it here would bake a
    /// machine-specific value into the store.
    pub fn literal_secret(&self) -> Option<&str> {
        match self {
            Credential::ApiKey { keys } => keys
                .iter()
                .find(|k| k.enabled && !k.secret.is_empty())
                .map(|k| k.secret.as_str()),
            _ => None,
        }
    }

    /// Whether any secret material is present at all.
    pub fn has_secret(&self) -> bool {
        match self {
            Credential::None { .. } | Credential::ExternalCli { .. } => false,
            Credential::ApiKey { keys } => keys.iter().any(|k| !k.secret.is_empty()),
            Credential::EnvVar { name } => !name.is_empty(),
            Credential::File { path } => !path.is_empty(),
            Credential::Command { command, .. } => !command.is_empty(),
        }
    }

    /// The variant tag, for DTOs that must not carry the payload.
    pub fn kind(&self) -> CredentialKind {
        match self {
            Credential::None { .. } => CredentialKind::None,
            Credential::ApiKey { .. } => CredentialKind::ApiKey,
            Credential::EnvVar { .. } => CredentialKind::EnvVar,
            Credential::File { .. } => CredentialKind::File,
            Credential::Command { .. } => CredentialKind::Command,
            Credential::ExternalCli { .. } => CredentialKind::ExternalCli,
        }
    }

    /// A one-line description safe to send to the renderer.
    ///
    /// Literal secrets are masked here and **only** here; the plaintext never
    /// leaves the backend through a DTO. For the non-literal variants the
    /// summary is the pointer itself (a variable name, a path, a command),
    /// which is not secret and is exactly what the user needs to see to know
    /// where the key is coming from.
    pub fn summary(&self) -> String {
        match self {
            Credential::None { reason } => match reason {
                NoCredentialReason::LocalService => "local service".to_string(),
                NoCredentialReason::NativeLogin => "native login".to_string(),
            },
            Credential::ApiKey { keys } => match keys.iter().find(|k| k.enabled) {
                Some(entry) => mask_secret(&entry.secret),
                None => String::new(),
            },
            Credential::EnvVar { name } => format!("${name}"),
            Credential::File { path } => path.clone(),
            Credential::Command { command, .. } => command.clone(),
            Credential::ExternalCli { surface } => format!("{surface} login"),
        }
    }
}

/// Mask a literal key: keep enough on each end to recognise which key it is,
/// hide the rest. Short strings are replaced wholesale rather than partially
/// revealed — a 6-character key with its first 4 shown is not masked.
pub fn mask_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    match chars.len() {
        0 => String::new(),
        1..=8 => "••••••••".to_string(),
        n => {
            let head: String = chars[..4].iter().collect();
            let tail: String = chars[n - 4..].iter().collect();
            format!("{head}••••{tail}")
        }
    }
}

/// Why a provider has no credential.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "NoCredentialReason.ts")]
pub enum NoCredentialReason {
    /// Runs locally (ollama and friends). Still needs a base URL.
    LocalService,
    /// The user logs in through a browser or client. Needs nothing.
    NativeLogin,
}

/// The variant tag of a [`Credential`], with no payload.
///
/// This is what crosses into the renderer in place of the credential itself.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "CredentialKind.ts")]
pub enum CredentialKind {
    None,
    ApiKey,
    EnvVar,
    File,
    Command,
    ExternalCli,
}

/// One key in the failover chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiKeyEntry {
    /// Stable id. Editors should match by value first, then by position, and
    /// only mint a new id as a last resort — otherwise changing one character
    /// reassigns every id in the list and the UI loses track of which row the
    /// user was editing.
    pub id: String,
    /// Plaintext. **Never enters a frontend DTO** — see
    /// [`Credential::summary`].
    pub secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
