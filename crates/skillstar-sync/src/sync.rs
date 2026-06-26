//! S3 sync orchestration: push all skills, pull the manifest, restore selected.
//!
//! All entry points resolve a target by id (via [`crate::store`]) + secret, build
//! an S3 client, and stream progress through a [`ProgressSink`]. They are
//! Tauri-agnostic; the command layer injects a sink that forwards to
//! `window.emit("s3://sync-stream")`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use aws_sdk_s3::Client;
use chrono::Utc;

use crate::client::build_client;
use crate::local_pack::{pack_skill, unpack_skill};
use crate::manifest::{PackedLocal, annotate_installed, build_manifest, parse, serialise};
use crate::progress::{Phase, ProgressSink, Status, event, event_with_detail};
use crate::store::{SecretStore, load_targets};
use crate::types::{
    InstallOutcome, InstallSummary, Manifest, ManifestEntry, ManifestEntryView, PushSummary,
    S3TargetDef,
};

// ── target resolution ───────────────────────────────────────────────

/// Resolve a target def by id from the on-disk store.
pub fn resolve_target(target_id: &str) -> Result<S3TargetDef> {
    load_targets()
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| anyhow::anyhow!("S3 target '{}' not found", target_id))
}

/// Resolve target + secret and build a ready S3 client.
pub fn resolve_client<S: SecretStore>(
    target_id: &str,
    secrets: &S,
) -> Result<(S3TargetDef, Client)> {
    let target = resolve_target(target_id)?;
    let secret = secrets.get_secret(&target.id)?;
    let client = build_client(&target, secret);
    Ok((target, client))
}

// ── device id ───────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
struct DeviceId {
    device_id: String,
}

/// Load or create this device's stable id, persisted at `state/sync_device.json`.
fn device_id() -> Result<String> {
    let path = skillstar_core::infra::paths::sync_device_id_path();
    if let Ok(content) = std::fs::read_to_string(&path)
        && let Ok(parsed) = serde_json::from_str::<DeviceId>(&content) {
            return Ok(parsed.device_id);
        }
    // Mint a new one: <hostname-or-fallback>-<8 hex>.
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "device".to_string());
    let suffix: String = (0..8)
        .map(|i| {
            let n = (chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64 >> i) & 0xf;
            char::from_digit(n as u32, 16).unwrap_or('0')
        })
        .collect();
    let id = format!("{host}-{suffix}");
    let blob = serde_json::to_vec(&DeviceId { device_id: id.clone() })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, blob).ok(); // best-effort
    Ok(id)
}

// ── push ────────────────────────────────────────────────────────────

/// Upload all local skills to the target's bucket as a fresh manifest.
pub async fn push_all<S: SecretStore>(
    target_id: &str,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<PushSummary> {
    let (target, client) = resolve_client(target_id, secrets)?;
    sink.emit(event(session_id, Phase::ListLocal, Status::Start, "Enumerating local skills…"));

    let skills = skillstar_skills::installed_skill::list_installed_skills()
        .await
        .map_err(|e| anyhow::anyhow!("list installed skills: {e}"))?;
    let (hub, local) = crate::manifest::partition_skills(skills);
    sink.emit(event(
        session_id,
        Phase::ListLocal,
        Status::Ok,
        format!("Found {} hub + {} local skills", hub.len(), local.len()),
    ));

    // Pack + upload each local skill.
    let mut local_meta: HashMap<String, PackedLocal> = HashMap::new();
    let mut uploaded = 0usize;
    let mut skipped = 0usize;
    for s in &local {
        sink.emit(event(session_id, Phase::Pack, Status::Start, format!("Packing '{}'…", s.name)));
        let packed = pack_skill(&s.name).with_context(|| format!("pack '{}'", s.name))?;
        let key = target.tarball_key(&s.name, &packed.sha256);

        // Content-addressed dedup: skip upload if the object already exists.
        let exists = head_object(&client, &target.bucket, &key).await.unwrap_or(false);
        if exists {
            skipped += 1;
            sink.emit(event(session_id, Phase::Upload, Status::Ok, format!("'{}' unchanged (cached)", s.name)));
        } else {
            put_object(&client, &target.bucket, &key, &packed.bytes, "application/gzip").await?;
            uploaded += 1;
            sink.emit(event(
                session_id,
                Phase::Upload,
                Status::Ok,
                format!("Uploaded '{}' ({} bytes)", s.name, packed.size_bytes),
            ));
        }

        local_meta.insert(
            s.name.clone(),
            PackedLocal {
                sha256: packed.sha256,
                size_bytes: packed.size_bytes,
                tarball_key: key,
                uploaded_at: Utc::now().to_rfc3339(),
            },
        );
    }

    // Build + upload manifest.
    let did = device_id().unwrap_or_else(|_| "unknown".to_string());
    let manifest = build_manifest(
        hub.clone(),
        local.clone(),
        local_meta,
        did,
        Utc::now().to_rfc3339(),
    );
    let bytes = serialise(&manifest).context("serialise manifest")?;
    put_object(&client, &target.bucket, &target.manifest_key(), &bytes, "application/json").await?;
    sink.emit(event(session_id, Phase::UploadManifest, Status::Ok, "Manifest uploaded"));

    let summary = crate::manifest::summarise_push(hub.len(), local.len(), uploaded, skipped, true);
    sink.emit(event_with_detail(
        session_id,
        Phase::Done,
        Status::Ok,
        "Push complete",
        serde_json::to_value(&summary).unwrap_or(serde_json::Value::Null),
    ));
    Ok(summary)
}

// ── pull ────────────────────────────────────────────────────────────

/// Download the manifest and annotate each entry with local install state.
pub async fn pull_manifest<S: SecretStore>(
    target_id: &str,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<Vec<ManifestEntryView>> {
    let (target, client) = resolve_client(target_id, secrets)?;
    sink.emit(event(session_id, Phase::Scan, Status::Start, "Fetching manifest…"));
    let bytes = get_object(&client, &target.bucket, &target.manifest_key())
        .await
        .context("download manifest")?;
    let manifest: Manifest = parse(&bytes).context("parse manifest")?;
    sink.emit(event(
        session_id,
        Phase::Scan,
        Status::Ok,
        format!("Manifest v{} with {} skills", manifest.version, manifest.skills.len()),
    ));
    let views = annotate_installed(manifest);
    sink.emit(event(session_id, Phase::Done, Status::Ok, format!("{} skills listed", views.len())));
    Ok(views)
}

// ── restore ─────────────────────────────────────────────────────────

/// Restore (install) a user-selected subset of manifest entries on this device.
///
/// Mirrors `install_from_share_code`'s per-entry decision loop:
/// - already installed locally → `Existing`
/// - hub kind → `install_skills_batch(git_url, [name])` (normal git install)
/// - local kind → download tarball → `unpack_skill`
/// - failure / no source → `Skipped`
pub async fn restore_entries<S: SecretStore>(
    target_id: &str,
    secrets: &S,
    entries: Vec<ManifestEntry>,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<InstallSummary> {
    let (target, client) = resolve_client(target_id, secrets)?;
    let requested = entries.len();
    let mut outcomes: Vec<InstallOutcome> = Vec::with_capacity(requested);
    let mut installed_names = Vec::new();
    let mut existing_names = Vec::new();
    let mut restored_names = Vec::new();
    let mut skipped_names = Vec::new();

    for entry in entries {
        let name = entry.name().to_string();
        sink.emit(event(session_id, Phase::Resolve, Status::Start, format!("Restoring '{name}'…")));

        if is_installed_locally(&name) {
            existing_names.push(name.clone());
            outcomes.push(InstallOutcome::Existing { name: name.clone() });
            sink.emit(event(session_id, Phase::Resolve, Status::Ok, format!("'{name}' already installed")));
            continue;
        }

        let outcome = match entry {
            ManifestEntry::Hub { git_url, .. } => {
                // `name` is moved into the outcome below, so we clone it for the
                // install call's slice. Clippy's `from_ref` suggestion would borrow
                // `name` here, which is fine, but the clone keeps the move explicit.
                #[allow(clippy::cloned_ref_to_slice_refs)]
                let batch_result =
                    skillstar_skills::skill_install::install_skills_batch(&git_url, &[name.clone()]);
                match batch_result {
                    Ok(_) => {
                        installed_names.push(name.clone());
                        sink.emit(event(session_id, Phase::Resolve, Status::Ok, format!("Installed '{name}' from git")));
                        InstallOutcome::Installed { name }
                    }
                    Err(e) => {
                        let reason = format!("git install failed: {e}");
                        skipped_names.push(name.clone());
                        sink.emit(event(session_id, Phase::Resolve, Status::Warn, reason.clone()));
                        InstallOutcome::Skipped { name, reason }
                    }
                }
            }
            ManifestEntry::Local { tarball_key, .. } => {
                sink.emit(event(session_id, Phase::Download, Status::Start, format!("Downloading '{name}'…")));
                match get_object(&client, &target.bucket, &tarball_key).await {
                    Ok(bytes) => {
                        sink.emit(event(session_id, Phase::Unpack, Status::Start, format!("Unpacking '{name}'…")));
                        match unpack_skill(&name, &bytes) {
                            Ok(_) => {
                                restored_names.push(name.clone());
                                sink.emit(event(session_id, Phase::Unpack, Status::Ok, format!("Restored '{name}'")));
                                InstallOutcome::Restored { name }
                            }
                            Err(e) => {
                                let reason = format!("unpack failed: {e}");
                                skipped_names.push(name.clone());
                                sink.emit(event(session_id, Phase::Unpack, Status::Warn, reason.clone()));
                                InstallOutcome::Skipped { name, reason }
                            }
                        }
                    }
                    Err(e) => {
                        let reason = format!("download failed: {e}");
                        skipped_names.push(name.clone());
                        sink.emit(event(session_id, Phase::Download, Status::Warn, reason.clone()));
                        InstallOutcome::Skipped { name, reason }
                    }
                }
            }
        };
        outcomes.push(outcome);
    }

    skillstar_skills::installed_skill::invalidate_cache();

    let summary = InstallSummary {
        requested_count: requested,
        installed_names,
        existing_names,
        restored_names,
        skipped_names,
        outcomes,
    };
    sink.emit(event(session_id, Phase::Done, Status::Ok, "Restore complete"));
    Ok(summary)
}

fn is_installed_locally(name: &str) -> bool {
    let hub = skillstar_core::infra::paths::hub_skills_dir().join(name);
    hub.symlink_metadata().is_ok()
}

// ── S3 primitives ───────────────────────────────────────────────────

async fn put_object(
    client: &Client,
    bucket: &str,
    key: &str,
    bytes: &[u8],
    content_type: &str,
) -> Result<()> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type(content_type)
        .body(bytes.to_vec().into())
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("put {key}: {e}"))?;
    Ok(())
}

async fn get_object(client: &Client, bucket: &str, key: &str) -> Result<Vec<u8>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("get {key}: {e}"))?;
    let body = resp
        .body
        .collect()
        .await
        .map_err(|e| anyhow::anyhow!("read body {key}: {e}"))?;
    Ok(body.into_bytes().to_vec())
}

async fn head_object(client: &Client, bucket: &str, key: &str) -> Result<bool> {
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Ok(true),
        Err(err) => {
            let svc_err = err.into_service_error();
            if svc_err.is_not_found() {
                Ok(false)
            } else {
                Err(anyhow::anyhow!("head {key}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_stable_across_reads() {
        let _g = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }
        let first = device_id().unwrap();
        let second = device_id().unwrap();
        assert!(!first.is_empty());
        assert_eq!(first, second, "device id must persist once minted");
    }
}
