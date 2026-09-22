//! Approval records for one deployment plan.
//!
//! Only elicitation and SkillStar can write a record. Tool arguments have no
//! conversion into this type.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalSource {
    Elicitation,
    Skillstar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub plan_id: String,
    pub plan_hash: String,
    pub source: ApprovalSource,
    pub recorded_at: DateTime<Utc>,
}

pub fn record_from_elicitation(plan_id: &str, plan_hash: &str) -> Result<ApprovalRecord> {
    record(plan_id, plan_hash, ApprovalSource::Elicitation)
}

pub fn record_from_skillstar(plan_id: &str, plan_hash: &str) -> Result<ApprovalRecord> {
    record(plan_id, plan_hash, ApprovalSource::Skillstar)
}

pub fn load_approval(plan_id: &str) -> Result<Option<ApprovalRecord>> {
    let path = approval_path(plan_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let record: ApprovalRecord = serde_json::from_slice(&bytes).context("parse approval record")?;
    if record.plan_id != plan_id {
        anyhow::bail!("approval plan id does not match its file");
    }
    Ok(Some(record))
}

fn record(plan_id: &str, plan_hash: &str, source: ApprovalSource) -> Result<ApprovalRecord> {
    if let Some(existing) = load_approval(plan_id)? {
        if existing.source == source && existing.plan_hash == plan_hash {
            return Ok(existing);
        }
        anyhow::bail!("approval already recorded from another source or hash");
    }
    let record = ApprovalRecord {
        plan_id: plan_id.to_string(),
        plan_hash: plan_hash.to_string(),
        source,
        recorded_at: Utc::now(),
    };
    let path = approval_path(plan_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&record)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(record)
}

fn approval_path(plan_id: &str) -> Result<PathBuf> {
    if plan_id.is_empty()
        || !plan_id
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
    {
        anyhow::bail!("invalid plan id");
    }
    Ok(skillstar_core::infra::paths::state_dir()
        .join("project-skill-approvals")
        .join(format!("{plan_id}.json")))
}

#[cfg(test)]
mod approval_record_tests {
    use super::{load_approval, record_from_elicitation, record_from_skillstar};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-approval-{nanos}"));
            fs::create_dir_all(root.join("data")).unwrap();
            let home = std::env::var_os("HOME");
            let data = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe {
                std::env::set_var("HOME", root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", root.join("data"));
            }
            Self {
                root,
                home,
                data,
                _lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match self.home.take() {
                    Some(value) => std::env::set_var("HOME", value),
                    None => std::env::remove_var("HOME"),
                }
                match self.data.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn file_text(plan_id: &str) -> String {
        let path = skillstar_core::infra::paths::state_dir()
            .join("project-skill-approvals")
            .join(format!("{plan_id}.json"));
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn elicitation_then_skillstar_is_rejected() {
        let _env = EnvGuard::new();
        record_from_elicitation("abc", "hash-1").unwrap();
        let before = file_text("abc");
        let err = record_from_skillstar("abc", "hash-1").unwrap_err();
        assert!(err.to_string().contains("already recorded"), "{err}");
        assert_eq!(file_text("abc"), before);
    }

    #[test]
    fn skillstar_then_elicitation_is_rejected() {
        let _env = EnvGuard::new();
        record_from_skillstar("abc", "hash-1").unwrap();
        let before = file_text("abc");
        assert!(record_from_elicitation("abc", "hash-1").is_err());
        assert_eq!(file_text("abc"), before);
    }

    #[test]
    fn same_source_same_hash_is_idempotent() {
        let _env = EnvGuard::new();
        let first = record_from_skillstar("abc", "hash-1").unwrap();
        let second = record_from_skillstar("abc", "hash-1").unwrap();
        assert_eq!(first, second);
        assert!(record_from_skillstar("abc", "hash-2").is_err());
    }

    #[test]
    fn approval_json_has_no_user_confirmed_field() {
        let _env = EnvGuard::new();
        record_from_elicitation("abc", "hash-1").unwrap();
        let text = file_text("abc");
        assert!(!text.contains("user_confirmed"));
        assert!(text.contains("elicitation"));
    }

    #[test]
    fn approval_store_follows_skillstar_data_dir() {
        let env = EnvGuard::new();
        record_from_skillstar("def", "hash-9").unwrap();
        let path = env.root.join("data/state/project-skill-approvals/def.json");
        assert!(path.is_file(), "{}", path.display());
        let loaded = load_approval("def").unwrap().unwrap();
        assert_eq!(loaded.plan_hash, "hash-9");
        assert!(load_approval("abcd").unwrap().is_none());
    }
}
