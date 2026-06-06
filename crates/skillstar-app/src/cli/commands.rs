use skillstar_core::types::lockfile::Lockfile;
use skillstar_core::infra::paths::{hub_skills_dir, local_skills_dir, lockfile_path};
use skillstar_marketplace::snapshot;

/// List scope filter for `skillstar list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter {
    All,
    HubOnly,
    LocalOnly,
}

#[must_use]
pub fn cmd_list_filter(local: bool, hub: bool) -> ListFilter {
    match (local, hub) {
        (true, false) => ListFilter::LocalOnly,
        (false, true) => ListFilter::HubOnly,
        _ => ListFilter::All,
    }
}

/// Default value for `skillstar init` when no name is supplied.
const DEFAULT_INIT_NAME: &str = "my-new-skill";

const SKILL_TEMPLATE: &str = "---
name: {{name}}
description: A new SkillStar skill
---

# {{title}}

Add your skill instructions here.

## When to Use

Describe the scenarios where this skill should be used.

## Steps

1. First, do this
2. Then, do that
";

pub fn cmd_init(name: Option<&str>) {
    let skill_name = name.unwrap_or(DEFAULT_INIT_NAME);
    if skill_name.trim().is_empty() {
        eprintln!("✗ Skill name cannot be empty");
        std::process::exit(1);
    }
    let dir = std::env::current_dir().unwrap_or_default().join(skill_name);

    if dir.exists() {
        eprintln!("✗ Path already exists: {}", dir.display());
        std::process::exit(1);
    }

    println!("Creating skill template at {}...", dir.display());

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("✗ Failed to create directory: {}", e);
        std::process::exit(1);
    }

    let title = title_case(skill_name);
    let content = SKILL_TEMPLATE
        .replace("{{name}}", skill_name)
        .replace("{{title}}", &title);

    if let Err(e) = std::fs::write(dir.join("SKILL.md"), content) {
        eprintln!("✗ Failed to write SKILL.md: {}", e);
        std::process::exit(1);
    }

    println!("✓ Skill template created at {}", dir.display());
    println!("  Edit SKILL.md, then run 'skillstar publish' to share it.");
}

fn title_case(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rich `list` that covers hub, local authored, and lockfile-only entries.
pub fn cmd_list(filter: ListFilter) {
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path).unwrap_or_default();
    let hub_dir = hub_skills_dir();
    let local_dir = local_skills_dir();

    let mut rows: Vec<ListRow> = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();

    // Hub skills: directory children + symlinks
    if matches!(filter, ListFilter::All | ListFilter::HubOnly) {
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                let meta = match path.symlink_metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let is_symlink = skillstar_core::infra::fs_ops::is_link(&path);
                let broken = is_symlink && !path.exists();
                if !is_symlink && !meta.is_dir() {
                    continue;
                }
                seen.insert(name.clone());
                let entry_row = lockfile.skills.iter().find(|s| s.name == name);
                rows.push(ListRow {
                    name,
                    kind: RowKind::Hub,
                    git_url: entry_row.map(|e| e.git_url.clone()).unwrap_or_default(),
                    tree_hash: entry_row.map(|e| e.tree_hash.clone()).unwrap_or_default(),
                    deploy: if broken {
                        DeployKind::Broken
                    } else if is_symlink {
                        DeployKind::Link
                    } else {
                        DeployKind::Dir
                    },
                });
            }
        }
        // Orphan lockfile entries — installed in lockfile but missing on disk
        for entry in &lockfile.skills {
            if seen.contains(&entry.name) {
                continue;
            }
            seen.insert(entry.name.clone());
            rows.push(ListRow {
                name: entry.name.clone(),
                kind: RowKind::Hub,
                git_url: entry.git_url.clone(),
                tree_hash: entry.tree_hash.clone(),
                deploy: DeployKind::Missing,
            });
        }
    }

    // Local authored skills
    if matches!(filter, ListFilter::All | ListFilter::LocalOnly)
        && let Ok(entries) = std::fs::read_dir(&local_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                rows.push(ListRow {
                    name,
                    kind: RowKind::Local,
                    git_url: String::new(),
                    tree_hash: String::new(),
                    deploy: DeployKind::Dir,
                });
            }
        }

    if rows.is_empty() {
        match filter {
            ListFilter::LocalOnly => {
                println!("No local authored skills. Use 'skillstar init <name>' to start one.");
            }
            ListFilter::HubOnly => {
                println!("No hub skills installed. Use 'skillstar install <url>' to add one.");
            }
            ListFilter::All => {
                println!(
                    "No skills installed. Use 'skillstar install <url>' or 'skillstar init <name>' to start."
                );
            }
        }
        return;
    }

    rows.sort_by_key(|a| a.name.to_lowercase());

    println!(
        "{:<4} {:<25} {:<8} {:<50} TREE",
        "KIND", "NAME", "DEPLOY", "GIT URL",
    );
    println!("{}", "-".repeat(100));
    for row in &rows {
        let short_hash: String = row.tree_hash.chars().take(8).collect();
        println!(
            "{:<4} {:<25} {:<8} {:<50} {}",
            row.kind.label(),
            truncate(&row.name, 25),
            row.deploy.label(),
            truncate(&row.git_url, 50),
            short_hash,
        );
    }
    let total = rows.len();
    println!("\n{} skill(s) listed.", total);
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

struct ListRow {
    name: String,
    kind: RowKind,
    git_url: String,
    tree_hash: String,
    deploy: DeployKind,
}

enum RowKind {
    Hub,
    Local,
}

impl RowKind {
    fn label(&self) -> &'static str {
        match self {
            RowKind::Hub => "hub",
            RowKind::Local => "loc",
        }
    }
}

enum DeployKind {
    Link,
    Dir,
    Broken,
    Missing,
}

impl DeployKind {
    fn label(&self) -> &'static str {
        match self {
            DeployKind::Link => "link",
            DeployKind::Dir => "dir",
            DeployKind::Broken => "broken",
            DeployKind::Missing => "missing",
        }
    }
}

/// `skillstar find [query]` — marketplace search via local snapshot (local-first).
pub fn cmd_find(query: Option<&str>, limit: u32, json: bool) {
    let query = match query {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => {
            eprintln!(
                "Usage: skillstar find <query> [--limit N] [--json]\n\
                 Interactive picker is GUI-only for now; pass a keyword here."
            );
            std::process::exit(2);
        }
    };

    let limit = limit.clamp(1, 200);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("✗ Failed to start async runtime: {}", e);
            std::process::exit(1);
        }
    };

    let result = runtime.block_on(snapshot::search_local(&query, Some(limit)));
    match result {
        Ok(result) => {
            if json {
                match serde_json::to_string_pretty(&result) {
                    Ok(s) => println!("{}", s),
                    Err(e) => {
                        eprintln!("✗ Failed to serialize result: {}", e);
                        std::process::exit(1);
                    }
                }
                return;
            }

            let skills = &result.data;
            if skills.is_empty() {
                println!(
                    "No matches for '{}' (snapshot: {:?}). Try a different keyword or run the GUI to seed the marketplace cache.",
                    query, result.snapshot_status
                );
                return;
            }

            println!(
                "Found {} skill(s) for '{}' (snapshot: {:?}):\n",
                skills.len(),
                query,
                result.snapshot_status
            );
            println!(
                "{:<6} {:<32} {:<8} INSTALL HINT",
                "STARS", "NAME", "TYPE"
            );
            println!("{}", "-".repeat(90));
            for skill in skills {
                let install_hint = install_hint_for(skill);
                println!(
                    "{:<6} {:<32} {:<8} {}",
                    skill.stars,
                    truncate(&skill.name, 32),
                    if skill.installed { "inst" } else { "new" },
                    install_hint,
                );
                if !skill.description.trim().is_empty() {
                    println!("       {}", truncate(&skill.description, 80));
                }
            }
            println!();
            println!("Install with: skillstar install <name-or-source>");
        }
        Err(e) => {
            eprintln!("✗ Search failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn install_hint_for(skill: &skillstar_marketplace::Skill) -> String {
    if !skill.git_url.trim().is_empty() {
        return skill.git_url.clone();
    }
    if let Some(source) = skill.source.as_deref()
        && !source.trim().is_empty() {
            return source.to_string();
        }
    skill.name.clone()
}
