//! Managed + system host listing and CRUD commands.

use std::collections::HashSet;

use skillstar_core::infra::error::AppError;
use skillstar_ssh::store::{KeyringSecretStore, load_hosts};
use skillstar_ssh::{HostsStore, SshHostDef, SystemHost, parse_system_hosts};

use super::whoami_username;

/// A host entry surfaced in the SSH page list.
///
/// `Managed` hosts live in `ssh_hosts.toml` (editable/deletable). `System`
/// hosts are discovered from `~/.ssh/config` (read-only, importable).
#[derive(Debug, serde::Serialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum SshHostListItem {
    Managed(SshHostDef),
    System(SystemHost),
}

#[tauri::command]
pub async fn list_ssh_hosts() -> Result<Vec<SshHostListItem>, AppError> {
    let managed = load_hosts();
    // De-dup: a system host whose HostName matches a managed host's `host`
    // is already in the user's library, so don't show it twice.
    let managed_hosts: HashSet<String> = managed.iter().map(|h| h.host.clone()).collect();
    let system = parse_system_hosts()
        .into_iter()
        .filter(|s| !managed_hosts.contains(&s.host))
        .map(SshHostListItem::System);

    Ok(managed
        .into_iter()
        .map(SshHostListItem::Managed)
        .chain(system)
        .collect())
}

/// Import a system-discovered host (from `~/.ssh/config`) into the managed
/// store so it becomes editable and gains a `default_remote_dir`. Reuses the
/// system IdentityFile path as the auth method.
#[tauri::command]
pub async fn import_system_host(alias: String) -> Result<SshHostDef, AppError> {
    let sys = skillstar_ssh::find_host_by_alias(&alias)
        .ok_or_else(|| AppError::Ssh(format!("system host '{alias}' not found")))?;
    let def = SshHostDef {
        id: String::new(),
        display_name: sys.alias.clone(),
        host: sys.host,
        port: sys.port,
        username: if sys.username.is_empty() {
            whoami_username()
        } else {
            sys.username
        },
        auth_method: match sys.identity_file {
            Some(path) => skillstar_ssh::AuthMethod::Key { key_path: path },
            None => skillstar_ssh::AuthMethod::Password,
        },
        default_remote_dir: String::new(),
    };
    let store = HostsStore::new(KeyringSecretStore);
    store.add(def, None).map_err(|e| AppError::Ssh(e.to_string()))
}

/// Add a new SSH host. `credential` is the passphrase (for key auth) or
/// password (for password auth); it is stored in the keyring and discarded.
#[tauri::command]
pub async fn add_ssh_host(
    def: SshHostDef,
    credential: Option<String>,
) -> Result<SshHostDef, AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store
        .add(def, credential.as_deref())
        .map_err(|e| AppError::Ssh(e.to_string()))
}

#[tauri::command]
pub async fn update_ssh_host(
    id: String,
    def: SshHostDef,
    credential: Option<String>,
) -> Result<(), AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store
        .update(&id, def, credential.as_deref())
        .map_err(|e| AppError::Ssh(e.to_string()))
}

#[tauri::command]
pub async fn delete_ssh_host(id: String) -> Result<(), AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store.remove(&id).map_err(|e| AppError::Ssh(e.to_string()))
}
