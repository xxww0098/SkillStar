use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use tracing::{debug, warn};

use super::*;
use crate::Skill;

// ── Cached Regexes (compiled once, reused forever) ─────────────────

pub(crate) fn re_strip_html() -> &'static Regex {
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
    let client = marketplace_client()?;

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

    let mut skills = parse_skills_sh_html(&html);
    debug!(target: "skills_sh", count = skills.len(), "parsed skills from HTML");

    // Supplement with search API to get ALL skills beyond the SSR payload.
    // The SSR payload only contains ~500-600 top skills; the search API
    // can return the full registry (~50K+).
    let api_skills = fetch_all_skills_via_api(&client).await;
    match api_skills {
        Ok(mut extra) => {
            let existing: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
            let next_rank = skills.len() as u32 + 1;
            extra.sort_by_key(|s| std::cmp::Reverse(s.stars));
            let mut appended = 0u32;
            for mut s in extra {
                if existing.contains(&s.name) {
                    continue;
                }
                s.rank = Some(next_rank + appended);
                appended += 1;
                skills.push(s);
            }
            debug!(
                target: "skills_sh",
                appended,
                total = skills.len(),
                "supplemented leaderboard with API skills"
            );
        }
        Err(err) => {
            warn!(target: "skills_sh", error = %err, "API supplement failed, using SSR-only data");
        }
    }

    if skills.is_empty() {
        warn!(target: "skills_sh", "HTML parsing failed, using search API fallback");
        let fallback_url = "https://skills.sh/api/search?q=skill&limit=100000";
        let response: SkillsShSearchResponse = client
            .get(fallback_url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .context("Fallback failed")?
            .json()
            .await
            .context("Fallback parse failed")?;
        let mut result: Vec<Skill> = response.skills.into_iter().map(Skill::from).collect();
        result.sort_by_key(|s| std::cmp::Reverse(s.stars));
        for (i, skill) in result.iter_mut().enumerate() {
            skill.rank = Some((i + 1) as u32);
        }
        return Ok(result);
    }

    Ok(skills)
}

/// Fetch the full skills.sh registry via the search API.
async fn fetch_all_skills_via_api(client: &reqwest::Client) -> Result<Vec<Skill>> {
    let url = "https://skills.sh/api/search?q=skill&limit=100000";
    debug!(target: "skills_sh", "fetching full registry via search API");
    let response: SkillsShSearchResponse = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to fetch full skill registry")?
        .json()
        .await
        .context("Failed to parse full registry response")?;
    let skills: Vec<Skill> = response.skills.into_iter().map(Skill::from).collect();
    debug!(target: "skills_sh", count = skills.len(), "fetched full registry");
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
