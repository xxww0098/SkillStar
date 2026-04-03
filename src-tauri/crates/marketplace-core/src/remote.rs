use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, OnceLock};

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{OfficialPublisher, Skill, SkillCategory, SkillType};

/// Build-time User-Agent string derived from Cargo.toml version.
const USER_AGENT: &str = concat!("SkillStar/", env!("CARGO_PKG_VERSION"));

/// Shared HTTP client for all marketplace requests.
///
/// Reuses TLS sessions and connection pools across calls, avoiding
/// the ~50-100ms overhead of a fresh TLS handshake per request.
static MARKETPLACE_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to build marketplace HTTP client")
});

fn marketplace_client() -> &'static reqwest::Client {
    &MARKETPLACE_CLIENT
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceResult {
    pub skills: Vec<Skill>,
    pub total_count: u32,
    pub page: u32,
    pub has_more: bool,
}

// ── skills.sh Integration ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SkillsShSearchResponse {
    skills: Vec<SkillsShSkill>,
}

#[derive(Debug, Deserialize)]
struct SkillsShSkill {
    #[serde(rename = "skillId")]
    _skill_id: String,
    name: String,
    description: Option<String>,
    #[serde(rename = "source")]
    source: String,
    installs: u32,
    #[serde(rename = "repoUrl")]
    repo_url: Option<String>,
}

impl From<SkillsShSkill> for Skill {
    fn from(skill_entry: SkillsShSkill) -> Self {
        let git_url = skill_entry.repo_url.unwrap_or_else(|| {
            // source is "org/repo" (e.g. "vercel/ai"), the actual GitHub repo
            format!("https://github.com/{}", skill_entry.source)
        });
        let source = Some(skill_entry.source.clone());
        Skill {
            name: skill_entry.name,
            description: skill_entry.description.unwrap_or_default(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: skill_entry.installs,
            installed: false,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url,
            tree_hash: None,
            category: SkillCategory::None,
            author: Some(skill_entry.source),
            topics: vec![],
            agent_links: Some(Vec::new()),
            rank: None,
            source,
        }
    }
}

/// Search skills.sh registry via official API
/// API endpoint: GET https://skills.sh/api/search?q={query}&limit={limit}
/// Note: max limit is ~100 (API returns 400 for higher values)
/// Note: empty query returns 400 — a query is always required
pub async fn search_skills_sh(query: &str, limit: u32) -> Result<MarketplaceResult> {
    let client = marketplace_client();
    // Clamp limit to API maximum
    let clamped_limit = limit.min(100);
    let url = format!(
        "https://skills.sh/api/search?q={}&limit={}",
        url_encode_query_component(query),
        clamped_limit
    );

    let response: SkillsShSearchResponse = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to call skills.sh search API")?
        .json()
        .await
        .context("Failed to parse skills.sh response")?;

    let mut skills: Vec<Skill> = response.skills.into_iter().map(Skill::from).collect();
    // Sort by installs descending and assign ranks
    skills.sort_by(|a, b| b.stars.cmp(&a.stars));
    for (i, skill) in skills.iter_mut().enumerate() {
        skill.rank = Some((i + 1) as u32);
    }
    let total_count = skills.len() as u32;

    Ok(MarketplaceResult {
        skills,
        total_count,
        page: 1,
        has_more: false,
    })
}

// ── Cached Regexes (compiled once, reused forever) ─────────────────

fn re_strip_html() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<[^>]+>"#).expect("strip html regex"))
}

fn re_leaderboard_json_object() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\{[^{}]*"skillId"\s*:\s*"[^"]+"[^{}]*\}"#).expect("leaderboard json regex")
    })
}

fn re_leaderboard_escaped() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\\?"skillId\\?"\s*:\s*\\?"([^\\]+)\\?\\?"#)
            .expect("leaderboard escaped regex")
    })
}

fn re_nextjs_skill_data() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#""skillId":"([^"]+)","name":"([^"]+)","installs":(\d+)"#)
            .expect("nextjs skill data regex")
    })
}

/// Get skills.sh leaderboard via HTML scraping
pub async fn get_skills_sh_leaderboard(category: &str) -> Result<Vec<Skill>> {
    let client = marketplace_client();

    // Map category to URL path
    let url_path = match category {
        "hot" => "/hot",
        "popular" | "all" => "/",
        "trending" => "/trending",
        _ => "/",
    };

    let url = format!("https://skills.sh{}", url_path);
    debug!(target: "skills_sh", url = %url, "fetching leaderboard");

    let html = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to fetch skills.sh")?
        .text()
        .await
        .context("Failed to read HTML")?;

    let skills = parse_skills_sh_html(&html);
    debug!(target: "skills_sh", count = skills.len(), "parsed skills from HTML");

    // If HTML parsing fails, fallback to search API
    if skills.is_empty() {
        warn!(target: "skills_sh", "HTML parsing failed, using search API fallback");
        let fallback_url = "https://skills.sh/api/search?q=ai&limit=200";
        let response: SkillsShSearchResponse = client
            .get(fallback_url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .context("Fallback failed")?
            .json()
            .await
            .context("Fallback parse failed")?;
        return Ok(response.skills.into_iter().map(Skill::from).collect());
    }

    Ok(skills)
}

/// Extract skills from the escaped Next.js SSR payload.
///
/// The skills.sh homepage embeds skill data as backslash-escaped JSON inside
/// `<script>` tags. Each object looks like:
///   `{\"source\":\"vercel-labs/skills\",\"skillId\":\"find-skills\",\"name\":\"find-skills\",\"installs\":787461}`
///
/// Standard regex with `[^{}]` and unescaped `"` delimiters fails to match
/// these objects. This function uses a regex targeting escaped quotes and then
/// unescapes each match before serde parsing.
fn extract_skills_from_escaped_payload(html: &str) -> Vec<Skill> {
    fn re_escaped_skill_object() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            // Match flat JSON objects containing \"skillId\" (escaped quotes).
            // [^}]* is safe because these skill objects never have nested braces.
            Regex::new(r#"\{[^}]*\\"skillId\\"[^}]*\}"#).expect("escaped skill object regex")
        })
    }

    let re = re_escaped_skill_object();
    let mut skills = Vec::new();

    for cap in re.find_iter(html) {
        let raw = cap.as_str();
        // The SSR payload uses double-backslash escaping: \\"field\\"
        // Two passes of \" → " fully unescape to valid JSON.
        let unescaped = raw
            .replace("\\\"", "\"")
            .replace("\\\"", "\"")
            .replace("\\/", "/")
            .replace("\\\\", "\\");

        if let Ok(entry) = serde_json::from_str::<SkillsShSkill>(&unescaped) {
            let source = entry.source.clone();
            let repo_url = entry
                .repo_url
                .unwrap_or_else(|| format!("https://github.com/{}", source));
            let description = entry.description.unwrap_or_default();
            skills.push(Skill::from_skills_sh(
                entry.name,
                description,
                entry.installs,
                source,
                repo_url,
            ));
        }
    }

    skills
}

fn parse_skills_sh_html(html: &str) -> Vec<Skill> {
    // ── Strategy 0: Escaped SSR payload (current skills.sh format) ──────
    // Primary path. The Next.js SSR payload embeds skill data as
    // backslash-escaped JSON objects. Extract, unescape, parse.
    let mut skills = extract_skills_from_escaped_payload(html);
    if !skills.is_empty() {
        debug!(
            target: "skills_sh",
            count = skills.len(),
            "Strategy 0 (escaped SSR) matched"
        );
        let mut seen = std::collections::HashSet::new();
        skills.retain(|s| seen.insert(s.name.clone()));
        for (i, skill) in skills.iter_mut().enumerate() {
            skill.rank = Some((i + 1) as u32);
        }
        return skills;
    }

    // ── Strategy 1 (legacy fallback): unescaped JSON / HTML patterns ────

    // Pattern 1: Find JSON objects containing skillId and installs
    let cached_regexes: [&Regex; 2] = [re_leaderboard_json_object(), re_leaderboard_escaped()];

    for re in &cached_regexes {
        for cap in re.find_iter(html) {
            let json_str = cap.as_str();

            // Try direct parse
            if let Ok(s) = serde_json::from_str::<SkillsShSkill>(json_str) {
                let source = s.source.clone();
                let repo_url = s
                    .repo_url
                    .unwrap_or_else(|| format!("https://github.com/{}", source));
                let description = s.description.unwrap_or_default();

                let skill =
                    Skill::from_skills_sh(s.name, description, s.installs, source, repo_url);
                skills.push(skill);
                continue;
            }

            // Try unescaping
            let unescaped = json_str
                .replace("\\\"", "\"")
                .replace("\\\\/", "/")
                .replace("\\\\", "\\");

            if let Ok(s) = serde_json::from_str::<SkillsShSkill>(&unescaped) {
                let source = s.source.clone();
                let repo_url = s
                    .repo_url
                    .unwrap_or_else(|| format!("https://github.com/{}", source));
                let description = s.description.unwrap_or_default();

                let skill =
                    Skill::from_skills_sh(s.name, description, s.installs, source, repo_url);
                skills.push(skill);
            }
        }
    }

    // Pattern 2: Extract from Next.js server component data
    if skills.is_empty() {
        let re2 = re_nextjs_skill_data();
        for cap in re2.captures_iter(html) {
            let skill_id = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or(skill_id);
            let installs: u32 = cap
                .get(3)
                .map(|m| m.as_str())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);

            if installs > 0 {
                let source = extract_source_from_html(html, name);
                let description = extract_description_from_html(html, name);

                let git_url = format!("https://github.com/{}", source);
                let skill =
                    Skill::from_skills_sh(name.to_string(), description, installs, source, git_url);
                skills.push(skill);
            }
        }
    }

    // Deduplicate while preserving the page order from skills.sh.
    let mut seen = std::collections::HashSet::new();
    skills.retain(|s| seen.insert(s.name.clone()));

    // Assign ranks using the original leaderboard order for the current page.
    for (i, skill) in skills.iter_mut().enumerate() {
        skill.rank = Some((i + 1) as u32);
    }

    skills
}

fn extract_source_from_html(html: &str, skill_name: &str) -> String {
    // Build a targeted search string to avoid regex compilation per call.
    // Look for `"<name>","source":"<value>"` pattern using byte search.
    let needle = format!(r#""{}""#, skill_name);
    if let Some(pos) = html.find(&needle) {
        let after = &html[pos + needle.len()..];
        // Expect: ,"source":"..."
        if let Some(src_start) = after.find(r#""source":""#) {
            let value_start = src_start + r#""source":""#.len();
            if let Some(value_end) = after[value_start..].find('"') {
                return after[value_start..value_start + value_end].to_string();
            }
        }
    }
    "anthropics/skills".to_string()
}

fn extract_description_from_html(_html: &str, skill_name: &str) -> String {
    format!("Skill: {}", skill_name)
}

// ── Official Publishers ────────────────────────────────────────────────

/// Get official publishers from skills.sh/official via HTML scraping
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>> {
    let client = marketplace_client();

    let html = client
        .get("https://skills.sh/official")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to fetch skills.sh/official")?
        .text()
        .await
        .context("Failed to read HTML")?;

    let publishers = parse_official_publishers_html(&html);
    debug!(
        target: "skills_sh",
        count = publishers.len(),
        "parsed official publishers"
    );

    Ok(publishers)
}

fn parse_official_publishers_html(html: &str) -> Vec<OfficialPublisher> {
    let mut publishers = Vec::new();
    let normalized = html.replace('\n', "");
    let mut seen = std::collections::HashSet::new();

    fn push_publisher(
        publishers: &mut Vec<OfficialPublisher>,
        seen: &mut std::collections::HashSet<String>,
        name: &str,
        repo: &str,
        repo_count: u32,
        skill_count: u32,
    ) {
        if name.is_empty() || matches!(name, "official" | "audits" | "docs") {
            return;
        }
        if seen.insert(name.to_string()) {
            publishers.push(OfficialPublisher {
                name: name.to_string(),
                repo: repo.to_string(),
                repo_count,
                skill_count,
                url: format!("https://skills.sh/{}", name),
            });
        }
    }

    fn local_re_official_row() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(
                r#"href="/([a-z0-9_-]+)".*?<span[^>]*class="font-mono[^"]*"[^>]*>([^<]+)</span></div><div[^>]*>(\d+)</div><div[^>]*>(\d+)</div></a>"#,
            )
            .expect("official row regex")
        })
    }

    fn local_re_legacy_publisher() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r#""name":"([^"]+)","repo":"([^"]+)","repoCount":(\d+),"skillCount":(\d+)""#)
                .expect("legacy publisher regex")
        })
    }

    fn local_re_older_publisher() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r#""name":"([^"]+)","repo":"([^"]+)","skillCount":(\d+),"installs":(\d+)""#)
                .expect("older publisher regex")
        })
    }

    fn local_re_old_html_publisher() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r#"href="/([a-z0-9_-]+)"[^>]*>.*?(\d+)\s*skill.*?(\d+)\s*install"#)
                .expect("old html publisher regex")
        })
    }

    // Pattern 1 (current page) — cached regex
    for cap in local_re_official_row().captures_iter(&normalized) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let repo = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let repo_count = cap
            .get(3)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let skill_count = cap
            .get(4)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);

        push_publisher(
            &mut publishers,
            &mut seen,
            name,
            repo,
            repo_count,
            skill_count,
        );
    }

    // Pattern 2 (legacy SSR payload) — cached regex
    if publishers.is_empty() {
        for cap in local_re_legacy_publisher().captures_iter(&normalized) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let repo = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let repo_count = cap
                .get(3)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let skill_count = cap
                .get(4)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            push_publisher(
                &mut publishers,
                &mut seen,
                name,
                repo,
                repo_count,
                skill_count,
            );
        }
    }

    // Pattern 3 (older payload) — cached regex
    if publishers.is_empty() {
        for cap in local_re_older_publisher().captures_iter(&normalized) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let repo = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let skill_count = cap
                .get(3)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            push_publisher(&mut publishers, &mut seen, name, repo, 0, skill_count);
        }
    }

    // Pattern 4: very old HTML — cached regex
    if publishers.is_empty() {
        for cap in local_re_old_html_publisher().captures_iter(&normalized) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let skill_count = cap
                .get(2)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            push_publisher(&mut publishers, &mut seen, name, "skills", 0, skill_count);
        }
    }

    // Hardcoded fallback: known major publishers for reliability
    if publishers.is_empty() {
        warn!(target: "skills_sh", "HTML parsing failed, using known publishers");
        publishers = known_official_publishers();
    }

    // Sort by skill count descending, then repo count.
    publishers.sort_by(|a, b| {
        b.skill_count
            .cmp(&a.skill_count)
            .then_with(|| b.repo_count.cmp(&a.repo_count))
    });

    publishers
}

/// Known official publishers as fallback data
fn known_official_publishers() -> Vec<OfficialPublisher> {
    let fallback_publishers = vec![
        ("vercel-labs", "agent-skills", 3, 2195),
        ("microsoft", "github-copilot-for-azure", 23, 630),
        ("anthropics", "skills", 11, 256),
        ("google-labs-code", "stitch-skills", 3, 16),
        ("github", "awesome-copilot", 5, 277),
        ("cloudflare", "skills", 8, 50),
        ("expo", "skills", 2, 17),
        ("firebase", "agent-skills", 4, 35),
        ("openai", "skills", 6, 82),
        ("supabase", "agent-skills", 2, 8),
        ("langchain-ai", "langchain-skills", 6, 78),
        ("hashicorp", "agent-skills", 4, 47),
        ("stripe", "ai", 4, 7),
        ("posthog", "posthog", 5, 28),
        ("prisma", "skills", 2, 36),
        ("figma", "mcp-server-guide", 1, 10),
        ("firecrawl", "cli", 8, 68),
        ("flutter", "skills", 3, 49),
        ("vercel", "ai", 2, 3163),
        ("shadcn", "ui", 1, 0),
        ("google-gemini", "gemini-skills", 3, 19),
        ("huggingface", "skills", 3, 27),
        ("remotion-dev", "skills", 2, 9),
        ("tavily-ai", "skills", 2, 19),
        ("browser-use", "browser-use", 1, 4),
        ("facebook", "react", 2, 11),
        ("better-auth", "skills", 2, 11),
        ("resend", "resend-skills", 6, 10),
        ("sentry", "skills", 13, 207),
        ("neondatabase", "agent-skills", 6, 21),
        ("dagster-io", "erk", 4, 53),
        ("datadog-labs", "agent-skills", 2, 16),
        ("bitwarden", "ai-plugins", 3, 32),
        ("upstash", "context7", 9, 22),
        ("sanity-io", "agent-toolkit", 4, 18),
        ("pulumi", "agent-skills", 2, 28),
        ("mapbox", "mapbox-agent-skills", 2, 22),
        ("semgrep", "skills", 3, 6),
        ("clerk", "skills", 1, 17),
    ];

    fallback_publishers
        .into_iter()
        .map(|(name, repo, repo_count, skill_count)| OfficialPublisher {
            name: name.to_string(),
            repo: repo.to_string(),
            repo_count,
            skill_count,
            url: format!("https://skills.sh/{}", name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_official_publishers_html;

    #[test]
    fn parses_current_official_row_repo_and_skill_counts() {
        let html = r#"<a class="group grid grid-cols-[1fr_4rem_4rem]" href="/anthropics"><div class="min-w-0 flex items-center gap-3"><span class="font-semibold text-foreground">anthropics</span><span class="font-mono text-sm text-(--ds-gray-600)">skills</span></div><div class="text-right font-mono text-sm text-(--ds-gray-600)">11</div><div class="text-right font-mono text-sm text-(--ds-gray-600)">256</div></a>"#;
        let publishers = parse_official_publishers_html(html);
        assert_eq!(publishers.len(), 1);
        assert_eq!(publishers[0].name, "anthropics");
        assert_eq!(publishers[0].repo, "skills");
        assert_eq!(publishers[0].repo_count, 11);
        assert_eq!(publishers[0].skill_count, 256);
    }

    #[test]
    fn parses_publisher_repos_from_official_ssr_payload() {
        use super::parse_publisher_repos_from_official_payload;

        // Simulate the SSR payload with backslash-escaped quotes (as seen in real HTML)
        let html = r#"some prefix{\"owner\":\"github\",\"repos\":[{\"repo\":\"github/awesome-copilot\",\"totalInstalls\":2424777,\"skills\":[{\"name\":\"git-commit\",\"installs\":22757}]},{\"repo\":\"github/gh-aw\",\"totalInstalls\":100,\"skills\":[{\"name\":\"developer\",\"installs\":50},{\"name\":\"console\",\"installs\":50}]},{\"repo\":\"github/copilot-plugins\",\"totalInstalls\":30,\"skills\":[{\"name\":\"spark\",\"installs\":30}]},{\"repo\":\"github/gh-aw-firewall\",\"totalInstalls\":3,\"skills\":[{\"name\":\"awf-skill\",\"installs\":3}]},{\"repo\":\"github/synapsync\",\"totalInstalls\":2,\"skills\":[{\"name\":\"code-analyzer\",\"installs\":2}]}],\"totalInstalls\":2424881}some suffix"#;

        let repos = parse_publisher_repos_from_official_payload(html, "github");
        assert_eq!(
            repos.len(),
            5,
            "Should find all 5 repos including low-traffic ones"
        );
        assert_eq!(repos[0].repo, "awesome-copilot");
        assert_eq!(repos[0].skill_count, 1); // 1 skill in test data
        assert_eq!(repos[0].installs, 2424777);
        assert_eq!(repos[4].repo, "synapsync");
        assert_eq!(repos[4].installs, 2);
    }
}

fn url_encode_query_component(raw_query: &str) -> String {
    raw_query
        .replace(' ', "+")
        .replace(':', "%3A")
        .replace('>', "%3E")
        .replace('<', "%3C")
}

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

    let client = marketplace_client();
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
        if let Ok(entry) = serde_json::from_str::<SkillJsonEntry>(json_str) {
            if entry.source.to_lowercase() == source_match {
                skills.push(PublisherRepoSkill {
                    name: entry.name,
                    installs: entry.installs,
                });
                continue;
            }
        }
        // Try unescaped
        let unescaped = json_str.replace("\\\"", "\"").replace("\\/", "/");
        if let Ok(entry) = serde_json::from_str::<SkillJsonEntry>(&unescaped) {
            if entry.source.to_lowercase() == source_match {
                skills.push(PublisherRepoSkill {
                    name: entry.name,
                    installs: entry.installs,
                });
            }
        }
    }

    if !skills.is_empty() {
        // Deduplicate and sort
        let mut seen = HashSet::new();
        skills.retain(|s| seen.insert(s.name.clone()));
        skills.sort_by(|a, b| b.installs.cmp(&a.installs));
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

    skills.sort_by(|a, b| b.installs.cmp(&a.installs));
    skills
}

/// Fetch all repos for a publisher.
///
/// Strategy:
/// 1. Try `skills.sh/official` — the SSR payload contains every repo for every publisher
///    (the per-publisher page may omit low-traffic repos).
/// 2. Fall back to `skills.sh/<publisher>` HTML scraping if the official payload fails.
pub async fn get_publisher_repos(publisher_name: &str) -> Result<Vec<PublisherRepo>> {
    let client = marketplace_client();
    let publisher_lower = publisher_name.to_lowercase();

    // Strategy 1: official page SSR payload (complete data)
    if let Ok(official_html) = client
        .get("https://skills.sh/official")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
    {
        if let Ok(html) = official_html.text().await {
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
fn parse_publisher_repos_from_official_payload(
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
            let repo_name = e.repo.split('/').last().unwrap_or(&e.repo).to_string();
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

    repos.sort_by(|a, b| b.installs.cmp(&a.installs));
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
    repos.sort_by(|a, b| b.installs.cmp(&a.installs));

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

// ── Skill Detail Page Fetching ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAudit {
    pub name: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSkillDetails {
    /// Full Summary in Markdown (converted from HTML)
    pub summary: Option<String>,
    /// Full SKILL.md content in Markdown (converted from HTML)
    pub readme: Option<String>,
    /// Weekly installs label (e.g. "103.7K")
    pub weekly_installs: Option<String>,
    /// GitHub stars count
    pub github_stars: Option<u32>,
    /// First seen date string (e.g. "Feb 4, 2026")
    pub first_seen: Option<String>,
    /// Security audit results
    pub security_audits: Vec<SecurityAudit>,
}

/// Fetch rich detail data for a single skill from its skills.sh page.
///
/// URL format: `https://skills.sh/{source}/{name}`
/// where source = "org/repo" and name = skill name.
///
/// Returns `None`-filled fields on partial failure; the caller should
/// gracefully fall back to the original truncated description.
pub async fn fetch_marketplace_skill_details(
    source: &str,
    name: &str,
) -> Result<MarketplaceSkillDetails> {
    let url = format!("https://skills.sh/{}/{}", source, name);
    debug!(target: "skills_sh", url = %url, "fetching skill details");

    let client = marketplace_client();

    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        )
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to fetch skill detail page")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("skills.sh returned HTTP {}", status.as_u16());
    }

    let html = response
        .text()
        .await
        .context("Failed to read response body")?;

    // Check for Next.js error page
    if html.contains("__next_error__") {
        anyhow::bail!("Skill page not found (Next.js error page)");
    }

    Ok(parse_skill_detail_html(&html))
}

fn parse_skill_detail_html(html: &str) -> MarketplaceSkillDetails {
    // ── Summary ────────────────────────────────────────────────────
    let summary = extract_prose_block(html, ">Summary</div>")
        .map(|inner| html2md::parse_html(&inner))
        .filter(|s| !s.trim().is_empty());

    // ── SKILL.md ───────────────────────────────────────────────────
    let readme = extract_prose_block(html, "SKILL.md</span>")
        .map(|inner| html2md::parse_html(&inner))
        .filter(|s| !s.trim().is_empty());

    // ── Weekly Installs ────────────────────────────────────────────
    let weekly_installs = extract_text_after_label(html, "Weekly Installs");

    // ── GitHub Stars ───────────────────────────────────────────────
    let github_stars = extract_text_after_label(html, "GitHub Stars")
        .and_then(|s| s.replace(',', "").parse::<u32>().ok());

    // ── First Seen ─────────────────────────────────────────────────
    let first_seen = extract_text_after_label(html, "First Seen");

    // ── Security Audits ────────────────────────────────────────────
    let security_audits = extract_security_audits(html);

    MarketplaceSkillDetails {
        summary,
        readme,
        weekly_installs,
        github_stars,
        first_seen,
        security_audits,
    }
}

/// Extract the inner HTML of the first `<div class="prose ...">` that
/// appears after the given `keyword` anchor in the HTML string.
///
/// Uses a simple depth-tracking `<div`/`</div>` scanner instead of a
/// full DOM parser to keep dependencies minimal.
fn extract_prose_block(html: &str, keyword: &str) -> Option<String> {
    let kw_pos = html.find(keyword)?;
    let after_kw = &html[kw_pos..];

    // Find the prose container div
    let prose_offset = after_kw.find("<div class=\"prose")?;
    let prose_start = kw_pos + prose_offset;

    // Find the end of the opening tag
    let tag_end = html[prose_start..].find('>')? + prose_start + 1;

    // Walk through the HTML tracking div depth
    let mut depth: u32 = 1;
    let mut cursor = tag_end;

    while depth > 0 && cursor < html.len() {
        let next_open = html[cursor..].find("<div");
        let next_close = html[cursor..].find("</div>");

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                cursor += o + 4; // skip past "<div"
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return Some(html[tag_end..cursor + c].to_string());
                }
                cursor += c + 6; // skip past "</div>"
            }
            _ => break,
        }
    }

    None
}

/// Extract a single text value that appears after a sidebar label.
///
/// Pattern: `{label}</span>` or `{label}</div>` followed by a container div
/// with the value in a nested text node.
fn extract_text_after_label(html: &str, label: &str) -> Option<String> {
    let label_pos = html.find(label)?;
    let after_label = &html[label_pos..];

    // The value is typically in the next or nearby div/span with text content.
    // Look for the pattern: label_tag_close ... >VALUE</
    // Strategy: find the label-enclosing tag close, then scan forward
    // for the first text content in subsequent tags.

    // For "Weekly Installs": ...Weekly Installs</span></div><div class="...">103.7K</div>
    // For "First Seen":      ...First Seen</span></div><div class="...">Feb 4, 2026</div>
    // For "GitHub Stars":    ...GitHub Stars</span></div><div class="..."><svg...><span>167</span></div>

    // Find closing tags after label, then the next meaningful text
    let search_window = &after_label[..after_label.len().min(600)];

    // Skip past the label's own container (closing </div> after label)
    let first_close = search_window.find("</div>")?;
    let after_first_close = &search_window[first_close + 6..];

    // Now find the next opening tag with a class
    let next_div = after_first_close.find("<div")?;
    let after_next_div = &after_first_close[next_div..];

    // Find content between > and </div>
    let content_start = after_next_div.find('>')? + 1;
    let content_end = after_next_div.find("</div>")?;

    if content_start >= content_end {
        return None;
    }

    let raw_content = &after_next_div[content_start..content_end];

    // Strip HTML tags from the content (there might be SVGs, spans, etc.)
    let text = re_strip_html()
        .replace_all(raw_content, "")
        .trim()
        .to_string();

    if text.is_empty() { None } else { Some(text) }
}

/// Extract security audit results from the page.
fn extract_security_audits(html: &str) -> Vec<SecurityAudit> {
    let mut audits = Vec::new();

    let Some(audits_pos) = html.find("Security Audits") else {
        return audits;
    };

    let search_window = &html[audits_pos..html.len().min(audits_pos + 2000)];

    // Each audit follows this pattern:
    // <span class="...">Audit Name</span><span class="...text-green...">Pass</span>
    // We look for pairs of: name_span followed by result_span
    fn re_audit_entry() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(
                r#"<span class="[^"]*font-medium[^"]*">([^<]+)</span><span class="[^"]*">(Pass|Fail|Partial)[^<]*</span>"#,
            )
            .expect("audit entry regex")
        })
    }

    for cap in re_audit_entry().captures_iter(search_window) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let result = cap
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        if !name.is_empty() {
            audits.push(SecurityAudit { name, result });
        }
    }

    audits
}

// ── AI Marketplace Search ───────────────────────────────────────────

/// Result of AI-powered keyword search, including per-keyword attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiKeywordSearchResult {
    /// Merged, deduplicated skills sorted by installs.
    pub skills: Vec<Skill>,
    pub total_count: u32,
    /// Maps each keyword → list of skill names it found.
    pub keyword_skill_map: HashMap<String, Vec<String>>,
}

/// Search skills.sh concurrently with multiple keywords and merge results.
///
/// Each keyword is searched in parallel (bounded to 4 concurrent requests).
/// Results are deduplicated by skill name, keeping the entry with the highest
/// install count. Returns per-keyword attribution for frontend filtering.
pub async fn ai_search_by_keywords(keywords: &[String]) -> Result<AiKeywordSearchResult> {
    if keywords.is_empty() {
        return Ok(AiKeywordSearchResult {
            skills: Vec::new(),
            total_count: 0,
            keyword_skill_map: HashMap::new(),
        });
    }

    let mut join_set: tokio::task::JoinSet<Result<(String, MarketplaceResult)>> =
        tokio::task::JoinSet::new();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));

    for keyword in keywords.iter().cloned() {
        let permit = semaphore.clone();
        join_set.spawn(async move {
            let _permit = permit
                .acquire_owned()
                .await
                .map_err(|_| anyhow!("search semaphore closed"))?;
            debug!(target: "ai_search", keyword = %keyword, "searching keyword");
            let result = search_skills_sh(&keyword, 50).await?;
            Ok((keyword, result))
        });
    }

    // Merge all results, dedup by name keeping highest install count.
    // Also track which keyword found which skill names.
    let mut seen: HashMap<String, Skill> = HashMap::new();
    let mut keyword_skill_map: HashMap<String, Vec<String>> = HashMap::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((keyword, market_result))) => {
                let mut names_for_keyword = Vec::new();
                for skill in market_result.skills {
                    let key = skill.name.to_lowercase();
                    names_for_keyword.push(skill.name.clone());
                    let entry = seen.entry(key).or_insert_with(|| skill.clone());
                    if skill.stars > entry.stars {
                        *entry = skill;
                    }
                }
                keyword_skill_map.insert(keyword, names_for_keyword);
            }
            Ok(Err(e)) => {
                warn!(target: "ai_search", error = %e, "keyword search error");
            }
            Err(e) => {
                warn!(target: "ai_search", error = %e, "task join error");
            }
        }
    }

    let mut skills: Vec<Skill> = seen.into_values().collect();
    skills.sort_by(|a, b| b.stars.cmp(&a.stars));
    for (i, skill) in skills.iter_mut().enumerate() {
        skill.rank = Some((i + 1) as u32);
    }
    let total_count = skills.len() as u32;

    info!(
        target: "ai_search",
        unique_skills = total_count,
        keywords = keywords.len(),
        "merged search results"
    );

    Ok(AiKeywordSearchResult {
        skills,
        total_count,
        keyword_skill_map,
    })
}

#[cfg(test)]
mod ai_search_tests {
    use super::ai_search_by_keywords;

    /// Integration test: verifies keyword_skill_map is correctly populated.
    /// Uses real network calls, so marked #[ignore] for CI.
    /// Run with: cargo test ai_search_returns_keyword_map -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn ai_search_returns_keyword_map() {
        let keywords = vec!["react".to_string(), "typescript".to_string()];
        let result = ai_search_by_keywords(&keywords).await.unwrap();

        eprintln!("Total skills returned: {}", result.skills.len());
        eprintln!(
            "keyword_skill_map keys: {:?}",
            result.keyword_skill_map.keys().collect::<Vec<_>>()
        );

        // 1) Should return some skills
        assert!(
            !result.skills.is_empty(),
            "Expected at least 1 skill from search"
        );

        // 2) keyword_skill_map should have entries for each keyword
        for kw in &keywords {
            let names = result.keyword_skill_map.get(kw);
            assert!(
                names.is_some(),
                "keyword_skill_map missing entry for '{}'",
                kw
            );
            let names = names.unwrap();
            eprintln!(
                "Keyword '{}' found {} skills: {:?}",
                kw,
                names.len(),
                &names[..names.len().min(5)]
            );
            assert!(
                !names.is_empty(),
                "Expected at least 1 skill for keyword '{}'",
                kw
            );
        }

        // 3) total_count matches skills vec length
        assert_eq!(
            result.total_count as usize,
            result.skills.len(),
            "total_count should match skills.len()"
        );

        // 4) Every returned skill should be attributed to at least one keyword
        let all_attributed: std::collections::HashSet<String> = result
            .keyword_skill_map
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();
        for skill in &result.skills {
            assert!(
                all_attributed.contains(&skill.name),
                "Skill '{}' not found in any keyword_skill_map entry",
                skill.name
            );
        }

        // 5) Serializes to JSON correctly (simulates what Tauri sends to frontend)
        let json = serde_json::to_value(&result).unwrap();
        assert!(
            json["keyword_skill_map"].is_object(),
            "keyword_skill_map should serialize as JSON object"
        );
        assert!(
            json["skills"].is_array(),
            "skills should serialize as JSON array"
        );
        let map = json["keyword_skill_map"].as_object().unwrap();
        eprintln!("JSON keyword_skill_map: {} keys", map.len());
        for (k, v) in map {
            eprintln!("  '{}': {} skills", k, v.as_array().unwrap().len());
        }
    }

    #[tokio::test]
    #[ignore]
    async fn ai_search_empty_keywords_returns_empty() {
        let result = ai_search_by_keywords(&[]).await.unwrap();
        assert!(result.skills.is_empty());
        assert_eq!(result.total_count, 0);
        assert!(result.keyword_skill_map.is_empty());
    }
}
