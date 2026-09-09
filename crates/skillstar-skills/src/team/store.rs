//! Versioned local store for team intelligence.
//!
//! Unknown / future schema is fail-closed: no read projection, no write.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use skillstar_core::infra::fs_ops::atomic_write;
use skillstar_core::infra::paths::team_store_path;

use super::STORE_SCHEMA_VERSION;
use super::improve::{FrictionRecord, Learning};
use super::recall::RecallKind;

const MAX_LEARNINGS: usize = 200;
const MAX_EVENTS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamStore {
    pub schema_version: u32,
    #[serde(default)]
    pub learnings: Vec<Learning>,
    #[serde(default)]
    pub usage: Vec<UsageEvent>,
    #[serde(default)]
    pub recall_events: Vec<RecallEvent>,
    #[serde(default)]
    pub friction: Vec<FrictionRecord>,
}

impl Default for TeamStore {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            learnings: Vec::new(),
            usage: Vec::new(),
            recall_events: Vec::new(),
            friction: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub skill_name: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallEvent {
    pub id: String,
    pub kind: RecallKind,
    pub at: DateTime<Utc>,
}

pub fn load() -> Result<TeamStore, AppError> {
    let path = team_store_path();
    if !path.is_file() {
        return Ok(TeamStore::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let store: TeamStore = serde_json::from_str(&raw)?;
    ensure_supported_schema(&store)?;
    Ok(store)
}

pub fn save(mut store: TeamStore) -> Result<(), AppError> {
    ensure_supported_schema(&store)?;
    store.schema_version = STORE_SCHEMA_VERSION;
    trim_oldest(&mut store.learnings, MAX_LEARNINGS, |item| item.created_at);
    trim_oldest(&mut store.usage, MAX_EVENTS, |item| item.at);
    trim_oldest(&mut store.recall_events, MAX_EVENTS, |item| item.at);
    trim_oldest(&mut store.friction, MAX_EVENTS, |item| item.at);
    let json = serde_json::to_string_pretty(&store)?;
    atomic_write(&team_store_path(), json.as_bytes())?;
    Ok(())
}

pub fn mutate<T>(f: impl FnOnce(&mut TeamStore) -> Result<T, AppError>) -> Result<T, AppError> {
    let mut store = load()?;
    let value = f(&mut store)?;
    save(store)?;
    Ok(value)
}

fn ensure_supported_schema(store: &TeamStore) -> Result<(), AppError> {
    if store.schema_version == STORE_SCHEMA_VERSION {
        return Ok(());
    }
    Err(AppError::Other(format!(
        "Team store schema {} is not supported (this build reads v{STORE_SCHEMA_VERSION} at {}). Refuse to read or write.",
        store.schema_version,
        team_store_path().display()
    )))
}

fn trim_oldest<T>(items: &mut Vec<T>, max: usize, at: impl Fn(&T) -> DateTime<Utc>) {
    if items.len() <= max {
        return;
    }
    items.sort_by_key(|item| at(item));
    let drop = items.len() - max;
    items.drain(0..drop);
}
