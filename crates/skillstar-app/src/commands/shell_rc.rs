//! Shell-rc (de)serialization for Codex `third_party` auth.
//!
//! When a provider is wired into Codex under `auth_mode = "third_party"`, Codex
//! reads its API key from an env var named `SKILLSTAR_<PREFIX>_KEY` (see
//! `skillstar_models::tool_sync::types::codex_env_key_for`). The user must get
//! that `export` line into their shell profile. This module does it for them —
//! idempotently and safely — behind an **explicit** button in `CodexSettingsForm`.
//!
//! Design constraints (these are the reasons this is NOT just a `std::fs::write`):
//! - **Idempotent**: re-clicking the button with the same key is a no-op; with a
//!   new key the existing line is replaced in place (never duplicated).
//! - **Non-destructive**: every other line in the user's rc file is preserved
//!   verbatim. We never rewrite the whole file from a template.
//! - **Atomic**: the new content is written to a sibling temp file and `rename`d
//!   into place, so a crash mid-write cannot truncate the user's rc.
//! - **Single backup**: one timestamped `.bak` is kept before the first
//!   in-place mutation; we do not pile up rolling backups in the user's home.
//! - **Explicit only**: the caller (a UI button) drives this; nothing in the
//!   autosave path triggers it.

use std::path::{Path, PathBuf};

use serde::Serialize;
use skillstar_core::infra::error::AppError;

/// The marker comment that precedes every line we manage, so users (and a
/// future `remove_env_export`) can identify our additions.
const MARKER: &str = "# Added by SkillStar (Codex third-party auth)";

/// Outcome of an idempotent `export` write into `~/.zshrc`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellRcWriteResult {
    /// Absolute path of the rc file that was (or would have been) touched.
    pub path: String,
    /// `false` when the file already had the exact target line (true no-op);
    /// `true` when the file was actually mutated on disk.
    pub written: bool,
    /// Path of the timestamped backup created before the mutation, when one was
    /// made. `None` if the file did not previously exist or no write occurred.
    pub backup_path: Option<String>,
    /// `"added"` (new line appended), `"updated"` (existing line replaced), or
    /// `"noop"` (file already in target state).
    pub action: String,
}

/// Resolve `~/.zshrc` for the current user via the shared `home_dir()` helper
/// (which honours the test sandbox when running under `cfg(test)`).
fn zshrc_path(home: &Path) -> PathBuf {
    home.join(".zshrc")
}

/// Build the canonical managed line for `env_key` = `value`. The value is
/// single-quoted after the standard shell rule of replacing embedded `'` with
/// `'\''`, so the key survives intact even if it contains shell metacharacters.
/// The leading marker comment lets users and `remove_env_export` recognize it.
fn managed_line(env_key: &str, value: &str) -> String {
    let safe_value = value.replace('\'', "'\\''");
    format!("{MARKER}\nexport {env_key}='{safe_value}'")
}

/// Test whether a single rc line is the `export <env_key>=...` we manage.
/// Matches both our commented form and a bare `export FOO=...` a user may have
/// added manually (so we still consolidate duplicates into one managed line).
fn is_managed_export(line: &str, env_key: &str) -> bool {
    let trimmed = line.trim_start();
    let needle = format!("export {env_key}=");
    trimmed.starts_with(&needle)
}

/// Extract the value of an `export <env_key>='...'` / `export <env_key>="..."`
/// / `export <env_key>=bare` line, stripping surrounding quotes and un-escaping
/// the `'\''` sequence `managed_line` produces. Returns `None` if the line is
/// not an export of `env_key`.
fn extract_value(line: &str, env_key: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let prefix = format!("export {env_key}=");
    let rest = trimmed.strip_prefix(&prefix)?;
    let rest = rest.trim_end();
    let raw = if rest.len() >= 2 {
        let bytes = rest.as_bytes();
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            &rest[1..rest.len() - 1]
        } else {
            rest
        }
    } else {
        rest
    };
    // Reverse the `'\''` escaping applied by `managed_line` for single-quoted
    // values; leave bare values untouched.
    Some(raw.replace("'\\''", "'"))
}

/// Read the current value of `env_key` from `~/.zshrc`, if present. Used by the
/// UI to show "already written ✓" without re-writing.
pub fn read_env_export(home: &Path, env_key: &str) -> Option<String> {
    let path = zshrc_path(home);
    let content = std::fs::read_to_string(&path).ok()?;
    // Last definition wins in zsh, so scan the whole file and keep the final hit.
    let mut found: Option<String> = None;
    for line in content.lines() {
        if is_managed_export(line, env_key) {
            found = extract_value(line, env_key).or(found);
        }
    }
    found
}

/// Idempotently ensure `~/.zshrc` contains `export <env_key>='<value>'`. See the
/// module docs for the full safety contract (backup, atomic rename, no-op).
pub fn ensure_env_export(home: &Path, env_key: &str, value: &str) -> Result<ShellRcWriteResult, AppError> {
    let path = zshrc_path(home);
    let content = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    // Split into logical lines, preserving the original trailing-newline shape.
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let had_trailing_newline = content.ends_with('\n');

    // Locate the existing managed export line (and its marker, if any).
    // We only ever manage one line per env_key; if duplicates exist they get
    // folded into a single managed line.
    let mut existing_idx: Option<usize> = None;
    let mut existing_value: Option<String> = None;
    let mut to_drop: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if is_managed_export(line, env_key) {
            if existing_idx.is_none() {
                existing_idx = Some(i);
                existing_value = extract_value(line, env_key);
            } else {
                // Duplicate export — drop later occurrences.
                to_drop.push(i);
            }
        }
    }
    for i in to_drop.iter().rev() {
        lines.remove(*i);
    }

    let new_managed = managed_line(env_key, value);
    // `managed_line` is two physical lines (marker + export); when we splice it
    // into an existing slot we replace the single export line and drop a marker
    // immediately preceding it if present (to avoid stacking markers).

    // Tidy: if the value already matches we report noop regardless of whether
    // a marker needs injecting (the value is what matters to the caller).
    match existing_idx {
        Some(_) if existing_value.as_deref() == Some(value) => {
            return Ok(ShellRcWriteResult {
                path: path.to_string_lossy().to_string(),
                written: false,
                backup_path: None,
                action: "noop".to_string(),
            });
        }
        Some(idx) => {
            // Replace the existing export line in place. First remove a stale
            // marker immediately above it so we don't leave two markers stacked.
            if idx > 0 && lines[idx - 1].trim_start() == MARKER {
                lines.remove(idx - 1);
                // idx shifted down by one after the remove.
                let shifted = idx - 1;
                lines[shifted] = new_managed.clone();
            } else {
                lines[idx] = new_managed.clone();
            }
        }
        None => {
            // Append. Ensure a blank separator before the marker if the file is
            // non-empty and doesn't already end with a blank line.
            if !lines.is_empty() && !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push(new_managed.clone());
        }
    }

    // Rebuild content, preserving trailing newline.
    let mut new_content = lines.join("\n");
    if had_trailing_newline || !new_content.is_empty() {
        new_content.push('\n');
    }

    // Backup the original (only when it existed and we are about to mutate).
    let backup_path = if path.exists() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let bak = path.with_extension(format!("zshrc.bak.{ts}"));
        match std::fs::copy(&path, &bak) {
            Ok(_) => Some(bak.to_string_lossy().to_string()),
            Err(_) => None, // best-effort; don't block the write on backup failure
        }
    } else {
        None
    };

    // Atomic write: temp file in the same directory, then rename.
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".zshrc.skillstar.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &new_content)?;
    std::fs::rename(&tmp, &path)?;

    let action = if existing_idx.is_some() { "updated" } else { "added" };
    Ok(ShellRcWriteResult {
        path: path.to_string_lossy().to_string(),
        written: true,
        backup_path,
        action: action.to_string(),
    })
}

/// Remove the managed `export <env_key>=...` line (and its marker) from
/// `~/.zshrc`. Implemented for completeness / future "disconnect" flows; the UI
/// does not currently expose it.
pub fn remove_env_export(home: &Path, env_key: &str) -> Result<bool, AppError> {
    let path = zshrc_path(home);
    if !path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&path)?;
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let mut removed = false;
    let mut i = 0;
    while i < lines.len() {
        let is_target = is_managed_export(&lines[i], env_key);
        let is_marker_above_target =
            is_target && i > 0 && lines[i - 1].trim_start() == MARKER;
        if is_marker_above_target {
            lines.remove(i - 1);
            // i now points at what was the export line (shifted down).
            lines.remove(i - 1);
            removed = true;
            // Don't advance i — re-check the new line at this position.
        } else if is_target {
            lines.remove(i);
            removed = true;
        } else {
            i += 1;
        }
    }

    if !removed {
        return Ok(false);
    }

    let mut new_content = lines.join("\n");
    if had_trailing_newline && !new_content.is_empty() {
        new_content.push('\n');
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(".zshrc.skillstar.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &new_content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Write `export <env_key>='<value>'` into `~/.zshrc` idempotently. Triggered
/// only by an explicit button in `CodexSettingsForm` (third_party auth mode).
#[tauri::command]
pub async fn write_codex_env_to_zshrc(
    env_key: String,
    value: String,
) -> Result<ShellRcWriteResult, AppError> {
    let home = skillstar_core::infra::paths::home_dir();
    ensure_env_export(&home, &env_key, &value)
}

/// Read the current value of `env_key` from `~/.zshrc`, if any. Powers the UI
/// "already written ✓" badge so the user knows whether a click is still needed.
#[tauri::command]
pub async fn read_codex_env_from_zshrc(env_key: String) -> Result<Option<String>, AppError> {
    let home = skillstar_core::infra::paths::home_dir();
    Ok(read_env_export(&home, &env_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn read_zshrc(home: &Path) -> String {
        std::fs::read_to_string(zshrc_path(home)).unwrap_or_default()
    }

    #[test]
    fn appends_to_missing_file() {
        let tmp = TempDir::new().unwrap();
        let res = ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-123").unwrap();
        assert!(res.written);
        assert_eq!(res.action, "added");
        assert!(res.backup_path.is_none(), "no backup when file was absent");
        let content = read_zshrc(tmp.path());
        assert!(content.contains("# Added by SkillStar"));
        assert!(content.contains("export SKILLSTAR_AB_KEY='sk-123'"));
    }

    #[test]
    fn noop_when_value_already_matches() {
        let tmp = TempDir::new().unwrap();
        // Seed the exact target.
        ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-123").unwrap();
        // Second call with the same value must be a no-op.
        let res = ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-123").unwrap();
        assert!(!res.written);
        assert_eq!(res.action, "noop");
        assert!(res.backup_path.is_none());
        // And the file still contains exactly one export line.
        let content = read_zshrc(tmp.path());
        assert_eq!(
            content.matches("export SKILLSTAR_AB_KEY=").count(),
            1,
            "must not duplicate the line on noop"
        );
    }

    #[test]
    fn updates_value_in_place() {
        let tmp = TempDir::new().unwrap();
        ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-old").unwrap();
        let res = ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-new").unwrap();
        assert!(res.written);
        assert_eq!(res.action, "updated");
        assert!(res.backup_path.is_some(), "existing file must be backed up");
        let content = read_zshrc(tmp.path());
        assert!(content.contains("export SKILLSTAR_AB_KEY='sk-new'"));
        assert!(
            !content.contains("sk-old"),
            "old value must be fully replaced, not left behind"
        );
        assert_eq!(
            content.matches("export SKILLSTAR_AB_KEY=").count(),
            1,
            "no duplicate after update"
        );
    }

    #[test]
    fn preserves_unrelated_user_lines() {
        let tmp = TempDir::new().unwrap();
        let original = "# my config\nexport PATH=/usr/local/bin:$PATH\nalias g=git\nexport MY_TOKEN=keepme\n";
        std::fs::write(zshrc_path(tmp.path()), original).unwrap();

        ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-1").unwrap();
        let content = read_zshrc(tmp.path());

        // Every original line survives.
        assert!(content.contains("# my config"));
        assert!(content.contains("export PATH=/usr/local/bin:$PATH"));
        assert!(content.contains("alias g=git"));
        assert!(content.contains("export MY_TOKEN=keepme"));
        // And the new one is added.
        assert!(content.contains("export SKILLSTAR_AB_KEY='sk-1'"));
    }

    #[test]
    fn backup_contains_original_content() {
        let tmp = TempDir::new().unwrap();
        let original = "alias x=y\n";
        std::fs::write(zshrc_path(tmp.path()), original).unwrap();

        let res = ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-1").unwrap();
        let backup = std::fs::read_to_string(res.backup_path.unwrap()).unwrap();
        assert_eq!(backup, original, "backup must be a byte copy of the original");
    }

    #[test]
    fn folds_duplicate_exports_into_one() {
        let tmp = TempDir::new().unwrap();
        // A user (or two SkillStar versions) left two export lines.
        let messy = "export SKILLSTAR_AB_KEY='sk-a'\nalias g=git\nexport SKILLSTAR_AB_KEY='sk-b'\n";
        std::fs::write(zshrc_path(tmp.path()), messy).unwrap();

        let res = ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-final").unwrap();
        assert_eq!(res.action, "updated");
        let content = read_zshrc(tmp.path());
        assert_eq!(
            content.matches("export SKILLSTAR_AB_KEY=").count(),
            1,
            "duplicates must collapse to one"
        );
        assert!(content.contains("sk-final"));
        assert!(!content.contains("sk-a"));
        assert!(!content.contains("sk-b"));
        // Unrelated line preserved.
        assert!(content.contains("alias g=git"));
    }

    #[test]
    fn read_env_export_returns_last_definition() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            zshrc_path(tmp.path()),
            "export SKILLSTAR_AB_KEY='first'\nexport SKILLSTAR_AB_KEY='second'\n",
        )
        .unwrap();
        assert_eq!(
            read_env_export(tmp.path(), "SKILLSTAR_AB_KEY"),
            Some("second".to_string())
        );
        assert_eq!(read_env_export(tmp.path(), "SKILLSTAR_MISSING"), None);
    }

    #[test]
    fn remove_env_export_drops_line_and_marker() {
        let tmp = TempDir::new().unwrap();
        ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", "sk-1").unwrap();
        let removed = remove_env_export(tmp.path(), "SKILLSTAR_AB_KEY").unwrap();
        assert!(removed);
        let content = read_zshrc(tmp.path());
        assert!(!content.contains("SKILLSTAR_AB_KEY"));
        assert!(!content.contains(MARKER));
    }

    #[test]
    fn value_with_single_quote_is_escaped() {
        let tmp = TempDir::new().unwrap();
        // A pathological key containing a single quote must survive round-trip.
        let weird = "sk'quote";
        ensure_env_export(tmp.path(), "SKILLSTAR_AB_KEY", weird).unwrap();
        assert_eq!(
            read_env_export(tmp.path(), "SKILLSTAR_AB_KEY"),
            Some(weird.to_string()),
            "single quote must round-trip through the managed line"
        );
    }
}
