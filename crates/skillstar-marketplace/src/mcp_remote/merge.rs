//! Multi-source dedup and merge.
//!
//! Two things collapse here:
//!
//! 1. **Versions within a source.** The official registry publishes one row
//!    per version, so `io.github.acme/tool` shows up many times. We keep the
//!    newest, following the aggregator rules the registry documents: an
//!    explicit `isLatest` wins; otherwise semver beats semver; otherwise
//!    `publishedAt` decides; a valid semver beats an unparseable one.
//! 2. **The same server across sources.** `server.json` `name` is a reverse-DNS
//!    identifier that is globally unique by construction, so it — not the
//!    per-source `id` — is the merge key.
//!
//! The winner supplies the record; every other candidate can only *fill in*
//! fields the winner left empty (plus `stars`, which takes the maximum, and
//! `status`, where a deprecation from any source is kept because under-warning
//! is the worse failure). That is what lets the official registry define the
//! spec while GitHub's mirror contributes stars, license and readme.
//!
//! The merged row's `id` is set to the reverse-DNS name so it is stable across
//! sources and across syncs; per-source ids are opaque and change.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::mcp_models::{McpRegistryServer, McpServerKind, McpServerStatus};

/// One source's contribution to a merge.
#[derive(Debug, Clone)]
pub struct SourceServers {
    pub source_id: String,
    /// Merge authority; lower wins.
    pub priority: i32,
    pub servers: Vec<McpRegistryServer>,
}

#[derive(Debug, Clone)]
struct Candidate {
    priority: i32,
    server: McpRegistryServer,
}

/// Merge every source's servers into one deduplicated catalog, sorted by the
/// reverse-DNS name so the output (and therefore its content hash) is stable.
pub fn merge_catalogs(inputs: Vec<SourceServers>) -> Vec<McpRegistryServer> {
    let mut groups: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for input in inputs {
        for mut server in input.servers {
            if server.registry_source.is_none() {
                server.registry_source = Some(input.source_id.clone());
            }
            if server.contributing_sources.is_empty() {
                server.contributing_sources = vec![input.source_id.clone()];
            }
            groups
                .entry(merge_key(&server))
                .or_default()
                .push(Candidate {
                    priority: input.priority,
                    server,
                });
        }
    }

    groups.into_values().filter_map(merge_group).collect()
}

/// Dedup key: the reverse-DNS `name`, lowercased. Falls back to the source id
/// for rows that somehow lack one (a local directory file may).
fn merge_key(server: &McpRegistryServer) -> String {
    let namespace = server.namespace.trim();
    if namespace.is_empty() {
        server.id.trim().to_lowercase()
    } else {
        namespace.to_lowercase()
    }
}

fn merge_group(mut candidates: Vec<Candidate>) -> Option<McpRegistryServer> {
    if candidates.is_empty() {
        return None;
    }
    // Winner first: newest version, then most authoritative source, then the
    // record that actually carries the most information.
    candidates.sort_by(|a, b| {
        newer_first(&a.server, &b.server)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| completeness(&b.server).cmp(&completeness(&a.server)))
            .then_with(|| a.server.id.cmp(&b.server.id))
    });

    let mut winner = candidates[0].server.clone();
    let winner_version = winner.version.clone();
    let mut sources: Vec<String> = Vec::new();
    for candidate in &candidates {
        for source in candidate
            .server
            .contributing_sources
            .iter()
            .chain(candidate.server.registry_source.iter())
        {
            if !sources.contains(source) {
                sources.push(source.clone());
            }
        }
    }

    for other in candidates.iter().skip(1).map(|c| &c.server) {
        fill_from(&mut winner, other, winner_version.as_deref());
    }

    if !winner.namespace.trim().is_empty() {
        winner.id = winner.namespace.trim().to_string();
    }
    winner.contributing_sources = sources;
    Some(winner)
}

/// Fill gaps in `winner` from `other`. Never overwrites a value the winner
/// already has, with three deliberate exceptions documented inline.
fn fill_from(
    winner: &mut McpRegistryServer,
    other: &McpRegistryServer,
    winner_version: Option<&str>,
) {
    fill_string(&mut winner.description, &other.description);
    fill_string(&mut winner.repo_url, &other.repo_url);
    fill_option(&mut winner.license, &other.license);
    fill_option(&mut winner.version, &other.version);
    fill_option(&mut winner.readme, &other.readme);
    fill_option(&mut winner.updated_at, &other.updated_at);
    fill_option(&mut winner.published_at, &other.published_at);
    fill_option(&mut winner.title, &other.title);
    fill_option(&mut winner.website_url, &other.website_url);
    if winner.icons.is_empty() {
        winner.icons = other.icons.clone();
    }
    if winner.raw_server_json.trim().is_empty() || winner.raw_server_json.trim() == "{}" {
        winner.raw_server_json = other.raw_server_json.clone();
    }

    // 1. Stars are an enrichment signal: take the best number anyone reports.
    winner.stars = winner.stars.max(other.stars);

    // 2. A deprecation from any source is kept — showing a deprecated server
    //    as healthy is worse than the reverse.
    if winner.status == McpServerStatus::Active && other.status != McpServerStatus::Active {
        winner.status = other.status;
    }

    // 3. `isLatest` is per-version. If another source saw the *same* version
    //    and called it latest, believe it.
    if !winner.is_latest && other.is_latest && other.version.as_deref() == winner_version {
        winner.is_latest = true;
    }

    if winner.packages.is_empty() && !other.packages.is_empty() {
        winner.packages = other.packages.clone();
    }
    if winner.remotes.is_empty() && !other.remotes.is_empty() {
        winner.remotes = other.remotes.clone();
    }
    for runtime in &other.runtimes {
        if !runtime.is_empty() && !winner.runtimes.contains(runtime) {
            winner.runtimes.push(runtime.clone());
        }
    }
    winner.kind = kind_for(&winner.packages, &winner.remotes);
}

fn kind_for(
    packages: &[crate::mcp_models::McpRegistryPackageSummary],
    remotes: &[crate::mcp_models::McpRegistryRemoteSummary],
) -> McpServerKind {
    match (packages.is_empty(), remotes.is_empty()) {
        (false, false) => McpServerKind::Both,
        (false, true) => McpServerKind::Stdio,
        (true, false) => McpServerKind::Remote,
        (true, true) => McpServerKind::Unknown,
    }
}

fn fill_string(target: &mut String, source: &str) {
    if target.trim().is_empty() && !source.trim().is_empty() {
        *target = source.to_string();
    }
}

fn fill_option(target: &mut Option<String>, source: &Option<String>) {
    let empty = target.as_deref().map(str::trim).is_none_or(str::is_empty);
    if empty && let Some(value) = source.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        *target = Some(value.to_string());
    }
}

/// How much usable information a record carries — the tiebreak for "same name,
/// same version, equally authoritative".
fn completeness(server: &McpRegistryServer) -> usize {
    let mut score = 0;
    score += usize::from(!server.description.trim().is_empty());
    score += usize::from(!server.repo_url.trim().is_empty());
    score += usize::from(server.license.is_some());
    score += usize::from(server.version.is_some());
    score += usize::from(server.readme.is_some());
    score += usize::from(server.title.is_some());
    score += usize::from(server.website_url.is_some());
    score += usize::from(!server.icons.is_empty());
    score += server.packages.len();
    score += server.remotes.len();
    score += server
        .packages
        .iter()
        .map(|p| p.environment_variables.len() + p.package_arguments.len())
        .sum::<usize>();
    score += server
        .remotes
        .iter()
        .map(|r| r.headers.len())
        .sum::<usize>();
    score
}

/// Ordering that puts the newer release first, per the registry's documented
/// aggregator rules.
pub(crate) fn newer_first(a: &McpRegistryServer, b: &McpRegistryServer) -> Ordering {
    if a.is_latest != b.is_latest {
        // `Less` == sorts first == newer.
        return if a.is_latest {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    match (
        parse_semver(a.version.as_deref()),
        parse_semver(b.version.as_deref()),
    ) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => b
            .published_at
            .as_deref()
            .unwrap_or_default()
            .cmp(a.published_at.as_deref().unwrap_or_default()),
    }
}

/// Minimal semver: `major.minor.patch` with an optional pre-release tag.
///
/// Returned as a sortable tuple where a release outranks its own pre-release
/// (`1.0.0` > `1.0.0-rc.1`), which is the only pre-release rule that matters
/// for "which of these two rows is the newer one".
fn parse_semver(raw: Option<&str>) -> Option<(u64, u64, u64, u8, String)> {
    let raw = raw?.trim().trim_start_matches(['v', 'V']);
    if raw.is_empty() {
        return None;
    }
    let core_end = raw.find(['-', '+']).unwrap_or(raw.len());
    let (core, rest) = raw.split_at(core_end);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let prerelease = rest.strip_prefix('-').unwrap_or_default();
    // 1 = final release, 0 = pre-release, so plain tuple ordering is correct.
    let rank = if prerelease.is_empty() { 1 } else { 0 };
    Some((major, minor, patch, rank, prerelease.to_string()))
}
