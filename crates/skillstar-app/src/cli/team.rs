//! CLI adapter for `skillstar team` — team intelligence facade.

use chrono::Utc;
use skillstar_skills::team::{
    self, DEFAULT_RECALL_LIMIT, FrictionInput, LearningDraft, RecallKind,
};

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("✗ {message}");
    std::process::exit(1);
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => fail(format!("Failed to serialize JSON: {error}")),
    }
}

pub fn cmd_recall(query: &str, limit: u32, json: bool) {
    let limit = usize::try_from(limit).unwrap_or(DEFAULT_RECALL_LIMIT);
    match team::recall(query, limit, Utc::now()) {
        Ok(hits) => {
            if json {
                print_json(&hits);
                return;
            }
            if hits.is_empty() {
                println!("No recall hits for '{query}'.");
                return;
            }
            println!("Recall {} hit(s) for '{query}':\n", hits.len());
            for (index, hit) in hits.iter().enumerate() {
                let kind = match hit.kind {
                    RecallKind::Skill => "skill",
                    RecallKind::Learning => "note",
                };
                println!(
                    "{:>2}. [{kind}] {}  ({:.2})",
                    index + 1,
                    hit.title,
                    hit.score
                );
                if !hit.snippet.is_empty() {
                    println!("    {}", truncate(&hit.snippet, 110));
                }
            }
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_health(json: bool) {
    match team::health(Utc::now()) {
        Ok(rows) => {
            if json {
                print_json(&rows);
                return;
            }
            if rows.is_empty() {
                println!("No installed skills. Use 'skillstar add <url>' first.");
                return;
            }
            println!(
                "{:<22} {:<8} {:>5} {:>6} {:>6} SCORE",
                "NAME", "STATUS", "USE", "RECALL", "FRESH"
            );
            println!("{}", "-".repeat(64));
            for row in &rows {
                let status = match row.status {
                    team::SkillHealthStatus::Healthy => "healthy",
                    team::SkillHealthStatus::Aging => "aging",
                    team::SkillHealthStatus::Silent => "silent",
                    team::SkillHealthStatus::Stale => "stale",
                };
                println!(
                    "{:<22} {:<8} {:>5} {:>6} {:>6.2} {:>5.2}",
                    truncate(&row.name, 22),
                    status,
                    row.usage_count,
                    row.recall_count,
                    row.freshness,
                    row.score
                );
            }
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_digest(json: bool) {
    match team::digest(Utc::now()) {
        Ok(report) => {
            if json {
                print_json(&report);
                return;
            }
            println!("Team digest");
            println!(
                "  skills {}   notes {}   coverage {:.0}%",
                report.skill_count,
                report.learning_count,
                report.coverage * 100.0
            );
            println!(
                "  friction sessions {}   worth documenting {}",
                report.friction_sessions, report.worth_documenting
            );
            if !report.top_skills.is_empty() {
                println!("  top skills:");
                for row in &report.top_skills {
                    println!("    - {} ({})", row.name, row.usage_count);
                }
            }
            if !report.silent_skills.is_empty() {
                println!("  silent: {}", report.silent_skills.join(", "));
            }
            if !report.recent_learnings.is_empty() {
                println!("  recent notes:");
                for learning in &report.recent_learnings {
                    println!("    - {}", learning.title);
                }
            }
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_share(
    title: &str,
    body: &str,
    skill: Option<&str>,
    tags: &[String],
    confidence: f32,
    json: bool,
) {
    let draft = LearningDraft {
        title: title.to_string(),
        body: body.to_string(),
        tags: tags.to_vec(),
        skill_name: skill.map(ToOwned::to_owned),
        confidence,
    };
    match team::share_learning(draft, Utc::now()) {
        Ok(learning) => {
            if json {
                print_json(&learning);
                return;
            }
            println!("✓ Saved note {}", learning.id);
            println!("  {}", learning.title);
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_notes(json: bool) {
    match team::list_learnings() {
        Ok(learnings) => {
            if json {
                print_json(&learnings);
                return;
            }
            if learnings.is_empty() {
                println!("No team notes yet. Use 'skillstar team share --title … --body …'.");
                return;
            }
            for learning in &learnings {
                let skill = learning.skill_name.as_deref().unwrap_or("-");
                println!(
                    "{}  [{}]  {}",
                    &learning.id[..learning.id.len().min(8)],
                    skill,
                    learning.title
                );
            }
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_friction(
    interrupts: u32,
    rejects: u32,
    retries: u32,
    corrections: u32,
    task: Option<&str>,
    json: bool,
) {
    let input = FrictionInput {
        interrupts,
        rejects,
        retries,
        corrections,
        task: task.map(ToOwned::to_owned),
    };
    match team::record_friction(input, Utc::now()) {
        Ok(record) => {
            if json {
                print_json(&record);
                return;
            }
            println!(
                "Friction score {} ({})",
                record.score,
                if record.worth_documenting {
                    "worth documenting"
                } else {
                    "below threshold"
                }
            );
            if record.worth_documenting {
                println!(
                    "This session may contain a problem worth documenting. Consider:\n  skillstar team share --title \"…\" --body \"…\""
                );
            }
        }
        Err(error) => fail(error),
    }
}

pub fn cmd_used(name: &str) {
    match team::record_usage(name, Utc::now()) {
        Ok(()) => println!("✓ Recorded use of {name}"),
        Err(error) => fail(error),
    }
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}
