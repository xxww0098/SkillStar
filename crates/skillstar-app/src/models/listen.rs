//! Listen mode. The file and the bind decision live in `skillstar-gateway`.

use skillstar_gateway::{SaveListenError, listen_label, save_listen};

/// Why the listen mode was not saved. `Key` means `lan` was refused because the
/// install-level gateway key could not be loaded or created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveListenControlError {
    Store,
    Key,
}

impl std::fmt::Display for SaveListenControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Store => "listen_store",
            Self::Key => "listen_key",
        })
    }
}

/// `lan` or `loopback`.
pub fn load_listen_mode() -> String {
    listen_label().to_string()
}

/// The loopback origin agents are written with. A wildcard listen still
/// publishes loopback; this read binds nothing.
pub fn loopback_origin() -> String {
    skillstar_gateway::published_origin()
}

/// Save `lan` or `loopback`.
pub fn save_listen_mode(mode: &str) -> Result<(), SaveListenControlError> {
    save_listen(mode).map_err(|error| match error {
        SaveListenError::Store => SaveListenControlError::Store,
        SaveListenError::Key => SaveListenControlError::Key,
    })
}
