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

/// Normalize pasted browser cookie text before parsing.
///
/// Users often copy the full `Cookie:` request header line or include stray
/// newlines from DevTools — strip those so we only persist real `name=value`
/// pairs.
pub fn normalize_cookie_header(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return text;
    }

    // Collapse line breaks that sometimes appear when copying from DevTools.
    text = text.replace(['\r', '\n'], "; ");

    // Strip a leading `Cookie:` label whether it is on its own line or inline.
    if let Some(rest) = text.strip_prefix("Cookie:") {
        text = rest.trim().to_string();
    } else if let Some(rest) = text.strip_prefix("cookie:") {
        text = rest.trim().to_string();
    }

    text
}

pub fn parse_cookie_header(raw: &str) -> Vec<CookieEntry> {
    let normalized = normalize_cookie_header(raw);
    normalized
        .split(';')
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_cookie_label_prefix() {
        let raw = "Cookie: session=abc; token=xyz";
        let entries = parse_cookie_header(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "session");
        assert_eq!(entries[0].value, "abc");
        assert_eq!(entries[1].name, "token");
        assert_eq!(entries[1].value, "xyz");
    }

    #[test]
    fn collapses_newlines_before_parsing() {
        let raw = "session=abc;\ntoken=xyz";
        let entries = parse_cookie_header(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(build_cookie_header(&entries), "session=abc; token=xyz");
    }
}
