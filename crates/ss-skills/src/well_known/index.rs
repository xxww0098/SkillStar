//! Pure well-known index selection.
//!
//! A path-scoped URL that misses must not install the host root catalog.

use url::Url;

pub(crate) const SCHEMA_V2: &str = "https://schemas.agentskills.io/discovery/0.2.0/schema.json";
pub(crate) const WELL_KNOWN_PATHS: [&str; 2] = [".well-known/agent-skills", ".well-known/skills"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    pub path: String,
    pub root_base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexCandidate {
    pub index_url: String,
    pub resolved_base: String,
    pub well_known_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactKind {
    SkillMd,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IndexEntry {
    V2 {
        name: String,
        kind: ArtifactKind,
        artifact_url: String,
        digest: String,
    },
    V1 {
        name: String,
        files: Vec<String>,
        base_url: String,
        well_known_path: String,
    },
}

impl IndexEntry {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::V2 { name, .. } | Self::V1 { name, .. } => name,
        }
    }

    pub(crate) fn identity(&self) -> String {
        match self {
            Self::V2 {
                name,
                kind,
                artifact_url,
                digest,
            } => {
                let kind = match kind {
                    ArtifactKind::SkillMd => "skill-md",
                    ArtifactKind::Archive => "archive",
                };
                format!("v2\n{name}\n{kind}\n{artifact_url}\n{digest}")
            }
            Self::V1 { name, files, .. } => format!("v1\n{name}\n{}", files.join("\n")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchedIndex {
    pub resolved_base: String,
    pub entries: Vec<IndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeMiss {
    pub scope_path: String,
    pub root_url: String,
}

impl ScopeMiss {
    pub(crate) fn message(&self) -> String {
        format!(
            "No skills found for the scoped path '{}' on {}. Not falling back to the root skills index because that would install every skill the host publishes.",
            self.scope_path,
            display_url(&self.root_url)
        )
    }
}

pub(crate) fn is_candidate(input: &str) -> bool {
    let Ok(url) = Url::parse(input.trim()) else {
        return false;
    };
    if url.scheme() != "http" && url.scheme() != "https" {
        return false;
    }
    if input.trim().ends_with(".git") || input.contains('#') {
        return false;
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    !matches!(
        host.as_str(),
        "github.com" | "www.github.com" | "gitlab.com" | "raw.githubusercontent.com"
    )
}

pub(crate) fn scope_of(input: &str) -> Option<Scope> {
    let url = Url::parse(input.trim()).ok()?;
    let path = url.path().trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    Some(Scope {
        path: path.to_string(),
        root_base: origin_of(&url),
    })
}

pub(crate) fn index_candidates(input: &str) -> Vec<IndexCandidate> {
    let Ok(url) = Url::parse(input.trim()) else {
        return Vec::new();
    };
    if url.scheme() != "http" && url.scheme() != "https" {
        return Vec::new();
    }
    let origin = origin_of(&url);
    let base_path = url.path().trim_end_matches('/');
    let mut out = Vec::new();
    for well_known_path in WELL_KNOWN_PATHS {
        let resolved_base = format!("{origin}{base_path}");
        out.push(IndexCandidate {
            index_url: format!("{resolved_base}/{well_known_path}/index.json"),
            resolved_base,
            well_known_path: well_known_path.to_string(),
        });
        if !base_path.is_empty() {
            out.push(IndexCandidate {
                index_url: format!("{origin}/{well_known_path}/index.json"),
                resolved_base: origin.clone(),
                well_known_path: well_known_path.to_string(),
            });
        }
    }
    out
}

pub(crate) fn indexes_to_try<'a>(
    scope: Option<&Scope>,
    hits: &'a [FetchedIndex],
) -> Vec<&'a FetchedIndex> {
    match scope {
        Some(scope) => hits
            .iter()
            .filter(|hit| hit.resolved_base != scope.root_base)
            .collect(),
        None => hits.iter().collect(),
    }
}

pub(crate) fn scope_miss_if_root_published(
    scope: Option<&Scope>,
    hits: &[FetchedIndex],
) -> Option<ScopeMiss> {
    let scope = scope?;
    hits.iter()
        .any(|hit| hit.resolved_base == scope.root_base)
        .then(|| ScopeMiss {
            scope_path: scope.path.clone(),
            root_url: scope.root_base.clone(),
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedIndex {
    Entries(Vec<IndexEntry>),
    /// A JSON index whose schema SkillStar must not install or Git-clone past.
    Rejected,
}

pub(crate) fn parse_index(
    raw: &serde_json::Value,
    index_url: &str,
    resolved_base: &str,
    well_known_path: &str,
) -> Option<ParsedIndex> {
    let record = raw.as_object()?;
    let skills = record.get("skills")?.as_array()?;
    match record.get("$schema").and_then(|value| value.as_str()) {
        Some(SCHEMA_V2) => Some(parse_v2(skills, index_url)),
        Some(_) => Some(ParsedIndex::Rejected),
        None => parse_v1(skills, resolved_base, well_known_path).map(ParsedIndex::Entries),
    }
}

fn parse_v2(skills: &[serde_json::Value], index_url: &str) -> ParsedIndex {
    let mut entries = Vec::new();
    for skill in skills {
        let Some(entry) = skill.as_object() else {
            continue;
        };
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        if !valid_skill_name(name) {
            continue;
        }
        let Some(description) = entry.get("description").and_then(|value| value.as_str()) else {
            continue;
        };
        if description.is_empty() || description.len() > 1024 {
            continue;
        }
        let kind = match entry.get("type").and_then(|value| value.as_str()) {
            Some("skill-md") => ArtifactKind::SkillMd,
            Some("archive") => ArtifactKind::Archive,
            _ => continue,
        };
        let Some(url) = entry.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(artifact_url) = http_url(url, index_url) else {
            continue;
        };
        let Some(digest) = entry.get("digest").and_then(|value| value.as_str()) else {
            continue;
        };
        if !valid_digest(digest) {
            continue;
        }
        entries.push(IndexEntry::V2 {
            name: name.to_string(),
            kind,
            artifact_url,
            digest: digest.to_string(),
        });
    }
    ParsedIndex::Entries(entries)
}

fn parse_v1(
    skills: &[serde_json::Value],
    resolved_base: &str,
    well_known_path: &str,
) -> Option<Vec<IndexEntry>> {
    let mut entries = Vec::new();
    for skill in skills {
        let entry = skill.as_object()?;
        let name = entry.get("name").and_then(|value| value.as_str())?;
        if !valid_skill_name(name) {
            return None;
        }
        let description = entry.get("description").and_then(|value| value.as_str())?;
        if description.is_empty() {
            return None;
        }
        let files = entry.get("files").and_then(|value| value.as_array())?;
        if files.is_empty() {
            return None;
        }
        let mut paths = Vec::new();
        for file in files {
            let path = file.as_str()?;
            if !safe_legacy_file(path) {
                return None;
            }
            paths.push(path.to_string());
        }
        if !paths
            .iter()
            .any(|path| path.eq_ignore_ascii_case("SKILL.md"))
        {
            return None;
        }
        entries.push(IndexEntry::V1 {
            name: name.to_string(),
            files: paths,
            base_url: resolved_base.to_string(),
            well_known_path: well_known_path.to_string(),
        });
    }
    Some(entries)
}

pub(crate) fn valid_skill_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn valid_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_legacy_file(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\0')
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
}

pub(crate) fn http_url(value: &str, base: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    let joined = base.join(value).ok()?;
    matches!(joined.scheme(), "http" | "https").then(|| joined.to_string())
}

fn origin_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or("");
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let auth = match (url.username(), url.password()) {
        ("", None) => String::new(),
        (user, None) => format!("{user}@"),
        (user, Some(password)) => format!("{user}:{password}@"),
    };
    match url.port() {
        Some(port) => format!("{}://{auth}{host}:{port}", url.scheme()),
        None => format!("{}://{auth}{host}", url.scheme()),
    }
}

pub(crate) fn display_url(input: &str) -> String {
    let Ok(mut url) = Url::parse(input) else {
        return input.to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.to_string()
}
