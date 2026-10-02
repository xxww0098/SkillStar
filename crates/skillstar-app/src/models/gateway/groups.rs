//! Saved routing groups. The write is `skillstar-gateway`'s `save_group`.
//!
//! This module does not decide whether a member list cycles. A refusal comes
//! back as that function's error, and the gateway file is unchanged.

use serde::{Deserialize, Serialize};
use skillstar_gateway::{SaveGroupError, save_group, stored_groups};

/// One group already written to `model_gateway.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedGroupDto {
    pub id: String,
    pub members: Vec<String>,
}

/// Why `save_group` refused. The text is what the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveGroupControlError {
    Cycle,
    TooDeep,
    Store,
}

impl std::fmt::Display for SaveGroupControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cycle => "group_cycle",
            Self::TooDeep => "group_too_deep",
            Self::Store => "group_store",
        })
    }
}

/// Groups on disk. A missing file is an empty list and is not created.
pub fn load_saved_groups() -> Vec<SavedGroupDto> {
    stored_groups()
        .into_iter()
        .map(|group| SavedGroupDto {
            id: group.id,
            members: group.members,
        })
        .collect()
}

/// Replace one group's members. `id` may start with `group/`.
pub fn save_group_members(id: &str, members: &[String]) -> Result<(), SaveGroupControlError> {
    save_group(id, members).map_err(|error| match error {
        SaveGroupError::Cycle => SaveGroupControlError::Cycle,
        SaveGroupError::TooDeep => SaveGroupControlError::TooDeep,
        SaveGroupError::Store => SaveGroupControlError::Store,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::{SaveGroupControlError, load_saved_groups, save_group_members};
    use crate::test_support::{ENV_LOCK, EnvGuard};

    #[tokio::test(flavor = "current_thread")]
    async fn group_control_rejects_a_cycle_and_keeps_the_file() {
        let _lock = ENV_LOCK.lock().await;
        let root = scratch("group-control");
        let _scratch = Scratch(root.clone());
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let _env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
            ("SKILLSTAR_DATA_DIR", &data),
        ]);
        let gateway = data.join("config").join("model_gateway.json");

        assert!(load_saved_groups().is_empty());
        assert!(!gateway.exists(), "a missing file is not created by a read");

        save_group_members("group/demo", &["openai/gpt-test".to_string()]).unwrap();
        let saved = load_saved_groups();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, "demo");
        assert_eq!(saved[0].members, vec!["openai/gpt-test".to_string()]);

        let before = fs::read(&gateway).unwrap();
        let err = save_group_members("demo", &["openai/gpt-test".to_string(), "group/demo".to_string()])
            .unwrap_err();
        assert_eq!(err, SaveGroupControlError::Cycle);
        assert_eq!(err.to_string(), "group_cycle");
        assert_eq!(fs::read(&gateway).unwrap(), before);
        assert_eq!(load_saved_groups()[0].members, vec!["openai/gpt-test".to_string()]);
    }

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()))
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
