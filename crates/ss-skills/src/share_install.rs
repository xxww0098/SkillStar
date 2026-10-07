//! Unified share-code install pipeline.
//!
//! Both `ImportModal` (My Skills) and `ImportShareCodeModal` (Decks) used to
//! re-implement the "for each share entry: embedded vs repo vs skip" loop in
//! TypeScript. This command centralizes that logic in Rust so the two UIs, the
//! CLI, and future automations can share one install pipeline.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::git_skill::GitSkillFacade;
use crate::source_resolver::Source;
use crate::{installed_skill, local_skill};

/// Upper bound for a decompressed share-code body. Real payloads are a few KB;
/// anything larger is a deflate bomb, not a skill list.
const MAX_SHARE_PAYLOAD_BYTES: u64 = 1024 * 1024;

/// A single skill entry in a share code payload. Keys match the TypeScript
/// `ShareCodeData` shape (abbreviated for density).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShareCodeSkill {
    /// Skill name (human-readable id).
    pub n: String,
    /// Git URL when the skill is git-backed. May be empty when `c` is set.
    #[serde(default)]
    pub u: String,
    /// Inline SKILL.md content (Base64 UTF-8). Optional.
    #[serde(default)]
    pub c: Option<String>,
    /// `true` if the repo requires auth (private).
    #[serde(default)]
    pub p: Option<bool>,
}

/// Normalized remote a share entry points at, for the preview and the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRemote {
    pub host: String,
    /// `owner/repo` (or the group path for nested hosts).
    pub repo: String,
}

impl ShareRemote {
    pub fn label(&self) -> String {
        format!("{}/{}", self.host, self.repo)
    }
}

impl ShareCodeSkill {
    /// The remote this entry installs from. Only https and SSH Git remotes are
    /// accepted: a share code comes from someone else, so a local path or
    /// `file://` URL must never make the importer read its own disk.
    pub fn remote(&self) -> anyhow::Result<ShareRemote> {
        share_remote(&self.u)
    }
}

pub fn share_remote(url: &str) -> anyhow::Result<ShareRemote> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let scp_like = !lower.contains("://") && url.contains('@') && url.contains(':');
    if !(lower.starts_with("https://") || lower.starts_with("ssh://") || scp_like) {
        bail!("Share code source {url:?} is not an https:// or SSH Git remote");
    }
    let source =
        Source::parse(url).with_context(|| format!("Invalid share code source {url:?}"))?;
    let host = remote_host(url)
        .filter(|host| !host.is_empty())
        .with_context(|| format!("Share code source {url:?} has no host"))?;
    Ok(ShareRemote {
        host: host.to_ascii_lowercase(),
        repo: source.short,
    })
}

fn remote_host(url: &str) -> Option<String> {
    let authority = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("HTTPS://"))
        .or_else(|| url.strip_prefix("ssh://"))
    {
        rest.split(['/', '?', '#']).next()?
    } else {
        url.split_once(':')?.0
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|ch| ch.is_ascii_digit()))
        .map_or(host, |(host, _)| host);
    Some(host.to_string())
}

/// Per-skill install outcome returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShareSkillOutcome {
    /// Already installed in the hub before this run.
    Existing { name: String },
    /// Freshly installed from a repo.
    Installed { name: String },
    /// Installed by decoding the embedded SKILL.md content.
    Embedded { name: String },
    /// Skipped because neither repo nor embedded content resolved.
    Skipped { name: String, reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareCodeInstallSummary {
    pub requested_count: usize,
    pub installed_names: Vec<String>,
    pub existing_names: Vec<String>,
    pub embedded_names: Vec<String>,
    pub skipped: Vec<SkippedSkill>,
    pub outcomes: Vec<ShareSkillOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedSkill {
    pub name: String,
    pub reason: String,
    /// The underlying error, when one explains the skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ShareCodeInstallSummary {
    /// Names now present in the hub because of, or before, this run.
    pub fn available_names(&self) -> Vec<String> {
        self.installed_names
            .iter()
            .chain(&self.existing_names)
            .chain(&self.embedded_names)
            .cloned()
            .collect()
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

fn decode_embedded(base64: &str) -> anyhow::Result<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.trim())
        .context("Base64 decode failed")?;
    String::from_utf8(bytes).context("Embedded content is not valid UTF-8")
}

fn is_installed_in_hub(skill_name: &str) -> bool {
    let Ok(folder) = crate::materialize::canonical_skill_name(skill_name) else {
        return false;
    };
    ss_core::infra::paths::hub_skills_dir()
        .join(folder)
        .symlink_metadata()
        .is_ok()
}

fn install_embedded(name: &str, encoded: &str) -> anyhow::Result<()> {
    let content = decode_embedded(encoded)?;
    local_skill::create(name, Some(&content))?;
    // Frontmatter gate: embedded content must be a valid skill too. Roll back
    // the just-created local skill when it is not.
    let created_dir = ss_core::infra::paths::local_skills_dir().join(name);
    if let Err(reason) = crate::validation::ensure_installable(&created_dir) {
        let _ = local_skill::delete(name);
        bail!("Embedded SKILL.md rejected: {reason}");
    }
    installed_skill::invalidate_cache();
    Ok(())
}

/// Install a share code through the user's stored Git credentials, so private
/// repositories resolve the same way as a direct repository install.
pub fn install_from_share_code(skills: Vec<ShareCodeSkill>) -> ShareCodeInstallSummary {
    install_from_share_code_with(&GitSkillFacade::from_file_store(), skills)
}

/// Same as [`install_from_share_code`], on a caller-owned facade so the caller
/// can stream progress and cancel between entries.
pub fn install_from_share_code_with(
    git: &GitSkillFacade,
    skills: Vec<ShareCodeSkill>,
) -> ShareCodeInstallSummary {
    let mut summary = ShareCodeInstallSummary {
        requested_count: skills.len(),
        installed_names: Vec::new(),
        existing_names: Vec::new(),
        embedded_names: Vec::new(),
        skipped: Vec::new(),
        outcomes: Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();

    for entry in skills {
        let name = entry.n.trim().to_string();
        if name.is_empty() || !seen.insert(normalize(&name)) {
            continue;
        }
        if git.session().is_cancelled() {
            summary.skip(name, "cancelled", None);
            continue;
        }
        if is_installed_in_hub(&name) {
            summary.existing_names.push(name.clone());
            summary.outcomes.push(ShareSkillOutcome::Existing { name });
            continue;
        }

        let mut failure = None;
        if !entry.u.trim().is_empty() {
            match entry.remote() {
                Ok(_) => match git.install_skills_batch(&entry.u, std::slice::from_ref(&name)) {
                    Ok(result) if !result.is_empty() => {
                        installed_skill::invalidate_cache();
                        for skill in result {
                            summary.installed_names.push(skill.name.clone());
                            summary
                                .outcomes
                                .push(ShareSkillOutcome::Installed { name: skill.name });
                        }
                        continue;
                    }
                    Ok(_) => {
                        debug!(target: "share_install", skill = %name, "repo scan produced no matches, trying embedded");
                        failure = Some((
                            "install_failed",
                            format!("{name} was not found in {}", entry.u),
                        ));
                    }
                    Err(err) => {
                        warn!(target: "share_install", skill = %name, error = %err, "repo install failed, trying embedded");
                        failure = Some(("install_failed", err));
                    }
                },
                Err(err) => {
                    warn!(target: "share_install", skill = %name, error = %err, "share entry source rejected");
                    failure = Some(("unsupported_source", err.to_string()));
                }
            }
        }

        let Some(encoded) = entry.c.as_deref() else {
            let (reason, detail) = failure.unwrap_or(("no_source", String::new()));
            summary.skip(name, reason, Some(detail).filter(|d| !d.is_empty()));
            continue;
        };
        match install_embedded(&name, encoded) {
            Ok(()) => {
                summary.embedded_names.push(name.clone());
                summary.outcomes.push(ShareSkillOutcome::Embedded { name });
            }
            Err(err) => {
                warn!(target: "share_install", skill = %name, error = %err, "embedded install failed");
                let (reason, detail) = failure.unwrap_or(("embedded_failed", format!("{err:#}")));
                summary.skip(name, reason, Some(detail));
            }
        }
    }

    summary
}

impl ShareCodeInstallSummary {
    fn skip(&mut self, name: String, reason: &str, detail: Option<String>) {
        self.skipped.push(SkippedSkill {
            name: name.clone(),
            reason: reason.to_string(),
            detail,
        });
        self.outcomes.push(ShareSkillOutcome::Skipped {
            name,
            reason: reason.to_string(),
        });
    }
}

/// Decoded share-code body. Field names match the TypeScript `ShareCodeData`.
#[derive(Debug, Clone, Deserialize)]
pub struct ShareCodePayload {
    pub n: String,
    #[serde(default)]
    pub d: String,
    #[serde(default)]
    pub i: String,
    pub s: Vec<ShareCodeSkill>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShareCodeKind {
    Skills,
    Deck,
}

#[derive(Debug, Clone)]
pub struct ParsedShareCode {
    pub kind: ShareCodeKind,
    pub payload: ShareCodePayload,
}

const SHARE_TTL_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1000.0;

/// Pull `ags-` / `agd-` out of a pasted share message, then decode it.
/// Same layout as `src/lib/shareCode.ts`: prefix, base64, version byte,
/// compression flag, little-endian f64 timestamp, raw deflate or JSON.
pub fn parse_share_code(text: &str) -> Result<ParsedShareCode, String> {
    let code = extract_share_code(text);
    let (kind, body) = if let Some(body) = code.strip_prefix("ags-") {
        (ShareCodeKind::Skills, body)
    } else if let Some(body) = code.strip_prefix("agd-") {
        (ShareCodeKind::Deck, body)
    } else {
        return Err("Invalid share code prefix (expected ags- or agd-)".into());
    };

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(body))
        .map_err(|_| "Share code is corrupted (Base64 decode error)".to_string())?;
    if bytes.len() < 11 {
        return Err("Share code is too short, possibly corrupted".into());
    }
    let compressed = bytes[1] == 1;
    let mut ts = [0u8; 8];
    ts.copy_from_slice(&bytes[2..10]);
    let timestamp = f64::from_le_bytes(ts);
    let expires_at = timestamp + SHARE_TTL_MS;
    let now = chrono::Utc::now().timestamp_millis() as f64;
    if now > expires_at {
        return Err("Share code expired (valid for 7 days)".into());
    }

    let payload = &bytes[10..];
    let json = if compressed {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(payload)
            .take(MAX_SHARE_PAYLOAD_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|err| format!("Share code decompression failed: {err}"))?;
        out
    } else {
        payload.to_vec()
    };
    if json.len() as u64 > MAX_SHARE_PAYLOAD_BYTES {
        return Err(format!(
            "Share code payload exceeds {} KB",
            MAX_SHARE_PAYLOAD_BYTES / 1024
        ));
    }
    let payload: ShareCodePayload =
        serde_json::from_slice(&json).map_err(|_| "Share code data format mismatch".to_string())?;
    Ok(ParsedShareCode { kind, payload })
}

fn extract_share_code(text: &str) -> &str {
    let text = text.trim();
    let Some(start) = text.find("ags-").or_else(|| text.find("agd-")) else {
        return text;
    };
    let rest = &text[start..];
    let end = rest
        .find(
            |c: char| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' | '=' | '_' | '-'),
        )
        .unwrap_or(rest.len());
    &rest[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_entries_without_a_source_as_skipped() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        let summary = install_from_share_code(vec![ShareCodeSkill {
            n: "demo".to_string(),
            u: String::new(),
            c: None,
            p: None,
        }]);

        assert_eq!(summary.requested_count, 1);
        assert!(matches!(
            summary.outcomes.as_slice(),
            [ShareSkillOutcome::Skipped { name, reason }]
                if name == "demo" && reason == "no_source"
        ));
    }

    #[test]
    fn parses_an_uncompressed_share_code() {
        let payload = br#"{"n":"Pack","d":"Tools","i":"box","s":[{"n":"demo","u":"https://example.com/demo.git"}]}"#;
        let mut bytes = vec![2, 0];
        let now = chrono::Utc::now().timestamp_millis() as f64;
        bytes.extend_from_slice(&now.to_le_bytes());
        bytes.extend_from_slice(payload);
        use base64::Engine as _;
        let code = format!(
            "ags-{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let parsed = parse_share_code(&format!("see {code} thanks")).unwrap();
        assert_eq!(parsed.kind, ShareCodeKind::Skills);
        assert_eq!(parsed.payload.n, "Pack");
        assert_eq!(parsed.payload.s.len(), 1);
        assert_eq!(parsed.payload.s[0].n, "demo");
    }

    #[test]
    fn rejects_an_expired_share_code() {
        let mut bytes = vec![2, 0];
        bytes.extend_from_slice(&0f64.to_le_bytes());
        bytes.extend_from_slice(br#"{"n":"Pack","s":[]}"#);
        use base64::Engine as _;
        let code = format!(
            "agd-{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        assert!(parse_share_code(&code).unwrap_err().contains("expired"));
    }

    #[test]
    fn contains_invalid_embedded_content_as_a_per_skill_failure() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        let summary = install_from_share_code(vec![ShareCodeSkill {
            n: "demo".to_string(),
            u: String::new(),
            c: Some("not base64".to_string()),
            p: None,
        }]);

        assert!(matches!(
            summary.outcomes.as_slice(),
            [ShareSkillOutcome::Skipped { reason, .. }] if reason == "embedded_failed"
        ));
    }

    #[test]
    fn normalizes_https_and_ssh_remotes_for_the_preview() {
        let https = share_remote("https://GitHub.com/Owner/Repo.git").unwrap();
        assert_eq!(https.host, "github.com");
        assert_eq!(https.label(), "github.com/Owner/Repo");
        let scp = share_remote("git@gitlab.example.com:team/tools.git").unwrap();
        assert_eq!(scp.host, "gitlab.example.com");
        let ssh = share_remote("ssh://git@git.example.com:2222/team/tools.git").unwrap();
        assert_eq!(ssh.host, "git.example.com");
    }

    #[test]
    fn rejects_local_and_plaintext_share_sources() {
        for url in [
            "/Users/me/secret-skills",
            "./skills",
            "file:///etc",
            "http://example.com/owner/repo",
            "owner/repo",
        ] {
            assert!(share_remote(url).is_err(), "{url} must be rejected");
        }
    }

    #[test]
    fn local_path_entries_are_skipped_without_touching_the_disk() {
        let sandbox = crate::test_sandbox::Sandbox::new();
        let outside = sandbox.root().join("outside/demo");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            outside.join("SKILL.md"),
            "---\nname: demo\ndescription: d\n---\n",
        )
        .unwrap();

        let summary = install_from_share_code(vec![ShareCodeSkill {
            n: "demo".to_string(),
            u: outside.parent().unwrap().to_string_lossy().into_owned(),
            c: None,
            p: None,
        }]);

        assert!(summary.installed_names.is_empty());
        assert_eq!(summary.skipped.len(), 1);
        assert_eq!(summary.skipped[0].reason, "unsupported_source");
        assert!(summary.skipped[0].detail.is_some());
        assert!(
            !ss_core::infra::paths::hub_skills_dir()
                .join("demo")
                .exists()
        );
    }

    #[test]
    fn rejects_a_share_code_that_inflates_past_the_cap() {
        use std::io::Write as _;
        let mut json = br#"{"n":"Pack","s":[],"d":""#.to_vec();
        json.extend(std::iter::repeat_n(b'a', 2 * 1024 * 1024));
        json.extend_from_slice(br#""}"#);
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&json).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut bytes = vec![2, 1];
        let now = chrono::Utc::now().timestamp_millis() as f64;
        bytes.extend_from_slice(&now.to_le_bytes());
        bytes.extend_from_slice(&compressed);
        use base64::Engine as _;
        let code = format!(
            "ags-{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        assert!(parse_share_code(&code).unwrap_err().contains("exceeds"));
    }
}
