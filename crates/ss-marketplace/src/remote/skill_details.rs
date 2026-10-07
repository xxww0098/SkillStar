use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use ts_rs::TS;

use super::*;

// ── Skill Detail Page Fetching ─────────────────────────────────────────

/// The one-line description of a skill, from its skills.sh detail page.
///
/// The leaderboard SSR payload and `/api/search` rows carry no description at
/// all — probed 2026-10-06, the SSR skill objects expose only
/// `source/skillId/name/installs/weeklyInstalls/isOfficial` — so the detail
/// page's JSON-LD `SoftwareApplication.description` (the same one-liner the
/// site renders as its Summary) is the cheapest structured source for it.
/// Used by the snapshot's description backfill; do not use it on hot paths,
/// it is one page fetch per skill.
pub async fn fetch_skill_description(source: &str, name: &str) -> Result<Option<String>> {
    let path = format!("/{}/{}", source, name);
    let (html, meta) = fetch_with_failover(&path, None, None).await?;
    if meta.payload_sha256.is_empty() || html.contains("__next_error__") {
        return Ok(None);
    }
    Ok(extract_jsonld_description(&html))
}

/// Pick the `SoftwareApplication` JSON-LD block's description out of a detail
/// page. Pages embed several blocks (BreadcrumbList, WebSite, …); only the
/// SoftwareApplication one describes the skill.
pub(crate) fn extract_jsonld_description(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(start) = rest.find("<script type=\"application/ld+json\">") {
        let body_start = start + "<script type=\"application/ld+json\">".len();
        let Some(body_end) = rest[body_start..].find("</script>") else {
            break;
        };
        let body = &rest[body_start..body_start + body_end];
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
            && value.get("@type").and_then(|t| t.as_str()) == Some("SoftwareApplication")
            && let Some(text) = value
                .get("description")
                .and_then(|d| d.as_str())
                .map(str::trim)
                .filter(|d| !d.is_empty())
        {
            return Some(text.to_string());
        }
        rest = &rest[body_start + body_end + "</script>".len()..];
    }
    None
}

/// One security-audit row from the skill's skills.sh page.
/// Carried only by [`MarketplaceSkillDetails`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SecurityAudit {
    pub name: String,
    pub result: String,
}

/// Rich detail for a single marketplace skill.
///
/// `security_audits` lives on this struct. A separate hand-written mirror
/// once omitted it, so the UI could not see the audit results.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

/// Fetch rich detail data for a single skill from its skills.sh page, with
/// content-addressing metadata.
///
/// URL format: `https://skills.sh/{source}/{name}`
/// where source = "org/repo" and name = skill name.
///
/// Returns `None`-filled fields on partial failure; the caller should
/// gracefully fall back to the original truncated description.
///
/// No `.0` convenience wrapper by design — see
/// [`get_skills_sh_leaderboard_with_meta`](super::get_skills_sh_leaderboard_with_meta).
pub async fn fetch_marketplace_skill_details_with_meta(
    source: &str,
    name: &str,
    etag: Option<&str>,
    etag_host: Option<&str>,
) -> Result<(MarketplaceSkillDetails, FetchMeta)> {
    let path = format!("/{}/{}", source, name);
    debug!(target: "skills_sh", path = %path, "fetching skill details");

    let (html, meta) = fetch_with_failover(&path, etag, etag_host).await?;
    if meta.payload_sha256.is_empty() {
        return Ok((
            MarketplaceSkillDetails {
                summary: None,
                readme: None,
                weekly_installs: None,
                github_stars: None,
                first_seen: None,
                security_audits: Vec::new(),
            },
            meta,
        ));
    }

    // Check for Next.js error page
    if html.contains("__next_error__") {
        anyhow::bail!("Skill page not found (Next.js error page)");
    }

    Ok((parse_skill_detail_html(&html), meta))
}

fn parse_skill_detail_html(html: &str) -> MarketplaceSkillDetails {
    // ── Summary ────────────────────────────────────────────────────
    let summary = extract_prose_block(html, ">Summary</div>")
        .and_then(|inner| prose_html_to_markdown(&inner))
        .filter(|s| !s.trim().is_empty());

    // ── SKILL.md ───────────────────────────────────────────────────
    let readme = extract_prose_block(html, "SKILL.md</span>")
        .and_then(|inner| prose_html_to_markdown(&inner))
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

/// Convert one scraped prose block from HTML to Markdown.
///
/// Backed by `htmd` (Apache-2.0, same licence as this repo). It replaced
/// `html2md`, which is GPL-3.0-or-later and therefore could not be statically
/// linked into an Apache-2.0 desktop binary — see the history note in
/// the workspace `deny.toml`.
///
/// `htmd::convert` is fallible (`io::Error` out of the Markdown writer) where
/// `html2md::parse_html` was not, so a failure is folded back into `None`:
/// both call sites already treat a missing field as "fall back to the
/// truncated description", and swallowing the error with `unwrap_or_default`
/// would turn a broken conversion into an indistinguishable empty field.
fn prose_html_to_markdown(inner_html: &str) -> Option<String> {
    match htmd::convert(inner_html) {
        Ok(markdown) => Some(markdown),
        Err(e) => {
            warn!(
                target: "skills_sh",
                error = %e,
                len = inner_html.len(),
                "html-to-markdown conversion failed; dropping prose block"
            );
            None
        }
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

// ── Tests ───────────────────────────────────────────────────────────
//
// These cover the HTML→Markdown seam, which had no coverage at all when the
// converter was swapped from `html2md` (GPL-3.0+) to `htmd` (Apache-2.0).
// They assert on *structure surviving the conversion* — headings, lists,
// fenced code with its language, links, tables — rather than on byte-exact
// output, because the exact whitespace and escaping style is the converter's
// business and pinning it would make every upgrade a false failure.
#[cfg(test)]
mod tests {
    use super::*;

    /// A page fragment shaped like a real skills.sh detail page: a `Summary`
    /// prose block and a `SKILL.md` prose block, each inside the
    /// `<div class="prose ...">` container the extractor looks for.
    const DETAIL_PAGE: &str = concat!(
        r#"<div class="text-sm">Summary</div>"#,
        r#"<div class="prose prose-sm"><p>Create <strong>well-formed</strong> commits.</p></div>"#,
        r#"<span class="font-mono">SKILL.md</span>"#,
        r#"<div class="prose prose-sm">"#,
        r#"<h2>Overview</h2>"#,
        r#"<p>Scaffold exercises, see <a href="https://skills.sh/docs">the docs</a>.</p>"#,
        r#"<ul><li>Creates <code>section/</code> dirs</li><li>Writes README<ul><li>nested</li></ul></li></ul>"#,
        r#"<pre><code class="language-bash">git add -A</code></pre>"#,
        r#"<table><thead><tr><th>Flag</th><th>Meaning</th></tr></thead>"#,
        r#"<tbody><tr><td>--all</td><td>Everything</td></tr></tbody></table>"#,
        r#"</div>"#,
    );

    #[test]
    fn converts_summary_prose_block_to_markdown() {
        let details = parse_skill_detail_html(DETAIL_PAGE);
        let summary = details.summary.expect("summary should be extracted");
        assert!(
            summary.contains("**well-formed**"),
            "inline emphasis lost: {summary:?}"
        );
    }

    #[test]
    fn skill_md_prose_block_keeps_every_block_construct() {
        let details = parse_skill_detail_html(DETAIL_PAGE);
        let readme = details.readme.expect("readme should be extracted");

        // Heading — ATX, and it must not be swallowed.
        assert!(readme.contains("## Overview"), "heading lost: {readme:?}");
        // Link with its href intact.
        assert!(
            readme.contains("[the docs](https://skills.sh/docs)"),
            "link lost: {readme:?}"
        );
        // List, including the nested level (indented, not flattened).
        assert!(
            readme.contains("Creates `section/` dirs"),
            "list item / inline code lost: {readme:?}"
        );
        assert!(
            readme
                .lines()
                .any(|l| l.starts_with("    ") && l.contains("nested")),
            "nested list flattened: {readme:?}"
        );
        // Fenced code block *with* its language — `html2md` used to drop the
        // language, `htmd` keeps it; this is the one behaviour change we
        // actually want to hold on to.
        assert!(
            readme.contains("```bash"),
            "code fence language lost: {readme:?}"
        );
        assert!(readme.contains("git add -A"), "code body lost: {readme:?}");
        // Table survives as a Markdown table, header row and all.
        assert!(
            readme.contains("| Flag") && readme.contains("| Meaning"),
            "table header lost: {readme:?}"
        );
        assert!(
            readme
                .lines()
                .any(|l| l.contains("---") && l.starts_with('|')),
            "table delimiter row lost: {readme:?}"
        );
        assert!(readme.contains("Everything"), "table body lost: {readme:?}");
    }

    #[test]
    fn missing_prose_blocks_yield_none_not_empty_strings() {
        let details = parse_skill_detail_html("<div>no prose here</div>");
        assert!(details.summary.is_none());
        assert!(details.readme.is_none());
    }

    #[test]
    fn whitespace_only_prose_block_is_filtered_out() {
        let html = concat!(
            r#"<div class="text-sm">Summary</div>"#,
            r#"<div class="prose"><p>   </p></div>"#,
        );
        assert!(parse_skill_detail_html(html).summary.is_none());
    }

    #[test]
    fn converter_tolerates_unclosed_and_non_html_input() {
        // The extractor hands over whatever it scraped; the converter must not
        // panic on malformed fragments.
        assert_eq!(
            prose_html_to_markdown("<p>unclosed").as_deref(),
            Some("unclosed")
        );
        assert_eq!(prose_html_to_markdown("").as_deref(), Some(""));
    }

    /// Shaped like a real skills.sh detail page head: several ld+json blocks,
    /// of which only the `SoftwareApplication` one carries the description.
    const DETAIL_PAGE_LD_JSON: &str = concat!(
        r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"BreadcrumbList","itemListElement":[]}</script>"#,
        r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"SoftwareApplication","name":"find-skills","description":"Helps users discover and install agent skills.","url":"https://www.skills.sh/vercel-labs/skills/find-skills","applicationCategory":"DeveloperApplication"}</script>"#,
        r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"WebSite","description":"Discover and install skills for AI agents."}</script>"#,
    );

    #[test]
    fn jsonld_description_comes_from_the_software_application_block() {
        assert_eq!(
            extract_jsonld_description(DETAIL_PAGE_LD_JSON).as_deref(),
            Some("Helps users discover and install agent skills.")
        );
    }

    #[test]
    fn jsonld_description_ignores_empty_and_missing_blocks() {
        assert_eq!(extract_jsonld_description("<html>no ld+json</html>"), None);
        assert_eq!(
            extract_jsonld_description(
                r#"<script type="application/ld+json">{"@type":"SoftwareApplication","description":"   "}</script>"#
            ),
            None
        );
        // Unterminated final block must not loop or panic.
        assert_eq!(
            extract_jsonld_description(r#"<script type="application/ld+json">{"@type":"WebSite""#),
            None
        );
    }
}
