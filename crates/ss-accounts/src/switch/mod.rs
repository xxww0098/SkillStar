//! Writing one account into the client that will serve it.
//!
//! Each agent gets a vertical slice: where the client keeps its credential,
//! what shape it wants, and how to read back who the client is serving. The
//! write is deliberately blunt: write the chosen credential, verify by reading
//! it back. Every writer works on a [ClientSnapshot] captured first, so a
//! refused or unverified write puts the client's previous credential back
//! instead of leaving it logged out or half-switched.

pub mod antigravity;
pub(crate) mod antigravity_credential;
pub(crate) mod antigravity_protobuf;
pub mod claude_code;
pub mod codex;
pub mod cursor;
pub mod devin_desktop;
pub mod gemini_cli;
pub mod grok;
pub mod kiro;
pub(crate) mod kiro_store;
pub(crate) mod vscdb;
pub mod zcode;
pub(crate) mod zcode_store;
pub mod paths;

use std::path::{Path, PathBuf};

use crate::oauth::{Identity, TokenSet};

use crate::error::{AccountError, AccountResult};

pub use paths::{DesktopOs, PathRoots};

/// What a client keeps for one agent, as read from disk.
///
/// Tokens never appear here: only the identity the client shows and one-way
/// fingerprints used to answer "is it holding the credential we wrote?".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveCredential {
    /// A stable identity we can compare (an email, a provider account id).
    pub identity: Option<String>,
    /// Fingerprint of the access token the client holds, when it has one.
    pub access_fingerprint: Option<String>,
    /// Fingerprint of the refresh token the client holds, when it has one.
    pub refresh_fingerprint: Option<String>,
    /// Where it was read from, for error messages.
    pub source: String,
}

/// A client credential read in full, for importing it as an account.
pub struct LocalCredential {
    /// The tokens the client holds right now.
    pub(crate) tokens: TokenSet,
    /// The identity fields the client surfaces (email, provider account id).
    pub(crate) identity: Identity,
    /// Provider-private context the client's own files also carry (an api
    /// server, an IdC registration, the upstream a login used). Importing it
    /// keeps the account readable without asking the provider again.
    pub(crate) provider_state: Option<serde_json::Value>,
    /// Where it was read from.
    pub source: String,
}

impl std::fmt::Debug for LocalCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalCredential")
            .field("identity", &self.identity)
            .field("source", &self.source)
            .field("tokens", &"<redacted>")
            .finish()
    }
}

/// Everything a writer needs about the account it is putting into a client.
pub struct SwitchRequest<'a> {
    /// The ss-skills agent id whose client is written.
    pub agent: &'a str,
    /// The credential to write.
    pub(crate) tokens: &'a TokenSet,
    /// Provider-private state (an IdC registration, a workspace id).
    pub provider_state: Option<&'a serde_json::Value>,
    /// The name the account is shown under; never a credential.
    pub display_name: &'a str,
    /// The provider's own account id, when the record knows it.
    pub account_id: Option<&'a str>,
    /// The account's email, when the record knows it.
    pub email: Option<&'a str>,
}

impl std::fmt::Debug for SwitchRequest<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SwitchRequest")
            .field("agent", &self.agent)
            .field("display_name", &self.display_name)
            .field("account_id", &self.account_id)
            .field("email", &self.email)
            .field("tokens", &"<redacted>")
            .finish()
    }
}

/// The client's previous state, kept so a failed switch can put it back.
#[derive(Clone)]
pub enum ClientSnapshot {
    /// A client that keeps one file we own: its bytes, or its absence.
    File {
        path: PathBuf,
        previous: Option<Vec<u8>>,
    },
    /// A client that keeps key/value rows (Cursor's ItemTable): every key we
    /// write, with the value it had before (None: the key did not exist).
    Items {
        path: PathBuf,
        rows: Vec<(String, Option<String>)>,
    },
    /// A client that reads more than one plain file (Gemini CLI keeps the
    /// tokens in one and the account binding in another): every file this
    /// switch writes, with its previous bytes (None: it did not exist).
    Files {
        entries: Vec<(PathBuf, Option<Vec<u8>>)>,
    },
    /// Antigravity writes one session into a row of its state database, the
    /// macOS Keychain and two files under ~/.gemini; rolling back has to put
    /// all of them back or the client serves a half-switched session.
    Antigravity(Box<antigravity::AntigravitySnapshot>),
}

impl std::fmt::Debug for ClientSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File { path, previous } => formatter
                .debug_struct("ClientSnapshot::File")
                .field("path", path)
                .field(
                    "previous",
                    &previous.as_ref().map(|bytes| format!("<{} bytes>", bytes.len())),
                )
                .finish(),
            Self::Items { path, rows } => formatter
                .debug_struct("ClientSnapshot::Items")
                .field("path", path)
                .field(
                    "rows",
                    &rows.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>(),
                )
                .finish(),
            Self::Files { entries } => formatter
                .debug_struct("ClientSnapshot::Files")
                .field(
                    "entries",
                    &entries
                        .iter()
                        .map(|(path, previous)| {
                            (path.display().to_string(), previous.as_ref().map(Vec::len))
                        })
                        .collect::<Vec<_>>(),
                )
                .finish(),
            Self::Antigravity(snapshot) => formatter
                .debug_struct("ClientSnapshot::Antigravity")
                .field("snapshot", snapshot)
                .finish(),
        }
    }
}

/// The outcome of switching a client to another account.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    /// The agent whose client was written.
    pub agent: String,
    /// The account now served, as read back from the client.
    pub live: Option<String>,
    /// Where the client keeps it.
    pub target: String,
}

// ---------------------------------------------------------------------------
// The facade: one environment read, then the agent's own slice.
// ---------------------------------------------------------------------------

/// Where the agent's client keeps its credential, for messages.
pub fn credential_path(agent: &str) -> AccountResult<PathBuf> {
    let roots = PathRoots::from_env();
    credential_path_with(&roots, agent)
}

/// Where the agent's client keeps its credential under an injected environment.
pub fn credential_path_with(roots: &PathRoots, agent: &str) -> AccountResult<PathBuf> {
    let spec = crate::agents::spec(crate::Agent::from_id(agent)?);
    Ok((spec.switch.credential_path)(roots))
}

/// Write the tokens into the agent's client credential store.
pub fn write(request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    write_with(&PathRoots::from_env(), request)
}

/// Write the tokens under an injected environment.
pub fn write_with(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let spec = crate::agents::spec(crate::Agent::from_id(request.agent)?);
    (spec.switch.write)(roots, request)
}

/// Read who the client for the agent is serving right now.
pub fn live(agent: &str) -> AccountResult<LiveCredential> {
    live_with(&PathRoots::from_env(), agent)
}

/// Read who the client is serving under an injected environment.
pub fn live_with(roots: &PathRoots, agent: &str) -> AccountResult<LiveCredential> {
    let spec = crate::agents::spec(crate::Agent::from_id(agent)?);
    let source = (spec.switch.credential_path)(roots);
    let Some(local) = (spec.switch.read)(roots)? else {
        return Ok(LiveCredential {
            identity: None,
            access_fingerprint: None,
            refresh_fingerprint: None,
            source: source.display().to_string(),
        });
    };
    let identity = (spec.switch.live_identity)(&local.identity);
    Ok(LiveCredential {
        identity,
        access_fingerprint: Some(crate::identity::fingerprint(&local.tokens.access_token)),
        refresh_fingerprint: local
            .tokens
            .refresh_token
            .as_deref()
            .map(crate::identity::fingerprint),
        source: local.source,
    })
}

/// Read the client credential in full (this is how "add from this machine"
/// imports an account).
pub fn read(agent: &str) -> AccountResult<LocalCredential> {
    read_with(&PathRoots::from_env(), agent)
}

/// Read the client credential in full under an injected environment.
pub fn read_with(roots: &PathRoots, agent: &str) -> AccountResult<LocalCredential> {
    match read_optional_with(roots, agent)? {
        Some(local) => Ok(local),
        None => {
            let path = credential_path_with(roots, agent)?;
            Err(AccountError::Client(format!(
                "{} has no local credential at {}",
                agent,
                path.display()
            )))
        }
    }
}

fn read_optional_with(
    roots: &PathRoots,
    agent: &str,
) -> AccountResult<Option<LocalCredential>> {
    let spec = crate::agents::spec(crate::Agent::from_id(agent)?);
    (spec.switch.read)(roots)
}

/// Capture what the client holds now, so a failed write can be undone.
pub fn snapshot(agent: &str) -> AccountResult<ClientSnapshot> {
    snapshot_with(&PathRoots::from_env(), agent)
}

/// Capture the client's current credential under an injected environment.
pub fn snapshot_with(roots: &PathRoots, agent: &str) -> AccountResult<ClientSnapshot> {
    let spec = crate::agents::spec(crate::Agent::from_id(agent)?);
    (spec.switch.snapshot)(roots)
}

/// Put a captured credential back.
pub fn restore(snapshot: &ClientSnapshot) -> AccountResult<()> {
    match snapshot {
        ClientSnapshot::File { path, previous } => restore_file(path, previous),
        ClientSnapshot::Items { path, rows } => vscdb::restore_items(path, rows),
        // 多文件客户端：按写入的反序放回，先写的最后还原。
        ClientSnapshot::Files { entries } => {
            for (path, previous) in entries.iter().rev() {
                restore_file(path, previous)?;
            }
            Ok(())
        }
        ClientSnapshot::Antigravity(snapshot) => antigravity::restore(snapshot),
    }
}

// ---------------------------------------------------------------------------
// Helpers shared by the file-backed writers (Codex, Claude Code).
// ---------------------------------------------------------------------------

/// Write a credential file atomically, 0600, creating its directory.
pub(crate) fn write_restricted(path: &Path, body: &[u8]) -> AccountResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    ss_core::infra::fs_ops::atomic_write(path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Read a JSON credential file; a missing file is None, garbage is an error.
pub(crate) fn read_json_file(path: &Path) -> AccountResult<Option<serde_json::Value>> {
    let body = match std::fs::read(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AccountError::Io(error)),
    };
    serde_json::from_slice(&body).map(Some).map_err(|error| {
        AccountError::Client(format!(
            "{} is not a readable credential file: {error}",
            path.display()
        ))
    })
}

/// Capture one file's bytes (None when it does not exist yet).
pub(crate) fn snapshot_file(path: &Path) -> AccountResult<ClientSnapshot> {
    let previous = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(AccountError::Io(error)),
    };
    Ok(ClientSnapshot::File {
        path: path.to_path_buf(),
        previous,
    })
}

/// Put one file back the way it was (or remove it when it did not exist).
pub(crate) fn restore_file(path: &Path, previous: &Option<Vec<u8>>) -> AccountResult<()> {
    match previous {
        Some(bytes) => write_restricted(path, bytes),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AccountError::Io(error)),
        },
    }
}
