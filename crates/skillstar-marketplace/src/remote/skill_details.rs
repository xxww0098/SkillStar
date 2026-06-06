use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::*;
use crate::Skill;

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

    let client = marketplace_client()?;

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
    skills.sort_by_key(|s| std::cmp::Reverse(s.stars));
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
