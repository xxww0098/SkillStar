use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::*;

// ── Publisher Repos ───────────────────────────────────────────────────

/// A skill entry within a publisher repo (lightweight, for repo drill-down)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherRepoSkill {
    pub name: String,
    pub installs: u32,
}

/// A repo belonging to a publisher, scraped from skills.sh/<publisher>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherRepo {
    /// Repo name (e.g. "github-copilot-for-azure")
    pub repo: String,
    /// Full source path (e.g. "microsoft/github-copilot-for-azure")
    pub source: String,
    /// Number of skills in this repo
    pub skill_count: u32,
    /// Formatted install count string (e.g. "2.9M", "12.8K")
    pub installs_label: String,
    /// Numeric install estimate for sorting
    pub installs: u32,
    /// URL to the repo page on skills.sh
    pub url: String,
    /// Skills in this repo (populated from SSR payload or page scraping)
    pub skills: Vec<PublisherRepoSkill>,
}

/// Fetch skills for a specific repo by scraping `skills.sh/<publisher>/<repo>`.
///
/// Used as fallback when the SSR official payload didn't include skill data.
pub async fn get_publisher_repo_skills(
    publisher_name: &str,
    repo_name: &str,
) -> Result<Vec<PublisherRepoSkill>> {
    let publisher_lower = publisher_name.to_lowercase();
    let repo_lower = repo_name.to_lowercase();
    let url = format!("https://skills.sh/{}/{}", publisher_lower, repo_lower);

    let client = marketplace_client()?;
    let html = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to fetch repo page")?
        .text()
        .await
        .context("Failed to read repo page HTML")?;

    let skills = parse_repo_skills_html(&html, &publisher_lower, &repo_lower);
    debug!(
        target: "skills_sh",
        count = skills.len(),
        publisher = %publisher_lower,
        repo = %repo_lower,
        "parsed repo skills"
    );
    Ok(skills)
}

/// Parse skills from a repo detail page HTML.
///
/// The page lists skills as links like:
/// `href="/publisher/repo/skill-name"` with install counts nearby.
fn parse_repo_skills_html(html: &str, publisher: &str, repo: &str) -> Vec<PublisherRepoSkill> {
    let normalized = html.replace('\n', "");
    let mut skills = Vec::new();

    // Try SSR JSON payload first: look for skill entries in Next.js data
    // Pattern: {"name":"skill-name","installs":12345,...,"source":"publisher/repo"}
    let source_match = format!("{}/{}", publisher, repo);

    // Strategy 1: Parse from SSR JSON — look for skillId entries
    static RE_REPO_SKILL_JSON: OnceLock<Regex> = OnceLock::new();
    let re_json = RE_REPO_SKILL_JSON.get_or_init(|| {
        Regex::new(r#"\{[^{}]*"skillId"\s*:\s*"[^"]+?"[^{}]*"installs"\s*:\s*\d+[^{}]*\}"#)
            .expect("repo skill json regex")
    });

    #[derive(Deserialize)]
    struct SkillJsonEntry {
        name: String,
        installs: u32,
        #[serde(default)]
        source: String,
    }

    for cap in re_json.find_iter(&normalized) {
        let json_str = cap.as_str();
        // Try direct parse
        if let Some(entry) = serde_json::from_str::<SkillJsonEntry>(json_str)
            .ok()
            .filter(|entry| entry.source.to_lowercase() == source_match)
        {
            skills.push(PublisherRepoSkill {
                name: entry.name,
                installs: entry.installs,
            });
            continue;
        }
        // Try unescaped
        let unescaped = json_str.replace("\\\"", "\"").replace("\\/", "/");
        if let Some(entry) = serde_json::from_str::<SkillJsonEntry>(&unescaped)
            .ok()
            .filter(|entry| entry.source.to_lowercase() == source_match)
        {
            skills.push(PublisherRepoSkill {
                name: entry.name,
                installs: entry.installs,
            });
        }
    }

    if !skills.is_empty() {
        // Deduplicate and sort
        let mut seen = HashSet::new();
        skills.retain(|s| seen.insert(s.name.clone()));
        skills.sort_by_key(|s| std::cmp::Reverse(s.installs));
        return skills;
    }

    // Strategy 2: Parse from HTML links — href="/publisher/repo/skill-name"
    let href_pattern = format!(
        r#"href="/{}/{}/([a-zA-Z0-9_.-]+)""#,
        regex::escape(publisher),
        regex::escape(repo)
    );
    let re_href = match Regex::new(&href_pattern) {
        Ok(r) => r,
        Err(_) => return skills,
    };

    // Look for install counts near the skill links
    static RE_INSTALL_NUM: OnceLock<Regex> = OnceLock::new();
    let re_installs = RE_INSTALL_NUM.get_or_init(|| {
        Regex::new(r#"(\d[\d,.]*)\s*([KMkm])?\s*(?:install|$)"#).expect("install num regex")
    });

    let mut seen = HashSet::new();
    for href_cap in re_href.captures_iter(&normalized) {
        let skill_name = href_cap
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        if skill_name.is_empty() || !seen.insert(skill_name.clone()) {
            continue;
        }

        // Look for install count near this link
        let match_start = href_cap.get(0).unwrap().start();
        let context_start = match_start.saturating_sub(50);
        let context_end = (match_start + 500).min(normalized.len());
        let context = &normalized[context_start..context_end];

        let installs = re_installs
            .captures(context)
            .map(|c| {
                let num_str = c.get(1).map(|m| m.as_str()).unwrap_or("0");
                let num: f64 = num_str.replace(',', "").parse().unwrap_or(0.0);
                let suffix = c.get(2).map(|m| m.as_str()).unwrap_or("");
                match suffix {
                    "K" | "k" => (num * 1_000.0) as u32,
                    "M" | "m" => (num * 1_000_000.0) as u32,
                    _ => num as u32,
                }
            })
            .unwrap_or(0);

        skills.push(PublisherRepoSkill {
            name: skill_name,
            installs,
        });
    }

    skills.sort_by_key(|s| std::cmp::Reverse(s.installs));
    skills
}

/// Fetch all repos for a publisher.
///
/// Strategy:
/// 1. Try `skills.sh/official` — the SSR payload contains every repo for every publisher
///    (the per-publisher page may omit low-traffic repos).
/// 2. Fall back to `skills.sh/<publisher>` HTML scraping if the official payload fails.
pub async fn get_publisher_repos(publisher_name: &str) -> Result<Vec<PublisherRepo>> {
    let client = marketplace_client()?;
    let publisher_lower = publisher_name.to_lowercase();

    // Strategy 1: official page SSR payload (complete data)
    if let Ok(html) = match client
        .get("https://skills.sh/official")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
    {
        Ok(official_html) => official_html.text().await,
        Err(err) => Err(err),
    } {
        let repos = parse_publisher_repos_from_official_payload(&html, &publisher_lower);
        if !repos.is_empty() {
            debug!(
                target: "skills_sh",
                count = repos.len(),
                publisher = %publisher_name,
                "parsed repos from official payload"
            );
            return Ok(repos);
        }
    }

    // Strategy 2: fall back to per-publisher page
    let url = format!("https://skills.sh/{}", publisher_lower);
    let html = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to fetch publisher page")?
        .text()
        .await
        .context("Failed to read publisher page HTML")?;

    let repos = parse_publisher_repos_html(&html, publisher_name);
    debug!(
        target: "skills_sh",
        count = repos.len(),
        publisher = %publisher_name,
        "parsed repos from detail page"
    );

    Ok(repos)
}

/// Parse repos for a specific publisher from the `skills.sh/official` SSR JSON payload.
///
/// The payload contains entries like:
/// `"owner":"github","repos":[{"repo":"github/awesome-copilot","totalInstalls":N,"skills":[...]}]`
pub(crate) fn parse_publisher_repos_from_official_payload(
    html: &str,
    publisher_lower: &str,
) -> Vec<PublisherRepo> {
    // The SSR payload uses backslash-escaped quotes: \"owner\":\"github\",\"repos\":[...]
    // Try escaped form first, then unescaped form as fallback.
    let escaped_needle = format!(r#"\"owner\":\"{}\"#, publisher_lower);
    let unescaped_needle = format!(r#""owner":"{}""#, publisher_lower);

    let owner_pos = html
        .find(&escaped_needle)
        .or_else(|| html.find(&unescaped_needle));
    let Some(owner_pos) = owner_pos else {
        return Vec::new();
    };

    // Find the start of "repos":[ after the owner match
    let after_owner = &html[owner_pos..];
    let repos_key_escaped = r#"\"repos\":["#;
    let repos_key_plain = r#""repos":["#;

    let repos_offset = after_owner
        .find(repos_key_escaped)
        .map(|p| p + repos_key_escaped.len() - 1) // point at '['
        .or_else(|| {
            after_owner
                .find(repos_key_plain)
                .map(|p| p + repos_key_plain.len() - 1)
        });

    let Some(repos_offset) = repos_offset else {
        return Vec::new();
    };

    let repos_array_start = owner_pos + repos_offset;

    // Find matching ']' — simple bracket counting
    let slice = &html[repos_array_start..];
    let mut depth = 0i32;
    let mut end_pos = 0;
    for (i, ch) in slice.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if end_pos == 0 {
        return Vec::new();
    }

    let repos_json = &slice[..end_pos];

    // The JSON may be escaped in the SSR payload — unescape backslash-escaped quotes
    let unescaped = repos_json.replace("\\\"", "\"").replace("\\/", "/");

    // Parse as array of repo objects
    #[derive(Deserialize)]
    struct RepoEntry {
        repo: String,
        #[serde(rename = "totalInstalls")]
        total_installs: u32,
        skills: Vec<SkillEntry>,
    }
    #[derive(Deserialize)]
    struct SkillEntry {
        #[allow(dead_code)]
        name: String,
        #[allow(dead_code)]
        installs: u32,
    }

    let entries: Vec<RepoEntry> = match serde_json::from_str(&unescaped) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                target: "skills_sh",
                publisher = %publisher_lower,
                error = %e,
                "failed to parse official payload repos"
            );
            return Vec::new();
        }
    };

    let mut repos: Vec<PublisherRepo> = entries
        .into_iter()
        .map(|e| {
            // e.repo is like "github/awesome-copilot"
            let repo_name = e.repo.split('/').next_back().unwrap_or(&e.repo).to_string();
            let installs_label = format_installs_label(e.total_installs);

            let skills: Vec<PublisherRepoSkill> = e
                .skills
                .into_iter()
                .map(|s| PublisherRepoSkill {
                    name: s.name,
                    installs: s.installs,
                })
                .collect();

            PublisherRepo {
                source: e.repo.clone(),
                url: format!("https://skills.sh/{}", e.repo),
                skill_count: skills.len() as u32,
                installs_label,
                installs: e.total_installs,
                repo: repo_name,
                skills,
            }
        })
        .collect();

    repos.sort_by_key(|s| std::cmp::Reverse(s.installs));
    repos
}

/// Format numeric installs into human-readable labels (e.g. 2424777 → "2.4M")
fn format_installs_label(installs_count: u32) -> String {
    if installs_count >= 1_000_000 {
        format!("{:.1}M", installs_count as f64 / 1_000_000.0)
    } else if installs_count >= 1_000 {
        format!("{:.1}K", installs_count as f64 / 1_000.0)
    } else {
        installs_count.to_string()
    }
}

fn parse_publisher_repos_html(html: &str, publisher_name: &str) -> Vec<PublisherRepo> {
    let normalized = html.replace('\n', "");
    let mut repos = Vec::new();
    let publisher_lower = publisher_name.to_lowercase();

    // Pattern: href="/publisher/repo-name">...<h3>repo-name</h3>...N skills:...installs</a>
    // We look for each href="/publisher/X" link and extract repo name, skill count, installs
    let href_pattern = format!(
        r#"href="/{}/([a-z0-9A-Z_.-]+)""#,
        regex::escape(&publisher_lower)
    );
    let re_href = match Regex::new(&href_pattern) {
        Ok(r) => r,
        Err(_) => return repos,
    };

    // For skill count: "N<!-- -->...skills" or just "N skills"
    static RE_SKILL_COUNT: OnceLock<Regex> = OnceLock::new();
    let re_skill_count = RE_SKILL_COUNT.get_or_init(|| {
        Regex::new(r#"(\d+)(?:\s*(?:<!--[^>]*-->)?\s*)*skill"#).expect("skill count regex")
    });

    // For installs: font-mono text-sm text-foreground">VALUE</span>
    static RE_INSTALLS: OnceLock<Regex> = OnceLock::new();
    let re_installs = RE_INSTALLS.get_or_init(|| {
        Regex::new(r#"font-mono text-sm text-foreground">([^<]+)</span>"#).expect("installs regex")
    });

    let mut seen = std::collections::HashSet::new();

    for href_cap in re_href.captures_iter(&normalized) {
        let repo_name = href_cap
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        if repo_name.is_empty() || !seen.insert(repo_name.clone()) {
            continue;
        }

        // Find the surrounding context for this repo link
        let match_start = href_cap.get(0).unwrap().start();
        let context_end = (match_start + 1000).min(normalized.len());
        let context = &normalized[match_start..context_end];

        // Extract skill count
        let skill_count = re_skill_count
            .captures(context)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);

        // Extract installs label
        let installs_label = re_installs
            .captures(context)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();

        let installs = parse_installs_label(&installs_label);

        repos.push(PublisherRepo {
            source: format!("{}/{}", publisher_lower, repo_name),
            url: format!("https://skills.sh/{}/{}", publisher_lower, repo_name),
            repo: repo_name.clone(),
            skill_count,
            installs_label,
            installs,
            skills: Vec::new(), // HTML fallback doesn't have per-skill data
        });
    }

    // Sort by installs descending
    repos.sort_by_key(|s| std::cmp::Reverse(s.installs));

    repos
}

/// Parse install labels like "2.9M", "12.8K", "85" into numeric values
fn parse_installs_label(label: &str) -> u32 {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return 0;
    }

    if let Some(num_str) = trimmed.strip_suffix('M') {
        num_str
            .parse::<f64>()
            .map(|n| (n * 1_000_000.0) as u32)
            .unwrap_or(0)
    } else if let Some(num_str) = trimmed.strip_suffix('K') {
        num_str
            .parse::<f64>()
            .map(|n| (n * 1_000.0) as u32)
            .unwrap_or(0)
    } else {
        trimmed.replace(',', "").parse::<u32>().unwrap_or(0)
    }
}
