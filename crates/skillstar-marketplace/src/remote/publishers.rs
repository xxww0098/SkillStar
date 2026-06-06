use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use tracing::{debug, warn};

use super::*;
use crate::OfficialPublisher;

// ── Official Publishers ────────────────────────────────────────────────

/// Get official publishers from skills.sh/official via HTML scraping
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>> {
    let client = marketplace_client()?;

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

pub(crate) fn parse_official_publishers_html(html: &str) -> Vec<OfficialPublisher> {
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
