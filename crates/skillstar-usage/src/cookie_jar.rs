use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

pub fn parse_cookie_header(raw: &str) -> Vec<CookieEntry> {
    raw.split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() {
                return None;
            }
            Some(CookieEntry {
                name: name.to_string(),
                value: value.to_string(),
                domain: None,
                path: None,
                expires: None,
                http_only: false,
                secure: false,
                source_url: None,
            })
        })
        .collect()
}

pub fn serialize_cookie_jar(entries: &[CookieEntry]) -> String {
    serde_json::to_string(entries).unwrap_or_default()
}

pub fn deserialize_cookie_jar(json: &str) -> Option<Vec<CookieEntry>> {
    serde_json::from_str(json).ok()
}

pub fn build_cookie_header(entries: &[CookieEntry]) -> String {
    entries
        .iter()
        .map(|e| format!("{}={}", e.name, e.value))
        .collect::<Vec<_>>()
        .join("; ")
}
