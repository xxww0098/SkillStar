//! State for the managed-skills section: the per-directory journal cache,
//! the epoch guard that rejects stale reads, and the notice the row shows.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ss_skills::workflows::agent_managed_skills::AgentManagedSkillsState;

pub(super) const SKIP_UNMANAGED_REAL_DIRECTORY: &str = "unmanaged_real_directory";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTone {
    Error,
    Ok,
    Warn,
}

#[derive(Clone)]
pub(crate) struct AgentNotice {
    pub(crate) tone: AgentTone,
    pub(crate) text: String,
    pub(crate) open_path: Option<String>,
}

impl AgentNotice {
    pub(super) fn error(text: impl ToString) -> Self {
        Self {
            tone: AgentTone::Error,
            text: text.to_string(),
            open_path: None,
        }
    }

    pub(super) fn warn(text: impl ToString) -> Self {
        Self {
            tone: AgentTone::Warn,
            text: text.to_string(),
            open_path: None,
        }
    }

    pub(super) fn ok(text: impl ToString) -> Self {
        Self {
            tone: AgentTone::Ok,
            text: text.to_string(),
            open_path: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct ManagedSkillsUi {
    pub(super) states: HashMap<String, AgentManagedSkillsState>,
    pub(super) pending: HashSet<String>,
    pub(super) epochs: HashMap<String, u64>,
    pub(super) requests: HashMap<String, u64>,
}

impl ManagedSkillsUi {
    pub(super) fn begin(&mut self, key: &str) -> (u64, u64) {
        let request = self.requests.get(key).copied().unwrap_or(0) + 1;
        self.requests.insert(key.to_string(), request);
        let epoch = self.epochs.get(key).copied().unwrap_or(0);
        (epoch, request)
    }

    pub(super) fn invalidate(&mut self, key: &str) {
        let epoch = self.epochs.get(key).copied().unwrap_or(0) + 1;
        self.epochs.insert(key.to_string(), epoch);
    }

    pub(super) fn accepts(&self, key: &str, epoch: u64, request: u64) -> bool {
        self.epochs.get(key).copied().unwrap_or(0) == epoch
            && self.requests.get(key) == Some(&request)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PauseStatus {
    Loading,
    Empty,
    Active,
    Paused,
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum PauseAction {
    Pause,
    Restore,
}

pub(super) struct PauseSnapshot {
    pub(super) status: PauseStatus,
    pub(super) active: Vec<String>,
    pub(super) suspended: Vec<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) action: Option<PauseAction>,
    pub(super) checked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadPurpose {
    Preload,
    Refresh,
}

pub(super) struct ManagedJob {
    pub(super) key: String,
    pub(super) epoch: u64,
    pub(super) request: u64,
    pub(super) agent_id: String,
    pub(super) purpose: ReadPurpose,
}

pub(crate) fn global_skills_target_key(path: &Path) -> String {
    path.to_string_lossy()
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}
