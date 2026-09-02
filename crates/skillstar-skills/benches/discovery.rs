//! Wall-clock load for `discover_skills` on a production-shaped skill pack.
//!
//! Dataset: a cloned-repo layout with 48 catalog skills under `skills/`, 16
//! agent-local skills under `.claude/skills/`, 8 under `.codex/skills/`, and
//! 4 nested `skills/<category>/<name>/` skills. Frontmatter + body sizes match
//! real SKILL.md files (~2 KiB). Missing agent dirs from `PRIORITY_SKILL_DIRS`
//! stay absent so the miss-stat cost is part of the load.
//!
//! Pin:
//! ```text
//! cargo bench -p skillstar-skills --bench discovery --profile profiling -- --loops 80
//! ```

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use skillstar_skills::discover_skills;
use skillstar_skills::validation::inspect_skill_frontmatter_content;

const LOOPS_DEFAULT: usize = 80;
const OUTER_RUNS: usize = 5;

const SKILL_BODY: &str = r#"# Skill

Use this skill when the user asks to transform, review, or generate a document.

## Steps

1. Read the files in the skill folder.
2. Apply the instructions in the description.
3. Return a concise result, then stop.

## Examples

- "Summarize this repository"
- "Draft a changelog from recent commits"
"#;

fn skill_md(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{SKILL_BODY}")
}

fn write_skill(root: &Path, rel: &str, name: &str, description: &str) {
    let path = root.join(rel).join("SKILL.md");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create skill dir");
    }
    std::fs::write(&path, skill_md(name, description)).expect("write SKILL.md");
}

fn build_tree(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for i in 0..48 {
        let rel = format!("skills/catalog-{i:02}");
        write_skill(
            root,
            &rel,
            &format!("catalog-{i:02}"),
            "Catalog skill used when searching, installing, or summarizing a public pack.",
        );
        paths.push(root.join(rel).join("SKILL.md"));
    }
    for i in 0..16 {
        let rel = format!(".claude/skills/claude-{i:02}");
        write_skill(
            root,
            &rel,
            &format!("claude-{i:02}"),
            "Claude-local skill used when the user is already inside an agent skills directory.",
        );
        paths.push(root.join(rel).join("SKILL.md"));
    }
    for i in 0..8 {
        let rel = format!(".codex/skills/codex-{i:02}");
        write_skill(
            root,
            &rel,
            &format!("codex-{i:02}"),
            "Codex-local skill used when installing from an agent-prefixed layout.",
        );
        paths.push(root.join(rel).join("SKILL.md"));
    }
    for i in 0..4 {
        let rel = format!("skills/docs/nested-{i:02}");
        write_skill(
            root,
            &rel,
            &format!("nested-{i:02}"),
            "Nested catalog skill two levels below skills/, matching the container depth walk.",
        );
        paths.push(root.join(rel).join("SKILL.md"));
    }
    paths
}

fn mean(durations: &[Duration]) -> Duration {
    let total: Duration = durations.iter().copied().sum();
    total / u32::try_from(durations.len()).unwrap_or(1)
}

fn parse_loops() -> usize {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--loops" {
            if let Some(value) = args.next() {
                return value.parse().unwrap_or(LOOPS_DEFAULT);
            }
        } else if let Some(value) = arg.strip_prefix("--loops=") {
            return value.parse().unwrap_or(LOOPS_DEFAULT);
        }
    }
    LOOPS_DEFAULT
}

fn time_runs(runs: usize, mut body: impl FnMut()) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        body();
        samples.push(started.elapsed());
    }
    samples
}

fn print_phase(name: &str, loops: usize, samples: &[Duration]) {
    println!(
        "{name} loops={loops} runs={} times_ms={:?} mean_ms={:.2} per_call_ns={:.0}",
        samples.len(),
        samples
            .iter()
            .map(|d| format!("{:.2}", d.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>(),
        mean(samples).as_secs_f64() * 1000.0,
        mean(samples).as_secs_f64() * 1_000_000_000.0 / loops as f64,
    );
}

fn main() {
    let loops = parse_loops();
    let tree = tempfile::tempdir().expect("tempdir");
    let skill_md_paths = build_tree(tree.path());
    let sample = skill_md(
        "catalog-00",
        "Catalog skill used when searching, installing, or summarizing a public pack.",
    );

    // Warm the page cache so the timed runs measure discover/parse, not first-read.
    black_box(discover_skills(tree.path(), false));
    black_box(inspect_skill_frontmatter_content(&sample));
    for path in &skill_md_paths {
        black_box(std::fs::read_to_string(path).ok());
    }

    let discovery = time_runs(OUTER_RUNS, || {
        for _ in 0..loops {
            black_box(discover_skills(tree.path(), false));
        }
    });

    let parse_loops = loops * 64;
    let parse = time_runs(OUTER_RUNS, || {
        for _ in 0..parse_loops {
            black_box(inspect_skill_frontmatter_content(&sample));
        }
    });

    let read = time_runs(OUTER_RUNS, || {
        for _ in 0..loops {
            for path in &skill_md_paths {
                black_box(std::fs::read_to_string(path).expect("read SKILL.md"));
            }
        }
    });

    let stat = time_runs(OUTER_RUNS, || {
        for _ in 0..loops {
            for path in &skill_md_paths {
                black_box(std::fs::symlink_metadata(path).expect("stat SKILL.md"));
            }
        }
    });

    let inspect_via_read = time_runs(OUTER_RUNS, || {
        for _ in 0..loops {
            for path in &skill_md_paths {
                let content = std::fs::read_to_string(path).expect("read SKILL.md");
                black_box(inspect_skill_frontmatter_content(&content));
            }
        }
    });

    print_phase("discovery", loops, &discovery);
    print_phase("frontmatter_parse", parse_loops, &parse);
    print_phase("read_skill_md", loops, &read);
    print_phase("stat_skill_md", loops, &stat);
    print_phase("read_then_parse", loops, &inspect_via_read);

    let discover_mean = mean(&discovery).as_secs_f64();
    let read_mean = mean(&read).as_secs_f64();
    let parse_per_discover = mean(&parse).as_secs_f64() / 64.0;
    let stat_mean = mean(&stat).as_secs_f64();
    let read_parse_mean = mean(&inspect_via_read).as_secs_f64();
    println!(
        "share_of_discovery read={:.1}% parse={:.1}% stat={:.1}% read_then_parse={:.1}% remainder={:.1}%",
        100.0 * read_mean / discover_mean,
        100.0 * parse_per_discover / discover_mean,
        100.0 * stat_mean / discover_mean,
        100.0 * read_parse_mean / discover_mean,
        100.0 * (discover_mean - read_parse_mean).max(0.0) / discover_mean,
    );
}
