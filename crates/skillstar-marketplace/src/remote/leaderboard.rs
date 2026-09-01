use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow};
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

/// Get skills.sh leaderboard via HTML scraping, with content-addressing
/// metadata (source host, payload SHA-256, ETag) for the snapshot's
/// incremental write path.
///
/// There is deliberately no `.0` convenience wrapper: `FetchMeta` carries
/// `degraded`, and every caller that dropped it — the `ErrorFallback` branches
/// in particular — silently turned a knowingly lossy payload into one the UI
/// presented as complete.
pub async fn get_skills_sh_leaderboard_with_meta(
    category: &str,
    etag: Option<&str>,
    etag_host: Option<&str>,
) -> Result<(Vec<Skill>, FetchMeta)> {
    // Map category to URL path
    let url_path = match category {
        "hot" => "/hot",
        "popular" | "all" => "/",
        "trending" => "/trending",
        _ => "/",
    };

    let (html, mut meta) = fetch_with_failover(url_path, etag, etag_host).await?;
    debug!(target: "skills_sh", host = %meta.source_host, "fetching leaderboard");

    // 304 Not Modified: the snapshot layer keeps its previous write.
    if meta.payload_sha256.is_empty() {
        return Ok((Vec::new(), meta));
    }

    let html_skills = parse_skills_sh_html(&html);
    debug!(target: "skills_sh", count = html_skills.len(), "parsed skills from HTML");

    // Supplement with the search API. The SSR payload carries the ~500-600 top
    // skills; the search API adds at most `SEARCH_API_HARD_LIMIT` fuzzy matches
    // on top of it — it is a supplement, never a full registry dump.
    let (skills, degraded) = combine_leaderboard(html_skills, fetch_all_skills_via_api().await)?;
    meta.degraded = degraded;

    Ok((skills, meta))
}

/// Merge the SSR leaderboard with the search-API supplement, and answer whether
/// the result is a real leaderboard.
///
/// The degradation is decided on `html_skills` — before the append — because
/// that is the only moment the two are still distinguishable. The search API is
/// a supplement; when the SSR payload stops parsing (skills.sh redesign, a WAF
/// challenge page, a mirror serving something else) that supplement silently
/// becomes the *whole* answer: ≤`SEARCH_API_HARD_LIMIT` fuzzy matches for the
/// literal word "skill", ordered by stars, carrying none of skills.sh's
/// ranking. Asking `skills.is_empty()` one line later — after the append made
/// it non-empty — is what left the entire degraded mechanism unreachable in the
/// exact scenario it was built for: 200 fallback rows replacing ~600 real ones,
/// with a full 6-hour TTL and a "fresh" label. See `docs/errors.md`.
///
/// Both halves failing is not a degraded payload but no payload: it errors,
/// rather than committing an empty leaderboard over a populated one.
pub(crate) fn combine_leaderboard(
    html_skills: Vec<Skill>,
    api_skills: Result<Vec<Skill>>,
) -> Result<(Vec<Skill>, bool)> {
    let mut skills = html_skills;
    let degraded = skills.is_empty();
    if degraded {
        warn!(
            target: "skills_sh",
            "leaderboard HTML parsing produced nothing; the capped search API is standing in for the whole leaderboard"
        );
    }

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
            if skills.is_empty() {
                // A second call to the very same endpoint (what this used to
                // do) goes through the same failover and the same serde parse,
                // so it fails for the same reason.
                return Err(err.context(
                    "Leaderboard HTML parsed to nothing and the search API fallback failed",
                ));
            }
        }
    }

    if skills.is_empty() {
        return Err(anyhow!("Leaderboard payload contained no skills"));
    }

    Ok((skills, degraded))
}

/// Fuzzy term the supplement searches for. Not a wildcard — `/api/search`
/// rejects a query shorter than 2 characters (`{"error":"Query must be at
/// least 2 characters"}`) and has no "match everything" syntax, so this is a
/// literal substring match against skill names, nothing more.
const SEARCH_SUPPLEMENT_QUERY: &str = "skill";

/// Hard server-side cap on `/api/search`, probed against `https://skills.sh`
/// on 2026-08-12:
///
/// * `limit` *is* honoured, but only below the cap — `limit=199` returns 199
///   rows, `limit=200` returns 200, and `limit=201`, `limit=500`,
///   `limit=100000` all return exactly 200. Omitting `limit` returns 100.
/// * There is no pagination of any kind. `offset`, `page`, `p`, `cursor`,
///   `after`, `skip`, `start`, `from` and `pageSize` are all *silently
///   ignored*: every one of them came back with the default first page (same
///   leading row, same 100 entries). The body carries only
///   `{query, searchType, skills, count, duration_ms}` and the response headers
///   carry no `Link`/`X-Total-Count`, so there is no cursor to follow either.
/// * No sibling endpoint exposes the full registry: `/api/skills`,
///   `/api/leaderboard`, `/api/all`, `/api/registry` and `/api/stats` are 404.
/// * Rows are `{id, skillId, name, installs, source}` — no `description`, no
///   `repoUrl`.
///
/// So this call can never be a full registry dump, and asking for a huge
/// `limit` does not make it one. It stays what its name says: a supplement of
/// at most 200 fuzzy matches appended after the SSR leaderboard. Re-probe
/// before treating any larger number as reachable.
pub(crate) const SEARCH_API_HARD_LIMIT: usize = 200;

/// Request path for the supplement: ask for exactly the cap, since anything
/// above it is discarded server-side anyway.
pub(crate) fn search_supplement_path() -> String {
    format!("/api/search?q={SEARCH_SUPPLEMENT_QUERY}&limit={SEARCH_API_HARD_LIMIT}")
}

/// Parsed `/api/search` supplement plus whether the server cap swallowed rows.
pub(crate) struct SearchSupplement {
    pub(crate) skills: Vec<Skill>,
    /// The response filled the cap, so the registry holds more matches than
    /// these — and, with no pagination parameter to follow, they are
    /// unreachable through this endpoint.
    pub(crate) truncated: bool,
}

/// Parse a `/api/search` body into the supplement (no network).
pub(crate) fn parse_search_supplement(body: &str) -> Result<SearchSupplement> {
    let response: SkillsShSearchResponse =
        serde_json::from_str(body).context("Failed to parse search supplement response")?;
    let skills: Vec<Skill> = response.skills.into_iter().map(Skill::from).collect();
    let truncated = skills.len() >= SEARCH_API_HARD_LIMIT;
    Ok(SearchSupplement { skills, truncated })
}

/// Fetch supplemental skills via the search API.
///
/// One request, by design: see [`SEARCH_API_HARD_LIMIT`] — the endpoint has no
/// offset/cursor, so a second request would return the same first page.
async fn fetch_all_skills_via_api() -> Result<Vec<Skill>> {
    debug!(target: "skills_sh", "fetching supplemental skills via search API");
    let (body, _) = fetch_with_failover(&search_supplement_path(), None, None).await?;
    let supplement = parse_search_supplement(&body)?;
    if supplement.truncated {
        debug!(
            target: "skills_sh",
            count = supplement.skills.len(),
            cap = SEARCH_API_HARD_LIMIT,
            "search supplement hit the server-side cap; the rest is unreachable (no pagination)"
        );
    }
    Ok(supplement.skills)
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
pub(crate) fn extract_skills_from_escaped_payload(html: &str) -> Vec<Skill> {
    fn re_escaped_skill_object() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            // Match flat JSON objects containing \"skillId\" (escaped quotes).
            // `[^{}]` (not `[^}]`) on both sides is required: the skill array is
            // itself wrapped in `{\"initialSkills\":[{...`, so a class that
            // allows `{` lets the leftmost match start at the *outer* brace and
            // swallow the rank-1 entry into unbalanced JSON that serde drops.
            Regex::new(r#"\{[^{}]*\\"skillId\\"[^{}]*\}"#).expect("escaped skill object regex")
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
