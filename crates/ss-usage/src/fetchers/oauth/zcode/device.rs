//! `X-Device-Mid` for zcode.z.ai.
//!
//! `/api/v1/zcode-plan/billing/balance` refuses a request without the
//! `X-Device-Mid` header — HTTP 400, business code 3001 "parameter error"
//! (magpie #282); the server only checks that it is there. The official
//! client names the machine with the `deviceMid` UUID it keeps in
//! `{zcode_home}/v2/telemetry-state.json`. SkillStar reads that file and
//! never writes it (`usage_switch` keeps the same promise), and falls back
//! to a UUID of its own at `state/usage/zcode-device-mid` — made once per
//! machine, kept for next time, used for the call when it can't be kept.

use std::path::{Path, PathBuf};

pub(super) fn device_mid() -> String {
    resolve(&telemetry_state_path(), &own_device_mid_path())
}

fn telemetry_state_path() -> PathBuf {
    crate::tool_paths::zcode_home()
        .join("v2")
        .join("telemetry-state.json")
}

fn own_device_mid_path() -> PathBuf {
    ss_core::infra::paths::zcode_device_mid_path()
}

fn resolve(official: &Path, own: &Path) -> String {
    read_official(official)
        .or_else(|| read_own(own))
        .unwrap_or_else(|| mint(own))
}

/// The official client's `deviceMid`, as a bare UUID. The file is only read;
/// a missing or malformed one is the fallback's to handle.
fn read_official(path: &Path) -> Option<String> {
    let value =
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(path).ok()?).ok()?;
    parse_uuid(value.get("deviceMid")?.as_str()?)
}

fn read_own(path: &Path) -> Option<String> {
    parse_uuid(&std::fs::read_to_string(path).ok()?)
}

fn parse_uuid(text: &str) -> Option<String> {
    uuid::Uuid::parse_str(text.trim())
        .ok()
        .map(|id| id.to_string())
}

fn mint(own: &Path) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    if let Some(dir) = own.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(own, &id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn official_device_mid_wins_and_is_never_written() {
        let dir = TempDir::new().unwrap();
        let official = dir.path().join("telemetry-state.json");
        let own = dir.path().join("own-mid");
        std::fs::write(
            &official,
            r#"{"deviceMid":"aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","other":1}"#,
        )
        .unwrap();
        std::fs::write(&own, "11111111-2222-4333-8444-555555555555").unwrap();

        assert_eq!(
            resolve(&official, &own),
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"
        );
        assert_eq!(
            std::fs::read_to_string(&official).unwrap(),
            r#"{"deviceMid":"aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","other":1}"#
        );
    }

    #[test]
    fn non_uuid_or_missing_official_falls_back_to_own_file() {
        let dir = TempDir::new().unwrap();
        let own = dir.path().join("nested").join("own-mid");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, " 11111111-2222-4333-8444-555555555555 \n").unwrap();

        let missing = dir.path().join("nope").join("telemetry-state.json");
        assert_eq!(
            resolve(&missing, &own),
            "11111111-2222-4333-8444-555555555555"
        );

        let garbage = dir.path().join("telemetry-state.json");
        std::fs::write(&garbage, r#"{"deviceMid":"not-a-uuid"}"#).unwrap();
        assert_eq!(
            resolve(&garbage, &own),
            "11111111-2222-4333-8444-555555555555"
        );
    }

    #[test]
    fn mint_makes_a_kept_uuid_when_nothing_is_kept() {
        let dir = TempDir::new().unwrap();
        let own = dir
            .path()
            .join("state")
            .join("usage")
            .join("zcode-device-mid");
        let official = dir.path().join("telemetry-state.json");

        let first = resolve(&official, &own);
        uuid::Uuid::parse_str(&first).expect("a uuid");
        assert_eq!(std::fs::read_to_string(&own).unwrap(), first);
        assert_eq!(resolve(&official, &own), first);
    }

    #[test]
    fn mint_survives_an_unwritable_kept_path() {
        let dir = TempDir::new().unwrap();
        // A directory where the file should be makes both read and write fail.
        let own = dir.path().join("own-mid");
        std::fs::create_dir_all(&own).unwrap();
        let official = dir.path().join("telemetry-state.json");

        let id = resolve(&official, &own);
        uuid::Uuid::parse_str(&id).expect("a uuid anyway");
    }
}
