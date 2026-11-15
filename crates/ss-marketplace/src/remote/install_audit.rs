//! Install-time security audits for confirmed-public github.com repositories.
//!
//! Skill names are sent only to `https://www.skills.sh/tele/audit`, and only
//! after `https://api.github.com` returns `private: false`. Private, missing,
//! enterprise, GitLab, local, and unknown sources return no rows and make no
//! audit request. A failed lookup never blocks installation. This is not
//! install telemetry.

use std::time::Duration;

use serde::Deserialize;

use super::SecurityAudit;
use ss_core::infra::http_client::probe_http_client_no_redirect;

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_AUDIT_SKILLS: usize = 50;
const GITHUB_API: &str = "https://api.github.com";
const AUDIT_ORIGIN: &str = "https://www.skills.sh";

/// Partner audit rows for one skill, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSecurityAudit {
    pub skill: String,
    pub audits: Vec<SecurityAudit>,
}

/// Audits for a confirmed-public `github.com` repository.
///
/// Empty when the source is not that host, GitHub does not confirm
/// `private: false`, the audit service fails, or no partner rows exist.
pub async fn fetch_public_github_skill_audits(
    repo_url: &str,
    skill_names: &[String],
) -> Vec<SkillSecurityAudit> {
    let Some((owner, repo)) = github_com_owner_repo(repo_url) else {
        return Vec::new();
    };
    let skills = audit_skill_names(skill_names);
    if skills.is_empty() {
        return Vec::new();
    }
    let Ok(client) = probe_http_client_no_redirect(LOOKUP_TIMEOUT) else {
        return Vec::new();
    };
    if !repo_is_confirmed_public(&client, &owner, &repo).await {
        return Vec::new();
    }
    let Some(url) = audit_request_url(&owner, &repo, &skills) else {
        return Vec::new();
    };
    let Ok(response) = client
        .get(&url)
        .header("User-Agent", concat!("SkillStar/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json")
        .send()
        .await
    else {
        return Vec::new();
    };
    if response.status() != reqwest::StatusCode::OK || !is_audit_url(response.url()) {
        return Vec::new();
    }
    let Ok(body) = response.text().await else {
        return Vec::new();
    };
    parse_audit_body(&body)
}

/// `https://github.com/owner/repo` only. Extra path segments, other hosts,
/// and unsafe names are not an audit source.
pub(crate) fn github_com_owner_repo(repo_url: &str) -> Option<(String, String)> {
    let rest = repo_url.trim().strip_prefix("https://github.com/")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.split('/');
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    if parts.next().is_some() || !is_github_name(owner) || !is_github_name(repo) {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn is_github_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Names safe to put in the audit query. Commas, slashes, and spaces are
/// dropped so they cannot change the query shape.
pub(crate) fn audit_skill_names(names: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            continue;
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            continue;
        }
        if out.iter().any(|existing: &String| existing == name) {
            continue;
        }
        out.push(name.to_string());
        if out.len() == MAX_AUDIT_SKILLS {
            break;
        }
    }
    out
}

pub(crate) fn confirmed_public(private_field: Option<bool>) -> bool {
    private_field == Some(false)
}

/// True only for an exact `https://github.com/owner/repo` clone URL.
pub fn may_query_install_audits(repo_url: &str) -> bool {
    github_com_owner_repo(repo_url).is_some()
}

fn confirmed_github_repo_url(url: &reqwest::Url, owner: &str, repo: &str) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("api.github.com")
        && url.path() == format!("/repos/{owner}/{repo}")
}

#[derive(Deserialize)]
struct RepoVisibility {
    private: Option<bool>,
}

async fn repo_is_confirmed_public(client: &reqwest::Client, owner: &str, repo: &str) -> bool {
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}");
    let Ok(response) = client
        .get(&url)
        .header("User-Agent", concat!("SkillStar/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
    else {
        return false;
    };
    if response.status() != reqwest::StatusCode::OK
        || !confirmed_github_repo_url(response.url(), owner, repo)
    {
        return false;
    }
    let Ok(body) = response.text().await else {
        return false;
    };
    serde_json::from_str::<RepoVisibility>(&body)
        .ok()
        .is_some_and(|repo| confirmed_public(repo.private))
}

pub(crate) fn audit_request_url(owner: &str, repo: &str, skills: &[String]) -> Option<String> {
    if skills.is_empty() || !is_github_name(owner) || !is_github_name(repo) {
        return None;
    }
    let skills = skills
        .iter()
        .map(|name| encode_query_component(name))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "{AUDIT_ORIGIN}/tele/audit?source={owner}/{repo}&skills={skills}"
    ))
}

fn is_audit_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("www.skills.sh")
        && url.path() == "/tele/audit"
}

fn encode_query_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Map the partner payload onto [`SecurityAudit`]. Unknown partners and
/// malformed rows are ignored. An empty list is not a pass.
pub(crate) fn parse_audit_body(body: &str) -> Vec<SkillSecurityAudit> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let Some(skills) = value.as_object() else {
        return Vec::new();
    };
    let mut parsed = Vec::new();
    for (skill, entry) in skills {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let audits = partner_rows(entry);
        if audits.is_empty() {
            continue;
        }
        parsed.push(SkillSecurityAudit {
            skill: skill.clone(),
            audits,
        });
    }
    parsed.sort_by(|left, right| left.skill.cmp(&right.skill));
    parsed
}

fn partner_rows(entry: &serde_json::Map<String, serde_json::Value>) -> Vec<SecurityAudit> {
    let mut rows = Vec::new();
    if let Some(label) = entry.get("ath").and_then(risk_label) {
        rows.push(SecurityAudit {
            name: "Gen".to_string(),
            result: label,
        });
    }
    if let Some(socket) = entry.get("socket").and_then(socket_label) {
        rows.push(SecurityAudit {
            name: "Socket".to_string(),
            result: socket,
        });
    }
    if let Some(label) = entry.get("snyk").and_then(risk_label) {
        rows.push(SecurityAudit {
            name: "Snyk".to_string(),
            result: label,
        });
    }
    rows
}

fn risk_label(value: &serde_json::Value) -> Option<String> {
    let risk = value.get("risk")?.as_str()?;
    let label = match risk {
        "safe" => "Safe",
        "low" => "Low Risk",
        "medium" => "Medium Risk",
        "high" => "High Risk",
        "critical" => "Critical",
        "unknown" => "Unknown",
        _ => return None,
    };
    Some(label.to_string())
}

fn socket_label(value: &serde_json::Value) -> Option<String> {
    let alerts = value.get("alerts")?.as_u64()?;
    Some(if alerts == 1 {
        "1 alert".to_string()
    } else {
        format!("{alerts} alerts")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_github_com_repos_are_audit_sources() {
        assert_eq!(
            github_com_owner_repo("https://github.com/acme/skills.git"),
            Some(("acme".into(), "skills".into()))
        );
        assert!(github_com_owner_repo("https://gitlab.com/acme/skills.git").is_none());
        assert!(github_com_owner_repo("https://github.example.com/acme/skills").is_none());
        assert!(github_com_owner_repo("https://github.com/acme/skills/tree/main").is_none());
        assert!(github_com_owner_repo("ext::sh -c id").is_none());
        assert!(github_com_owner_repo("/tmp/skills").is_none());
    }

    #[test]
    fn only_explicit_public_flag_allows_a_query() {
        assert!(confirmed_public(Some(false)));
        assert!(!confirmed_public(Some(true)));
        assert!(!confirmed_public(None));
    }

    #[test]
    fn unsafe_skill_names_never_enter_the_query() {
        let names = audit_skill_names(&[
            "demo".into(),
            "demo".into(),
            "bad name".into(),
            "a,b".into(),
            "../x".into(),
        ]);
        assert_eq!(names, vec!["demo".to_string()]);
        let url = audit_request_url("acme", "skills", &names).unwrap();
        assert_eq!(
            url,
            "https://www.skills.sh/tele/audit?source=acme/skills&skills=demo"
        );
        assert!(audit_request_url("acme", "skills", &[]).is_none());
    }

    #[test]
    fn partner_payload_maps_onto_security_audit_rows() {
        let parsed = parse_audit_body(
            r#"{
                "demo": {
                    "ath": {"risk": "low", "analyzedAt": "2026-10-01"},
                    "socket": {"alerts": 0, "risk": "safe"},
                    "snyk": {"risk": "critical"},
                    "other": {"risk": "high"}
                },
                "broken": "nope"
            }"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].skill, "demo");
        assert_eq!(
            parsed[0].audits,
            vec![
                SecurityAudit {
                    name: "Gen".into(),
                    result: "Low Risk".into(),
                },
                SecurityAudit {
                    name: "Socket".into(),
                    result: "0 alerts".into(),
                },
                SecurityAudit {
                    name: "Snyk".into(),
                    result: "Critical".into(),
                },
            ]
        );
        assert!(parse_audit_body("not-json").is_empty());
    }

    #[tokio::test]
    async fn non_github_sources_return_no_rows_without_a_query() {
        let rows = fetch_public_github_skill_audits(
            "https://gitlab.com/acme/skills.git",
            &["demo".to_string()],
        )
        .await;
        assert!(rows.is_empty());
        assert!(!may_query_install_audits("https://gitlab.com/acme/skills.git"));
    }
}
