//! Tauri commands for the fingerprint store.
//!
//! These commands let the frontend manage [`DeviceFingerprint`] entries
//! (list / create-from-preset / update / delete / activate) and discover
//! the built-in [`PresetTemplate`]s available.
//!
//! Storage lives at `~/.skillstar/config/fingerprints.json`. The
//! `"original"` row is auto-created on first read and refuses deletion.

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use skillstar_fingerprint::{
    DeviceFingerprint, FingerprintStore, HttpProfile, IdeProjector, NetworkProfile, PresetId,
    PresetTemplate, SupportedIde, TlsProfile, VsCodeForkProjector, all_presets, instantiate,
};

// ── DTOs ──────────────────────────────────────────────────────────────

/// Wrapper around [`DeviceFingerprint`] that also tells the UI whether
/// this entry is the active one. Sent as the response type to all CRUD
/// commands so the frontend can simply replace its current list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintRow {
    #[serde(flatten)]
    pub fingerprint: DeviceFingerprint,
    /// `true` if this row is currently marked active in the store.
    pub is_active: bool,
    /// `true` for the immutable baseline (`id == "original"`).
    pub is_original: bool,
}

/// List response — items + the currently active id (so the frontend can
/// render a selection ring without re-scanning the list).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintListDto {
    pub items: Vec<FingerprintRow>,
    pub active_id: Option<String>,
}

/// Patch input for [`update_fingerprint`]. Each field is optional —
/// `None` leaves the underlying value untouched. `name` is the only
/// field that can be set to a fresh string; the deeper TLS / HTTP
/// profiles are fully replaced when present (no partial merge — keeps
/// the data model simple).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFingerprintInput {
    pub name: Option<String>,
    pub http: Option<HttpProfile>,
    pub tls: Option<TlsProfile>,
    pub network: Option<NetworkProfile>,
}

// ── Helpers ───────────────────────────────────────────────────────────

fn load() -> Result<FingerprintStore, AppError> {
    FingerprintStore::load_default().map_err(|e| AppError::Other(format!("fingerprint store: {e}")))
}

fn save(store: &FingerprintStore) -> Result<(), AppError> {
    store
        .save_default()
        .map_err(|e| AppError::Other(format!("fingerprint store save: {e}")))
}

fn row(fp: &DeviceFingerprint, active_id: Option<&str>) -> FingerprintRow {
    FingerprintRow {
        is_active: active_id == Some(fp.id.as_str()),
        is_original: fp.is_original(),
        fingerprint: fp.clone(),
    }
}

// ── Commands ──────────────────────────────────────────────────────────

/// List all stored fingerprints, original first, then newest user-created.
#[tauri::command]
pub fn list_fingerprints() -> Result<FingerprintListDto, AppError> {
    let store = load()?;
    let active = store.active_id.clone();
    let items = store
        .list_sorted()
        .into_iter()
        .map(|fp| row(fp, active.as_deref()))
        .collect();
    Ok(FingerprintListDto {
        items,
        active_id: active,
    })
}

/// Fetch a single fingerprint by id.
#[tauri::command]
pub fn get_fingerprint(id: String) -> Result<FingerprintRow, AppError> {
    let store = load()?;
    let active = store.active_id.clone();
    let fp = store
        .get(&id)
        .map_err(|e| AppError::Other(format!("fingerprint: {e}")))?;
    Ok(row(fp, active.as_deref()))
}

/// List built-in presets the user can clone from.
#[tauri::command]
pub fn list_fingerprint_presets() -> Result<Vec<PresetTemplate>, AppError> {
    Ok(all_presets())
}

/// Instantiate a fresh fingerprint from a preset id and persist it.
/// Returns the freshly stored row.
#[tauri::command]
pub fn create_fingerprint_from_preset(
    preset_id: String,
    name: String,
) -> Result<FingerprintRow, AppError> {
    let preset = PresetId::from_id(&preset_id)
        .ok_or_else(|| AppError::Other(format!("unknown preset id: {preset_id}")))?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other("指纹名称不能为空".into()));
    }
    let fp = instantiate(preset, trimmed.to_string());

    let mut store = load()?;
    let fp_id = fp.id.clone();
    store
        .upsert(fp)
        .map_err(|e| AppError::Other(format!("fingerprint upsert: {e}")))?;
    save(&store)?;

    let active = store.active_id.clone();
    let stored = store
        .get(&fp_id)
        .map_err(|e| AppError::Other(format!("fingerprint reload: {e}")))?;
    Ok(row(stored, active.as_deref()))
}

/// Patch a fingerprint's name / http / tls / network fields. Refuses to
/// touch the immutable `"original"` row.
#[tauri::command]
pub fn update_fingerprint(
    id: String,
    input: UpdateFingerprintInput,
) -> Result<FingerprintRow, AppError> {
    if id == "original" {
        return Err(AppError::Other("原始指纹不可修改".into()));
    }
    let mut store = load()?;
    let mut fp = store
        .get(&id)
        .map_err(|e| AppError::Other(format!("fingerprint: {e}")))?
        .clone();

    if let Some(name) = input.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::Other("指纹名称不能为空".into()));
        }
        fp.name = trimmed.to_string();
    }
    if let Some(http) = input.http {
        fp.http = http;
    }
    if let Some(tls) = input.tls {
        fp.tls = tls;
    }
    if let Some(network) = input.network {
        fp.network = network;
    }
    fp.updated_at = chrono::Utc::now().timestamp();

    store
        .upsert(fp.clone())
        .map_err(|e| AppError::Other(format!("fingerprint upsert: {e}")))?;
    save(&store)?;

    let active = store.active_id.clone();
    Ok(row(&fp, active.as_deref()))
}

/// Delete a fingerprint. Refuses to delete `"original"`. If the deleted
/// row was active, the store auto-falls back to `"original"`. Returns
/// the updated list response.
#[tauri::command]
pub fn delete_fingerprint(id: String) -> Result<FingerprintListDto, AppError> {
    let mut store = load()?;
    store
        .delete(&id)
        .map_err(|e| AppError::Other(format!("fingerprint delete: {e}")))?;
    save(&store)?;

    let active = store.active_id.clone();
    let items = store
        .list_sorted()
        .into_iter()
        .map(|fp| row(fp, active.as_deref()))
        .collect();
    Ok(FingerprintListDto {
        items,
        active_id: active,
    })
}

/// Mark a fingerprint as the active one. Used by the IDE projector
/// commands below as the default source when none is given.
#[tauri::command]
pub fn set_active_fingerprint(id: String) -> Result<FingerprintRow, AppError> {
    let mut store = load()?;
    store
        .set_active(&id)
        .map_err(|e| AppError::Other(format!("fingerprint set_active: {e}")))?;
    save(&store)?;

    let active = store.active_id.clone();
    let fp = store
        .get(&id)
        .map_err(|e| AppError::Other(format!("fingerprint reload: {e}")))?;
    Ok(row(fp, active.as_deref()))
}

// ── IDE projector commands (Phase 6) ──────────────────────────────────

fn find_projector(agent_id: &str) -> Result<&'static VsCodeForkProjector, AppError> {
    VsCodeForkProjector::all()
        .iter()
        .find(|p| p.agent_id() == agent_id)
        .ok_or_else(|| AppError::Other(format!("不支持的 IDE: {agent_id}")))
}

/// List every IDE whose telemetry SkillStar knows how to project. Each
/// entry reports whether it's installed locally and what telemetry is
/// currently on disk so the UI can show a diff vs. the bound fingerprint.
#[tauri::command]
pub fn list_supported_ides() -> Result<Vec<SupportedIde>, AppError> {
    Ok(VsCodeForkProjector::all()
        .iter()
        .map(|p| p.summary())
        .collect())
}

/// Apply the given fingerprint's `telemetry` block to the given IDE's
/// on-disk `storage.json`. Creates a baseline backup on first call so
/// the original identity is recoverable via [`restore_ide_baseline`].
///
/// `fingerprint_id` is optional: when omitted we fall back to whatever
/// is currently active in the store (which defaults to `"original"`).
#[tauri::command]
pub fn apply_fingerprint_to_ide(
    agent_id: String,
    fingerprint_id: Option<String>,
) -> Result<SupportedIde, AppError> {
    let projector = find_projector(&agent_id)?;
    let store = load()?;
    let target_id = fingerprint_id
        .or_else(|| store.active_id.clone())
        .unwrap_or_else(|| "original".to_string());
    let fp = store
        .get(&target_id)
        .map_err(|e| AppError::Other(format!("fingerprint: {e}")))?;
    projector
        .apply(&fp.telemetry)
        .map_err(|e| AppError::Other(format!("apply: {e}")))?;
    Ok(projector.summary())
}

/// Restore the originally-captured telemetry for an IDE. Fails if no
/// baseline has been stored yet (i.e. SkillStar has never written this
/// IDE's `storage.json`).
#[tauri::command]
pub fn restore_ide_baseline(agent_id: String) -> Result<SupportedIde, AppError> {
    let projector = find_projector(&agent_id)?;
    projector
        .restore_baseline()
        .map_err(|e| AppError::Other(format!("restore: {e}")))?;
    Ok(projector.summary())
}
