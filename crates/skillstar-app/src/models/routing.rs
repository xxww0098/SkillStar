//! Routing mode and affinity for one provider or one saved group.
//!
//! The page sends the eight enum words. This module writes
//! `model_gateway.json` through `skillstar-gateway` and does not open the
//! provider store.

use serde::{Deserialize, Serialize};
use skillstar_gateway::{AffinityMode, RouteMode, RouteOwner, routing_state, save_routing, stored_group_ids};

/// The two values a routing control is showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingControl {
    pub routing: String,
    pub affinity: String,
}

/// One saved group and the control values stored for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingGroupControl {
    pub id: String,
    pub routing: String,
    pub affinity: String,
}

/// Selected provider, when the page has one, plus every saved group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingPage {
    pub provider: Option<RoutingControl>,
    pub groups: Vec<RoutingGroupControl>,
}

/// Why a control value was not saved. The provider store is not involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingControlError {
    /// Owner or either value is outside the eight words.
    Unknown,
    /// The id is empty, or `model_gateway.json` could not be replaced.
    Store,
}

impl std::fmt::Display for RoutingControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unknown => "routing_unknown",
            Self::Store => "routing_store",
        })
    }
}

/// Read the selected provider and the saved groups.
///
/// An empty provider id leaves `provider` empty. A missing gateway file is
/// smart and auto, and this read does not create the file.
pub fn load_routing_page(provider_id: &str) -> RoutingPage {
    let provider = if provider_id.trim().is_empty() {
        None
    } else {
        control_of(RouteOwner::Provider, provider_id).ok()
    };
    let groups = stored_group_ids()
        .into_iter()
        .filter_map(|id| {
            let control = control_of(RouteOwner::Group, &id).ok()?;
            Some(RoutingGroupControl {
                id,
                routing: control.routing,
                affinity: control.affinity,
            })
        })
        .collect();
    RoutingPage { provider, groups }
}

/// Save one control. `owner` is `provider` or `group`.
pub fn save_routing_control(
    owner: &str,
    id: &str,
    routing: &str,
    affinity: &str,
) -> Result<(), RoutingControlError> {
    let owner = owner_of(owner).ok_or(RoutingControlError::Unknown)?;
    let mode = RouteMode::from_control(routing).ok_or(RoutingControlError::Unknown)?;
    let affinity = AffinityMode::from_control(affinity).ok_or(RoutingControlError::Unknown)?;
    save_routing(owner, id, mode, affinity).map_err(|_| RoutingControlError::Store)
}

fn control_of(owner: RouteOwner, id: &str) -> Result<RoutingControl, RoutingControlError> {
    let (mode, affinity) = routing_state(owner, id).map_err(|_| RoutingControlError::Store)?;
    Ok(RoutingControl {
        routing: mode.as_str().to_string(),
        affinity: affinity.as_str().to_string(),
    })
}

fn owner_of(raw: &str) -> Option<RouteOwner> {
    match raw {
        "provider" => Some(RouteOwner::Provider),
        "group" => Some(RouteOwner::Group),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use serde_json::Value;

    use super::{RoutingControlError, load_routing_page, save_routing_control};
    use crate::test_support::{ENV_LOCK, EnvGuard};

    #[tokio::test(flavor = "current_thread")]
    async fn routing_control_persists_in_gateway_json() {
        let _lock = ENV_LOCK.lock().await;
        let root = scratch("routing-persist");
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

        let fresh = load_routing_page("p1");
        assert_eq!(fresh.provider.as_ref().map(|row| row.routing.as_str()), Some("smart"));
        assert_eq!(fresh.provider.as_ref().map(|row| row.affinity.as_str()), Some("auto"));
        assert!(!gateway.exists(), "a missing file is not created by a read");

        fs::create_dir_all(gateway.parent().unwrap()).unwrap();
        fs::write(
            &gateway,
            r#"{"redact":false,"providers":[{"id":"p1","note":"keep-me"}],"groups":[{"id":"fast","members":["a/m","b/m"]}]}"#,
        )
        .unwrap();

        save_routing_control("provider", "p1", "rotate", "session").unwrap();
        let saved = read_json(&gateway);
        assert_eq!(saved["redact"], false);
        assert_eq!(saved["providers"][0]["note"], "keep-me");
        assert_eq!(saved["providers"][0]["routing"], "rotate");
        assert_eq!(saved["providers"][0]["affinity"], "session");
        assert_eq!(saved["groups"][0]["members"][0], "a/m");
        assert!(saved["groups"][0].get("routing").is_none());
        let page = load_routing_page("p1");
        let provider = page.provider.expect("selected provider");
        assert_eq!(provider.routing, "rotate");
        assert_eq!(provider.affinity, "session");

        save_routing_control("provider", "p1", "smart", "auto").unwrap();
        let cleared = read_json(&gateway);
        assert!(cleared["providers"][0].get("routing").is_none());
        assert!(cleared["providers"][0].get("affinity").is_none());
        assert_eq!(cleared["providers"][0]["note"], "keep-me");
        let page = load_routing_page("p1");
        assert_eq!(page.provider.as_ref().unwrap().routing, "smart");
        assert_eq!(page.provider.as_ref().unwrap().affinity, "auto");

        save_routing_control("group", "group/fast", "order", "off").unwrap();
        let grouped = read_json(&gateway);
        assert_eq!(grouped["groups"][0]["id"], "fast");
        assert_eq!(grouped["groups"][0]["routing"], "order");
        assert_eq!(grouped["groups"][0]["affinity"], "off");
        assert_eq!(grouped["groups"][0]["members"][1], "b/m");
        let page = load_routing_page("");
        assert!(page.provider.is_none());
        assert_eq!(page.groups.len(), 1);
        assert_eq!(page.groups[0].id, "fast");
        assert_eq!(page.groups[0].routing, "order");
        assert_eq!(page.groups[0].affinity, "off");
        let again = load_routing_page("p1");
        assert_eq!(again.groups[0].routing, "order");

        let before = fs::read(&gateway).unwrap();
        let err = save_routing_control("provider", "p1", "fastest", "auto").unwrap_err();
        assert_eq!(err, RoutingControlError::Unknown);
        assert_eq!(fs::read(&gateway).unwrap(), before);
        let err = save_routing_control("nope", "p1", "rotate", "auto").unwrap_err();
        assert_eq!(err, RoutingControlError::Unknown);
        assert_eq!(fs::read(&gateway).unwrap(), before);

        fs::write(&gateway, b"{").unwrap();
        let err = save_routing_control("provider", "p1", "rotate", "auto").unwrap_err();
        assert_eq!(err, RoutingControlError::Store);
        assert_eq!(fs::read(&gateway).unwrap(), b"{");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routing_control_does_not_rewrite_provider_store() {
        let _lock = ENV_LOCK.lock().await;
        let root = scratch("routing-store");
        let _scratch = Scratch(root.clone());
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(data.join("config")).unwrap();
        let _env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
            ("SKILLSTAR_DATA_DIR", &data),
        ]);
        let secret = "sk-secret-value";
        let host = "https://api.deepseek.com/v1";
        let store = data.join("config").join("model_providers.json");
        let body = format!(
            r#"{{"version":4,"providers":[{{"id":"p1","api_key":"{secret}","base_url":"{host}"}}]}}"#
        );
        fs::write(&store, &body).unwrap();
        let before = fs::read(&store).unwrap();

        save_routing_control("provider", "p1", "rotate", "auto").unwrap();

        let after = fs::read(&store).unwrap();
        assert_eq!(after, before, "the v4 store bytes stay put");
        let parsed: Value = serde_json::from_slice(&after).unwrap();
        assert_eq!(parsed["version"], 4);
        assert!(parsed["providers"][0].get("routing").is_none());
        assert!(parsed["providers"][0].get("affinity").is_none());

        let gateway = fs::read_to_string(data.join("config").join("model_gateway.json")).unwrap();
        let saved: Value = serde_json::from_str(&gateway).unwrap();
        assert_eq!(saved["providers"][0]["routing"], "rotate");
        assert!(saved["providers"][0].get("affinity").is_none());
        assert!(!gateway.contains(secret), "{gateway}");
        assert!(!gateway.contains("api.deepseek.com"), "{gateway}");
        assert!(!gateway.contains("https://"), "{gateway}");
    }

    fn read_json(path: &PathBuf) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "skillstar-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
