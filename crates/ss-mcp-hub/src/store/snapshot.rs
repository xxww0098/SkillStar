use crate::formats::paste::sanitize_slug;
use crate::model::{McpServer, StoreInputSpec, StoreItem, Transport};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SNAPSHOT_JSON: &str = include_str!("../../assets/market_servers.json");

static SNAPSHOT_CACHE: OnceLock<Vec<RawMarketServer>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMarketServer {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub repository: Option<RawRepo>,
    #[serde(default)]
    pub icons: Vec<RawIcon>,
    #[serde(default)]
    pub packages: Vec<RawPackage>,
    #[serde(default)]
    pub remotes: Vec<RawRemote>,
    #[serde(default)]
    pub stars: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRepo {
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawIcon {
    #[serde(default)]
    pub src: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPackage {
    #[serde(default)]
    pub registry_type: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub runtime_hint: String,
    #[serde(default)]
    pub transport: Option<RawTransport>,
    #[serde(default)]
    pub package_arguments: Vec<RawArg>,
    #[serde(default)]
    pub runtime_arguments: Vec<RawArg>,
    #[serde(default)]
    pub environment_variables: Vec<RawVar>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawTransport {
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawArg {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub value_hint: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawVar {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRemote {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<RawVar>,
}

fn parsed_snapshot() -> &'static [RawMarketServer] {
    SNAPSHOT_CACHE.get_or_init(|| serde_json::from_str(SNAPSHOT_JSON).unwrap_or_default())
}

/// Convert a registry/snapshot server record into an installable `StoreItem` template.
pub fn convert_raw_server(raw: &RawMarketServer, source: &str) -> Option<StoreItem> {
    let short_name = raw
        .name
        .rsplit('/')
        .next()
        .map(sanitize_slug)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| sanitize_slug(&raw.name));

    let publisher = raw
        .name
        .split('/')
        .next()
        .and_then(|ns| ns.rsplit('.').next())
        .unwrap_or("")
        .to_string();
    let title = if raw.title.trim().is_empty() {
        short_name.clone()
    } else {
        raw.title.trim().to_string()
    };
    let homepage = raw
        .repository
        .as_ref()
        .map(|r| r.url.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(raw.website_url.as_str())
        .to_string();
    let icon = raw
        .icons
        .first()
        .map(|i| i.src.clone())
        .unwrap_or_default();

    // Prefer stdio package when available with npx / uvx / docker, otherwise remote endpoint.
    if let Some((cmd, args, env, inputs, runs)) = pick_package(&raw.packages) {
        return Some(StoreItem {
            id: raw.name.clone(),
            name: short_name.clone(),
            title,
            description: raw.description.clone(),
            publisher,
            icon,
            homepage,
            transport: Transport::Stdio,
            runs,
            sign_in: false,
            featured: false,
            match_by_addr: false,
            source: source.to_string(),
            stars: raw.stars,
            have: None,
            inputs,
            template: McpServer {
                name: short_name,
                transport: Transport::Stdio,
                command: cmd,
                args,
                env,
                market_id: Some(raw.name.clone()),
                ..Default::default()
            },
        });
    }

    if let Some(remote) = raw.remotes.first().filter(|r| !r.url.trim().is_empty()) {
        let transport = if remote.kind.eq_ignore_ascii_case("sse") {
            Transport::Sse
        } else {
            Transport::Http
        };
        let mut headers = BTreeMap::new();
        let mut inputs = Vec::new();
        for hdr in &remote.headers {
            if !hdr.value.is_empty() && !hdr.value.contains('{') {
                headers.insert(hdr.name.clone(), hdr.value.clone());
            } else {
                let format = if hdr.value.contains('{') {
                    // Convert `{TOKEN}` placeholder to `Bearer {}` style if applicable.
                    if hdr.value.starts_with("Bearer ") {
                        "Bearer {}".to_string()
                    } else {
                        "{}".to_string()
                    }
                } else {
                    String::new()
                };
                inputs.push(StoreInputSpec {
                    key: hdr.name.clone(),
                    where_field: "header".into(),
                    label: hdr.name.clone(),
                    description: hdr.description.clone(),
                    placeholder: String::new(),
                    secret: hdr.is_secret,
                    required: hdr.is_required,
                    format,
                });
            }
        }
        return Some(StoreItem {
            id: raw.name.clone(),
            name: short_name.clone(),
            title,
            description: raw.description.clone(),
            publisher,
            icon,
            homepage,
            transport,
            runs: String::new(),
            sign_in: inputs.is_empty() && raw.description.to_lowercase().contains("oauth"),
            featured: false,
            match_by_addr: false,
            source: source.to_string(),
            stars: raw.stars,
            have: None,
            inputs,
            template: McpServer {
                name: short_name,
                transport,
                url: remote.url.trim().to_string(),
                headers,
                market_id: Some(raw.name.clone()),
                ..Default::default()
            },
        });
    }

    None
}

type PickedPackage = (
    String,
    Vec<String>,
    BTreeMap<String, String>,
    Vec<StoreInputSpec>,
    String,
);

fn pick_package(packages: &[RawPackage]) -> Option<PickedPackage> {
    let pkg = packages
        .iter()
        .find(|p| p.registry_type == "npm" || p.registry_type == "pypi")
        .or_else(|| packages.first())?;
    if pkg
        .transport
        .as_ref()
        .is_some_and(|t| t.kind != "stdio" && !t.kind.is_empty())
    {
        return None;
    }

    let runner = if !pkg.runtime_hint.trim().is_empty() {
        pkg.runtime_hint.trim().to_string()
    } else {
        match pkg.registry_type.as_str() {
            "npm" => "npx".into(),
            "pypi" => "uvx".into(),
            "oci" | "docker" => "docker".into(),
            _ => return None,
        }
    };

    let mut args = Vec::new();
    match runner.as_str() {
        "npx" => args.push("-y".into()),
        "docker" => {
            args.push("run".into());
            args.push("--rm".into());
            args.push("-i".into());
        }
        _ => {}
    }
    for rarg in &pkg.runtime_arguments {
        append_raw_arg(&mut args, rarg);
    }
    if !pkg.identifier.trim().is_empty() {
        args.push(pkg.identifier.trim().to_string());
    }

    let mut inputs = Vec::new();
    for parg in &pkg.package_arguments {
        if !parg.value.trim().is_empty() {
            append_raw_arg(&mut args, parg);
        } else if !parg.default.trim().is_empty() {
            args.push(parg.default.trim().to_string());
        } else if !parg.value_hint.trim().is_empty() || parg.is_required {
            let key = if !parg.name.is_empty() {
                parg.name.clone()
            } else {
                parg.value_hint.clone()
            };
            inputs.push(StoreInputSpec {
                key,
                where_field: "arg".into(),
                label: if !parg.name.is_empty() {
                    parg.name.clone()
                } else {
                    parg.value_hint.clone()
                },
                description: parg.description.clone(),
                placeholder: parg.value_hint.clone(),
                secret: parg.is_secret,
                required: parg.is_required,
                format: if parg.kind == "named" && !parg.name.is_empty() {
                    format!("{} {{}}", parg.name)
                } else {
                    String::new()
                },
            });
        }
    }

    let mut env = BTreeMap::new();
    for var in &pkg.environment_variables {
        if var.name.is_empty() {
            continue;
        }
        if !var.value.trim().is_empty() {
            env.insert(var.name.clone(), var.value.clone());
        } else if var.is_required || var.is_secret {
            inputs.push(StoreInputSpec {
                key: var.name.clone(),
                where_field: "env".into(),
                label: var.name.clone(),
                description: var.description.clone(),
                placeholder: String::new(),
                secret: var.is_secret,
                required: var.is_required,
                format: String::new(),
            });
        }
    }

    let runs = runner.clone();
    Some((runner, args, env, inputs, runs))
}

fn append_raw_arg(args: &mut Vec<String>, arg: &RawArg) {
    if arg.kind == "named" && !arg.name.is_empty() {
        args.push(arg.name.clone());
        if !arg.value.trim().is_empty() {
            args.push(arg.value.trim().to_string());
        }
    } else if !arg.value.trim().is_empty() {
        args.push(arg.value.trim().to_string());
    }
}

/// Search the embedded offline `market_servers.json` snapshot by query string or repository/package address.
pub fn search_snapshot(query: &str, limit: usize) -> Vec<StoreItem> {
    let q = query.trim().to_lowercase();
    let addr = normalize_addr(&q);
    let raw_list = parsed_snapshot();
    let mut scored: Vec<(i32, u64, StoreItem)> = Vec::new();

    for raw in raw_list {
        let Some(mut item) = convert_raw_server(raw, "snapshot") else {
            continue;
        };
        if q.is_empty() {
            scored.push((1, raw.stars.unwrap_or(0), item));
            continue;
        }
        let mut score = 0i32;
        if let Some(a) = &addr
            && matches_raw_addr(raw, a)
        {
            item.match_by_addr = true;
            score += 500;
        }
        let id_l = item.id.to_lowercase();
        let name_l = item.name.to_lowercase();
        let title_l = item.title.to_lowercase();
        let desc_l = item.description.to_lowercase();
        if name_l == q || id_l == q {
            score += 300;
        } else if name_l.contains(&q) || title_l.contains(&q) {
            score += 150;
        } else if id_l.contains(&q) {
            score += 100;
        } else if desc_l.contains(&q) {
            score += 40;
        }
        if score > 0 {
            scored.push((score, raw.stars.unwrap_or(0), item));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    scored.into_iter().take(limit).map(|(_, _, it)| it).collect()
}

pub fn normalize_addr(raw: &str) -> Option<String> {
    let s = if let Some(rest) = raw
        .trim()
        .strip_prefix("npm:")
        .or_else(|| raw.trim().strip_prefix("NPM:"))
    {
        rest.trim().trim_end_matches('/')
    } else {
        raw.trim().trim_end_matches('/')
    };
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None;
    }
    let stripped = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let normalized = stripped.strip_suffix(".git").unwrap_or(stripped).to_lowercase();
    if normalized.contains('.') || normalized.contains('/') || normalized.starts_with('@') {
        Some(normalized)
    } else {
        None
    }
}

/// Returns `true` if `raw` is a bare PyPI package name without `.` or `/`
/// (e.g. `mcp-server-fetch`) so direct-address probing can reach PyPI.
pub(crate) fn is_valid_pypi_name(raw: &str) -> bool {
    let s = raw.trim();
    if s.len() < 2 || s.len() > 128 {
        return false;
    }
    let Some(first) = s.chars().next() else {
        return false;
    };
    let Some(last) = s.chars().last() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn matches_raw_addr(raw: &RawMarketServer, norm_addr: &str) -> bool {
    let candidates = [
        raw.repository.as_ref().map(|r| r.url.as_str()).unwrap_or(""),
        raw.website_url.as_str(),
    ];
    for c in candidates {
        if let Some(n) = normalize_addr(c)
            && (n == norm_addr || n.contains(norm_addr))
        {
            return true;
        }
    }
    for pkg in &raw.packages {
        if pkg.identifier.to_lowercase() == norm_addr {
            return true;
        }
    }
    for rem in &raw.remotes {
        if let Some(n) = normalize_addr(&rem.url)
            && (n == norm_addr || n.contains(norm_addr))
        {
            return true;
        }
    }
    false
}
