//! Listen mode. The file and the bind decision live in `skillstar-gateway`.

use skillstar_gateway::{SaveListenError, listen_label, save_listen};

/// Why the listen mode was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveListenControlError {
    Store,
}

impl std::fmt::Display for SaveListenControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Store => "listen_store",
        })
    }
}

/// `lan` or `loopback`.
pub fn load_listen_mode() -> String {
    listen_label().to_string()
}

/// Save `lan` or `loopback`.
pub fn save_listen_mode(mode: &str) -> Result<(), SaveListenControlError> {
    save_listen(mode).map_err(|error| match error {
        SaveListenError::Store => SaveListenControlError::Store,
    })
}
