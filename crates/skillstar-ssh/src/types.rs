//! Data model for SSH remote hosts and remote skills.
//!
//! `SshHostDef` intentionally stores **only non-sensitive metadata**. Passphrases
//! and passwords live in the system keyring (see [`crate::store`]), keyed by the
//! host `id`, so the on-disk TOML is safe to back up.

use serde::{Deserialize, Serialize};

/// Authentication method for an SSH host.
///
/// - [`AuthMethod::Key`] references a private-key file **path** on the local
///   machine (the key bytes are never persisted by SkillStar). An optional
///   passphrase for that key is stored in the keyring under the host id.
/// - [`AuthMethod::Password`] authenticates with a password kept in the keyring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AuthMethod {
    /// Public-key auth using a local private key file.
    Key {
        /// Absolute or `~`-prefixed path to the private key
        /// (e.g. `~/.ssh/id_ed25519`).
        key_path: String,
    },
    /// Password auth (password itself is stored in the keyring).
    Password,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

/// A user-defined SSH remote host. Non-sensitive fields only — see crate docs.
///
/// `id` is stable across renames and is the keyring credential key. New hosts
/// should get an auto-generated id; see [`crate::store`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshHostDef {
    /// Stable unique id (e.g. `ssh_<unix_ms>`). Used as the keyring account name.
    pub id: String,
    /// Human-friendly label shown in the UI.
    pub display_name: String,
    /// Hostname or IP address.
    pub host: String,
    /// TCP port (default 22).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Remote login user.
    pub username: String,
    /// How this host authenticates.
    #[serde(default)]
    pub auth_method: AuthMethod,
    /// Default remote directory the UI opens when listing skills
    /// (e.g. `~/.claude/skills`). Empty string = no default; UI prompts.
    #[serde(default)]
    pub default_remote_dir: String,
}

fn default_port() -> u16 {
    22
}

/// A host discovered from `~/.ssh/config` (read-only, not persisted).
///
/// Surfaced in the UI as a "system" connection the user can reuse or import
/// into the managed store. `identity_file` stores only the **path** written in
/// the config (`~` left unexpanded); the bytes of the key are never read here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemHost {
    /// The `Host` alias from ssh config (e.g. `vps-yy`).
    pub alias: String,
    /// `HostName` value, falling back to the alias when the directive is absent.
    pub host: String,
    /// `Port` (default 22).
    #[serde(default = "default_port")]
    pub port: u16,
    /// `User` (empty string when the directive is absent — the server decides).
    #[serde(default)]
    pub username: String,
    /// `IdentityFile` path, unexpanded (`~` preserved). `None` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
}

/// How a remote agent skill entry is laid out relative to `~/.skillstar/hub/content`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSkillLayout {
    /// Agent dir entry is a symlink into `~/.skillstar/hub/content/<name>` and hub content exists.
    HubManaged,
    /// Real directory (or symlink elsewhere) under the agent folder — should be migrated into hub.
    #[default]
    Standalone,
}

/// A skill detected on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSkill {
    /// Skill name (the remote directory's basename).
    pub name: String,
    /// Absolute path of the skill directory on the remote host.
    pub path: String,
    /// Agent id the skill was found under (e.g. `grok`, `agents`, `claude`),
    /// derived from the `~/.<agent>/skills` parent directory.
    #[serde(default)]
    pub agent: String,
    /// Total size in bytes of the skill directory (sum of file sizes).
    pub size: u64,
    /// RFC3339 mtime of the directory, if the server reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// Whether this entry is already managed via the remote SkillStar hub layout.
    #[serde(default)]
    pub layout: RemoteSkillLayout,
}

/// Accepted host-key fingerprint entry (TOFU store).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHost {
    /// The host `id` this fingerprint was accepted for.
    pub host_id: String,
    /// `host:port` the user connected to at acceptance time.
    pub host: String,
    /// SHA-256 fingerprint of the server public key (OpenSSH format,
    /// e.g. `SHA256:base64...`).
    pub fingerprint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_method_key_roundtrips_through_json() {
        let m = AuthMethod::Key {
            key_path: "~/.ssh/id_ed25519".into(),
        };
        let json = serde_json::to_string(&m).unwrap();
        // tagged: {"kind":"key","key_path":"..."}
        assert!(json.contains("\"kind\":\"key\""));
        let back: AuthMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn auth_method_password_tag_is_lowercase() {
        let m = AuthMethod::Password;
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "{\"kind\":\"password\"}");
    }

    #[test]
    fn ssh_host_def_uses_default_port_when_missing() {
        let toml = r#"
id = "x"
display_name = "X"
host = "[IP]"
username = "root"
"#;
        let h: SshHostDef = toml::from_str(toml).unwrap();
        assert_eq!(h.port, 22);
        assert!(matches!(h.auth_method, AuthMethod::Password));
        assert_eq!(h.default_remote_dir, "");
    }

    #[test]
    fn remote_skill_layout_defaults_to_standalone() {
        let json = r#"{"name":"x","path":"/p","agent":"grok","size":0}"#;
        let s: RemoteSkill = serde_json::from_str(json).unwrap();
        assert_eq!(s.layout, RemoteSkillLayout::Standalone);
    }
}

/// Content of a remote skill's SKILL.md read from the remote hub layout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSkillContent {
    /// Skill name (basename).
    pub name: String,
    /// Raw file content (UTF-8 text of SKILL.md).
    pub content: String,
    /// RFC3339-ish mtime of the file if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// Update availability state for a remote skill (hub-managed git repo).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSkillUpdateState {
    /// Skill name.
    pub name: String,
    /// Whether `git rev-list HEAD..@{u}` reports > 0 commits.
    pub update_available: bool,
}