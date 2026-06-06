//! Qoder OpenAPI request headers (machine token cache from default Qoder IDE install).

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;

use crate::tool_paths::qoder_machine_token_path;

#[derive(Debug, Clone, Default)]
pub struct QoderMachineInfo {
    pub token: Option<String>,
    pub machine_type: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct MachineTokenCache {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, rename = "machineType")]
    machine_type: Option<String>,
    #[serde(default, rename = "machine_type")]
    machine_type_snake: Option<String>,
}

pub fn read_machine_info() -> QoderMachineInfo {
    let path = qoder_machine_token_path();
    if !path.exists() {
        return QoderMachineInfo::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return QoderMachineInfo::default();
    };
    let parsed: MachineTokenCache = serde_json::from_str(&content).unwrap_or_default();
    QoderMachineInfo {
        token: parsed.token.filter(|s| !s.trim().is_empty()),
        machine_type: parsed
            .machine_type
            .or(parsed.machine_type_snake)
            .filter(|s| !s.trim().is_empty()),
    }
}

pub fn build_qoder_headers(access_token: &str, machine: &QoderMachineInfo) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
        headers.insert(AUTHORIZATION, v);
    }
    if let Ok(v) = HeaderValue::from_str("application/json") {
        headers.insert(ACCEPT, v);
    }
    insert_header(&mut headers, "Cosy-MachineToken", machine.token.as_deref());
    insert_header(
        &mut headers,
        "Cosy-MachineType",
        machine.machine_type.as_deref(),
    );
    insert_header(
        &mut headers,
        "Cosy-MachineOS",
        Some(cosy_machine_os().as_str()),
    );
    insert_header(&mut headers, "Cosy-ClientType", Some("0"));
    headers
}

fn insert_header(headers: &mut HeaderMap, name: &str, value: Option<&str>) {
    let Some(value) = value.filter(|s| !s.is_empty()) else {
        return;
    };
    let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
        return;
    };
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(header_name, v);
    }
}

fn cosy_machine_os() -> String {
    match std::env::consts::OS {
        "macos" => "darwin".to_string(),
        "windows" => "win32".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
}
