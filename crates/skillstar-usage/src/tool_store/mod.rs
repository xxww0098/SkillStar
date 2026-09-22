//! Local credential IO: VS Code item tables, atomic JSON, and macOS
//! internet-password keychain access. No provider meaning.

pub mod atomic_json;
pub mod byte_crypto;
pub mod enc_v1;
pub mod keychain_cli;
pub mod safe_storage;
pub mod vscdb_ext;
