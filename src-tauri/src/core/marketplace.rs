use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::skill::{extract_github_source_from_url, OfficialPublisher, Skill, SkillCategory};

const DESCRIPTION_CACHE_TTL_DAYS: i64 = 14;
const DESCRIPTION_CACHE_MAX_ENTRIES: usize = 5000;
const DESCRIPTION_FETCH_CONCURRENCY: usize = 4;
const DESCRIPTION_MAX_CHARS: usize = 240;

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceResult {
    pub skills: Vec<Skill>,
    pub total_count: u32,
    pub page: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceDescriptionRequest {
    pub name: String,
    pub source: Option<String>,
    pub git_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceDescriptionPatch {
    pub key: String,
    pub name: String,
    pub source: Option<String>,
    pub description: String,
    pub from_cache: bool,
}

#[derive(Debug, Clone)]
struct NormalizedDescriptionTarget {
    key: String,
    name: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DescriptionCacheEntry {
    description: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DescriptionCacheFile {
    entries: HashMap<String, DescriptionCacheEntry>,
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
    fn from(s: SkillsShSkill) -> Self {
        let git_url = s.repo_url.unwrap_or_else(|| {
            // source is "org/repo" (e.g. "vercel/ai"), the actual GitHub repo
            format!("https://github.com/{}", s.source)
        });
        let source = Some(s.source.clone());
        Skill {
            name: s.name,
            description: s.description.unwrap_or_default(),
            stars: s.installs,
            installed: false,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url,
            tree_hash: None,
            category: SkillCategory::None,
            author: Some(s.source),
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
    let client = reqwest::Client::new();
    // Clamp limit to API maximum
    let clamped_limit = limit.min(100);
    let url = format!(
        "https://skills.sh/api/search?q={}&limit={}",
        urlencoded(query),
        clamped_limit
    );

    let response: SkillsShSearchResponse = client
        .get(&url)
        .header("User-Agent", "SkillStar/0.1.0")
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

/// Hydrate missing marketplace descriptions by fetching skills.sh skill pages.
///
/// Resolution order:
/// 1) cache hit in `marketplace_description_cache.json`
/// 2) fetch `https://skills.sh/{source}/{skill}` and parse Summary
///
/// Cache read/write failures are treated as non-fatal.
pub async fn hydrate_marketplace_descriptions(
    requests: Vec<MarketplaceDescriptionRequest>,
) -> Result<Vec<MarketplaceDescriptionPatch>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let mut cache = load_description_cache();
    prune_description_cache(&mut cache);

    let mut patches = Vec::new();
    let mut misses = Vec::new();
    let mut seen = HashSet::new();
    let now = Utc::now();

    for request in requests {
        let Some(target) = normalize_description_target(&request) else {
            continue;
        };

        if !seen.insert(target.key.clone()) {
            continue;
        }

        if let Some(entry) = cache.get(&target.key) {
            if is_cache_entry_fresh(entry, &now) && is_valid_description(&entry.description) {
                patches.push(MarketplaceDescriptionPatch {
                    key: target.key.clone(),
                    name: target.name.clone(),
                    source: Some(target.source.clone()),
                    description: entry.description.clone(),
                    from_cache: true,
                });
                continue;
            }
        }

        misses.push(target);
    }

    if !misses.is_empty() {
        let client = reqwest::Client::new();
        let fetched = fetch_descriptions_with_limited_concurrency(&client, &misses).await;

        if !fetched.is_empty() {
            for patch in fetched {
                cache.insert(
                    patch.key.clone(),
                    DescriptionCacheEntry {
                        description: patch.description.clone(),
                        updated_at: Utc::now().to_rfc3339(),
                    },
                );
                patches.push(patch);
            }
            persist_description_cache(&cache);
        }
    }

    Ok(patches)
}

async fn fetch_descriptions_with_limited_concurrency(
    client: &reqwest::Client,
    targets: &[NormalizedDescriptionTarget],
) -> Vec<MarketplaceDescriptionPatch> {
    let mut patches = Vec::new();

    for chunk in targets.chunks(DESCRIPTION_FETCH_CONCURRENCY) {
        let mut join_set = tokio::task::JoinSet::new();

        for target in chunk {
            let client = client.clone();
            let target = target.clone();
            join_set.spawn(async move { fetch_description_for_target(client, target).await });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Some(patch)) => patches.push(patch),
                Ok(None) => {}
                Err(e) => {
                    eprintln!(
                        "[hydrate_marketplace_descriptions] fetch task join error: {}",
                        e
                    );
                }
            }
        }
    }

    patches
}

async fn fetch_description_for_target(
    client: reqwest::Client,
    target: NormalizedDescriptionTarget,
) -> Option<MarketplaceDescriptionPatch> {
    let url = format!("https://skills.sh/{}/{}", target.source, target.name);
    let response = client
        .get(&url)
        .header("User-Agent", "SkillStar/0.1.0")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .ok()?;

    let html = response.error_for_status().ok()?.text().await.ok()?;
    let description = extract_summary_description_from_html(&html)?;

    if !is_valid_description(&description) {
        return None;
    }

    Some(MarketplaceDescriptionPatch {
        key: target.key,
        name: target.name,
        source: Some(target.source),
        description,
        from_cache: false,
    })
}

fn normalize_description_target(
    request: &MarketplaceDescriptionRequest,
) -> Option<NormalizedDescriptionTarget> {
    let source = request
        .source
        .as_deref()
        .and_then(normalize_source)
        .or_else(|| {
            request
                .git_url
                .as_deref()
                .and_then(extract_github_source_from_url)
                .and_then(|s| normalize_source(&s))
        })?;

    let name = normalize_skill_name(&request.name)?;
    let key = format!("{}/{}", source, name);

    Some(NormalizedDescriptionTarget { key, name, source })
}

fn normalize_source(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let source = trimmed
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_lowercase();

    let mut parts = source.split('/').filter(|s| !s.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    Some(format!("{}/{}", owner, repo))
}

fn normalize_skill_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

// ── Cached Regexes (compiled once, reused forever) ─────────────────

fn re_summary_block() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#">Summary</div><div[^>]*>\s*<div class="prose[^"]*">([\s\S]*?)</div>\s*</div>"#,
        )
        .expect("summary block regex")
    })
}

fn re_strong() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<strong>([\s\S]*?)</strong>"#).expect("strong regex"))
}

fn re_paragraph() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<p>([\s\S]*?)</p>"#).expect("paragraph regex"))
}

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

fn extract_summary_description_from_html(html: &str) -> Option<String> {
    let block = re_summary_block()
        .captures(html)
        .and_then(|caps| caps.get(1).map(|m| m.as_str()))?;

    if let Some(caps) = re_strong().captures(block) {
        let text = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let normalized = normalize_description_text(text);
        if is_valid_description(&normalized) {
            return Some(normalized);
        }
    }

    if let Some(caps) = re_paragraph().captures(block) {
        let text = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let normalized = normalize_description_text(text);
        if is_valid_description(&normalized) {
            return Some(normalized);
        }
    }

    let fallback = normalize_description_text(block);
    if fallback.is_empty() {
        None
    } else {
        Some(fallback)
    }
}

fn normalize_description_text(raw: &str) -> String {
    let stripped = re_strip_html().replace_all(raw, " ");
    let decoded = decode_html_entities(&stripped);
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, DESCRIPTION_MAX_CHARS)
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    trimmed
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn is_valid_description(description: &str) -> bool {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("install the ") && lower.contains(" skill for ") {
        return false;
    }

    if lower.starts_with("skill: ") {
        return false;
    }

    true
}

fn description_cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("skillstar")
        .join("marketplace_description_cache.json")
}

fn load_description_cache() -> HashMap<String, DescriptionCacheEntry> {
    let path = description_cache_path();
    if !path.exists() {
        return HashMap::new();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "[hydrate_marketplace_descriptions] failed to read cache {}: {}",
                path.display(),
                e
            );
            return HashMap::new();
        }
    };

    let file = match serde_json::from_str::<DescriptionCacheFile>(&content) {
        Ok(file) => file,
        Err(e) => {
            eprintln!(
                "[hydrate_marketplace_descriptions] failed to parse cache {}: {}",
                path.display(),
                e
            );
            return HashMap::new();
        }
    };

    file.entries
}

fn persist_description_cache(entries: &HashMap<String, DescriptionCacheEntry>) {
    let path = description_cache_path();
    let mut pruned = entries.clone();
    prune_description_cache(&mut pruned);

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "[hydrate_marketplace_descriptions] failed to create cache dir {}: {}",
                parent.display(),
                e
            );
            return;
        }
    }

    let content = match serde_json::to_string_pretty(&DescriptionCacheFile { entries: pruned }) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "[hydrate_marketplace_descriptions] failed to serialize cache {}: {}",
                path.display(),
                e
            );
            return;
        }
    };

    if let Err(e) = std::fs::write(&path, content) {
        eprintln!(
            "[hydrate_marketplace_descriptions] failed to write cache {}: {}",
            path.display(),
            e
        );
    }
}

fn prune_description_cache(entries: &mut HashMap<String, DescriptionCacheEntry>) {
    let now = Utc::now();

    entries.retain(|_, entry| {
        is_cache_entry_fresh(entry, &now) && is_valid_description(&entry.description)
    });

    if entries.len() <= DESCRIPTION_CACHE_MAX_ENTRIES {
        return;
    }

    let mut pairs: Vec<(String, DescriptionCacheEntry)> = entries.drain().collect();
    pairs.sort_by(|a, b| {
        cache_entry_timestamp(&b.1.updated_at).cmp(&cache_entry_timestamp(&a.1.updated_at))
    });
    pairs.truncate(DESCRIPTION_CACHE_MAX_ENTRIES);
    entries.extend(pairs);
}

fn is_cache_entry_fresh(entry: &DescriptionCacheEntry, now: &chrono::DateTime<Utc>) -> bool {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&entry.updated_at) else {
        return false;
    };

    let updated = parsed.with_timezone(&Utc);
    *now - updated <= Duration::days(DESCRIPTION_CACHE_TTL_DAYS)
}

fn cache_entry_timestamp(updated_at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(updated_at)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// Get skills.sh leaderboard via HTML scraping
pub async fn get_skills_sh_leaderboard(category: &str) -> Result<Vec<Skill>> {
    let client = reqwest::Client::new();

    // Map category to URL path
    let url_path = match category {
        "hot" => "/hot",
        "popular" | "all" => "/",
        "trending" => "/trending",
        _ => "/",
    };

    let url = format!("https://skills.sh{}", url_path);
    eprintln!("[skills.sh] Fetching: {}", url);

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
    eprintln!("[skills.sh] Parsed {} skills from HTML", skills.len());

    // If HTML parsing fails, fallback to search API
    if skills.is_empty() {
        eprintln!("[skills.sh] HTML parsing failed, using search API fallback");
        let fallback_url = "https://skills.sh/api/search?q=ai&limit=200";
        let response: SkillsShSearchResponse = client
            .get(fallback_url)
            .header("User-Agent", "SkillStar/0.1.0")
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

fn parse_skills_sh_html(html: &str) -> Vec<Skill> {
    let mut skills = Vec::new();

    // Try multiple patterns to find skill data (using cached regexes)

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

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    skills.retain(|s| seen.insert(s.name.clone()));

    // Sort by stars (installs) descending
    skills.sort_by(|a, b| b.stars.cmp(&a.stars));

    // Assign ranks
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
    let client = reqwest::Client::new();

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
    eprintln!(
        "[skills.sh] Parsed {} official publishers",
        publishers.len()
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
        eprintln!("[skills.sh] HTML parsing failed, using known publishers");
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
    let data = vec![
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

    data.into_iter()
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
    use std::collections::HashMap;

    use chrono::{Duration, Utc};

    use super::{
        extract_summary_description_from_html, is_valid_description, normalize_description_target,
        parse_official_publishers_html, prune_description_cache, DescriptionCacheEntry,
        MarketplaceDescriptionRequest, DESCRIPTION_CACHE_MAX_ENTRIES,
    };

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
    fn extracts_summary_description_prefers_strong() {
        let html = r#"
        <div>Summary</div><div><div class="prose prose-invert"><p><strong>Fast screenshot capture for desktop and windows.</strong></p><p>extra text</p></div></div>
        "#;

        let summary = extract_summary_description_from_html(html).unwrap();
        assert_eq!(summary, "Fast screenshot capture for desktop and windows.");
    }

    #[test]
    fn extracts_summary_description_falls_back_to_paragraph() {
        let html = r#"
        <div>Summary</div><div><div class="prose prose-invert"><p>Render AI chat interfaces with optimistic updates.</p></div></div>
        "#;

        let summary = extract_summary_description_from_html(html).unwrap();
        assert_eq!(
            summary,
            "Render AI chat interfaces with optimistic updates."
        );
    }

    #[test]
    fn rejects_template_install_description() {
        assert!(!is_valid_description(
            "Install the screenshot skill for openai/skills"
        ));
        assert!(!is_valid_description("   "));
        assert!(is_valid_description(
            "Capture desktop screenshots with region support."
        ));
    }

    #[test]
    fn normalizes_description_target_from_source_and_git_url() {
        let from_source = MarketplaceDescriptionRequest {
            name: "Screenshot".to_string(),
            source: Some("OpenAI/Skills".to_string()),
            git_url: None,
        };
        let target1 = normalize_description_target(&from_source).unwrap();
        assert_eq!(target1.key, "openai/skills/screenshot");

        let from_git = MarketplaceDescriptionRequest {
            name: "screenshot".to_string(),
            source: None,
            git_url: Some("https://github.com/openai/skills.git".to_string()),
        };
        let target2 = normalize_description_target(&from_git).unwrap();
        assert_eq!(target2.key, "openai/skills/screenshot");
    }

    #[test]
    fn prunes_cache_by_ttl_and_max_entries() {
        let mut entries = HashMap::new();
        let now = Utc::now();

        entries.insert(
            "old/entry".to_string(),
            DescriptionCacheEntry {
                description: "stale".to_string(),
                updated_at: (now - Duration::days(30)).to_rfc3339(),
            },
        );

        for i in 0..(DESCRIPTION_CACHE_MAX_ENTRIES + 10) {
            entries.insert(
                format!("source/skill-{}", i),
                DescriptionCacheEntry {
                    description: format!("description {}", i),
                    updated_at: (now - Duration::seconds(i as i64)).to_rfc3339(),
                },
            );
        }

        prune_description_cache(&mut entries);

        assert!(!entries.contains_key("old/entry"));
        assert!(entries.len() <= DESCRIPTION_CACHE_MAX_ENTRIES);
        assert!(entries.contains_key("source/skill-0"));
    }
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace(':', "%3A")
        .replace('>', "%3E")
        .replace('<', "%3C")
}

// ── Publisher Repos ───────────────────────────────────────────────────

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
}

/// Fetch all repos for a publisher by scraping skills.sh/<publisher_name>.
/// This returns the complete list (not limited by the search API's 100-result cap).
pub async fn get_publisher_repos(publisher_name: &str) -> Result<Vec<PublisherRepo>> {
    let client = reqwest::Client::new();
    let url = format!("https://skills.sh/{}", publisher_name.to_lowercase());

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
    eprintln!(
        "[skills.sh] Parsed {} repos for publisher '{}'",
        repos.len(),
        publisher_name
    );

    Ok(repos)
}

fn parse_publisher_repos_html(html: &str, publisher_name: &str) -> Vec<PublisherRepo> {
    let normalized = html.replace('\n', "");
    let mut repos = Vec::new();
    let publisher_lower = publisher_name.to_lowercase();

    // Pattern: href="/publisher/repo-name">...<h3>repo-name</h3>...N skills:...installs</a>
    // We look for each href="/publisher/X" link and extract repo name, skill count, installs
    let href_pattern = format!(r#"href="/{}/([a-z0-9A-Z_.-]+)""#, regex::escape(&publisher_lower));
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
        let repo_name = href_cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
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
            repo: repo_name,
            skill_count,
            installs_label,
            installs,
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
        num_str.parse::<f64>().map(|n| (n * 1_000_000.0) as u32).unwrap_or(0)
    } else if let Some(num_str) = trimmed.strip_suffix('K') {
        num_str.parse::<f64>().map(|n| (n * 1_000.0) as u32).unwrap_or(0)
    } else {
        trimmed.replace(',', "").parse::<u32>().unwrap_or(0)
    }
}

