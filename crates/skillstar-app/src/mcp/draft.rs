//! `McpRegistryServer` (marketplace) → `McpServerEntry` draft (models).
//!
//! This is the cross-domain mapping the audit (§C.1) called out as living in
//! the command layer against the AGENTS.md rule. It moved here whole, and two
//! things changed on the way:
//!
//! 1. **It reads the structured spec, not the raw JSON.** `McpRegistryServer`
//!    now carries fully parsed `packages[]` / `remotes[]` with `Input`
//!    semantics, so re-parsing `raw_server_json` with hand-rolled key probing
//!    (which knew only `is_secret` / `is_required` and lost `choices`,
//!    `format`, `placeholder`, `variables`) is gone.
//! 2. **It fills the provenance fingerprint.** The four `None` placeholders
//!    W-B left on `McpServerEntry` are real values here — see
//!    [`registry_to_entry_for`].
//!
//! Which shape gets installed is [`super::runtime`]'s decision, not array
//! order.

use std::collections::BTreeMap;

use skillstar_marketplace::{
    McpArgument, McpArgumentKind, McpInput, McpKeyValueInput, McpRegistryPackageSummary,
    McpRegistryRemoteSummary, McpRegistryServer,
};
use skillstar_models::mcp::McpServerEntry;

use super::runtime::{
    CandidateOrigin, McpRuntimeCandidate, McpRuntimeShape, parse_candidate_id, select_runtime,
};

/// Sanitize a registry name into a valid MCP config key. Server keys are
/// written verbatim into every tool's config, so keep them to `[A-Za-z0-9_-]`.
pub(crate) fn sanitize_key(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "mcp-server".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Prefill an `Input`-shaped field.
///
/// Precedence is the schema's own: a publisher-set `value` is not the user's to
/// edit, so it wins even when the field is also marked secret (it is typically
/// a template like `Bearer {TOKEN}` whose *variable* is the secret). Otherwise
/// anything required or secret is deliberately blanked — the form must ask —
/// and only a plain optional field keeps its `default`.
pub(crate) fn prefill(input: &McpInput) -> String {
    if let Some(value) = input.value.as_deref().filter(|v| !v.is_empty()) {
        return value.to_string();
    }
    if input.is_required || input.is_secret {
        return String::new();
    }
    input.default.clone().unwrap_or_default()
}

fn key_values(items: &[McpKeyValueInput]) -> BTreeMap<String, String> {
    items
        .iter()
        .map(|item| (item.name.clone(), prefill(&item.input)))
        .collect()
}

/// Flatten `runtimeArguments[]` / `packageArguments[]` into a command line.
///
/// Named arguments contribute their flag; both kinds contribute a value when
/// the publisher set one. `placeholder` and `valueHint` are deliberately not
/// emitted — they are display hints, and pushing them would put the string
/// "PATH_TO_DIRECTORY" on the real command line.
pub(crate) fn argument_tokens(args: &[McpArgument]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        if arg.kind == McpArgumentKind::Named
            && let Some(name) = arg.name.as_deref().filter(|n| !n.trim().is_empty())
        {
            out.push(name.to_string());
        }
        let value = prefill(&arg.input);
        if !value.is_empty() {
            out.push(value);
        }
    }
    out
}

/// Substitute `{curly_braces}` placeholders that the publisher gave a value or
/// default for. Unresolved placeholders are left in place: the install form
/// collects them, and a half-substituted url is more honest than one with an
/// empty hole in it.
pub(crate) fn resolve_url(url: &str, variables: &[McpKeyValueInput]) -> String {
    let mut out = url.to_string();
    for variable in variables {
        let value = prefill(&variable.input);
        if value.is_empty() {
            continue;
        }
        out = out.replace(&format!("{{{}}}", variable.name), &value);
    }
    out
}

/// Build the `args` a package candidate launches with.
pub(crate) fn package_args(package: &McpRegistryPackageSummary) -> Vec<String> {
    let command = package.runtime.trim();
    let identifier = package.identifier.trim();
    let runtime_args = argument_tokens(&package.runtime_arguments);
    let versioned = |separator: &str| match package.version.as_deref() {
        Some(version) if !version.trim().is_empty() => {
            format!("{identifier}{separator}{}", version.trim())
        }
        _ => identifier.to_string(),
    };

    let mut args = runtime_args.clone();
    match command {
        "npx" | "bunx" => {
            if !args.iter().any(|a| a == "-y" || a == "--yes") {
                args.push("-y".to_string());
            }
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
        "uvx" | "dnx" => {
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
        "docker" => {
            if !runtime_args.iter().any(|a| a == "run") {
                args.extend(["run", "-i", "--rm"].iter().map(|s| (*s).to_string()));
            }
            if !identifier.is_empty() {
                args.push(versioned(":"));
            }
        }
        _ => {
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
    }
    args.extend(argument_tokens(&package.package_arguments));
    args
}

fn fill_stdio(entry: &mut McpServerEntry, package: &McpRegistryPackageSummary) {
    entry.transport = "stdio".to_string();
    let command = package.runtime.trim();
    if !command.is_empty() {
        entry.command = Some(command.to_string());
    }
    entry.args = package_args(package);
    entry.env = key_values(&package.environment_variables);
}

fn fill_remote(entry: &mut McpServerEntry, remote: &McpRegistryRemoteSummary) {
    entry.transport = remote.transport.clone();
    entry.url = Some(resolve_url(&remote.url, &remote.variables));
    entry.headers = key_values(&remote.headers);
}

/// Map a cached registry server into a prefilled `McpServerEntry` draft, using
/// the runtime shape this machine can actually run.
pub fn registry_to_entry(server: &McpRegistryServer) -> McpServerEntry {
    let selection = select_runtime(server);
    let candidate = selection.draft_candidate().cloned();
    registry_to_entry_for(server, candidate.as_ref())
}

/// [`registry_to_entry`] with the shape chosen explicitly — the user's override
/// from the runtime picker.
///
/// This is also where the provenance fingerprint (audit B.1-a) is filled in:
///
/// - `registry_name` — `server.json`'s own reverse-DNS name (`namespace`), the
///   identity the multi-source merge keys on. Never the sanitized config key.
/// - `installed_version` — the chosen package's version, falling back to the
///   server-level version the registry published.
/// - `source_id` — the source this row was primarily built from
///   (`registry_source`), or the curated publisher bucket for SkillStar's own
///   rows, which have no remote source.
/// - `runtime_kind` — the shape actually selected, not a guess.
///
/// A field stays `None` only when the registry genuinely did not say.
pub fn registry_to_entry_for(
    server: &McpRegistryServer,
    candidate: Option<&McpRuntimeCandidate>,
) -> McpServerEntry {
    let mut entry = McpServerEntry {
        id: String::new(),
        name: sanitize_key(&server.name),
        transport: "stdio".to_string(),
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        headers: BTreeMap::new(),
        description: (!server.description.is_empty()).then(|| server.description.clone()),
        homepage: (!server.repo_url.is_empty()).then(|| server.repo_url.clone()),
        tags: Vec::new(),
        registry_name: (!server.namespace.trim().is_empty()).then(|| server.namespace.clone()),
        source_id: server
            .registry_source
            .clone()
            .or_else(|| server.source.clone()),
        installed_version: None,
        runtime_kind: None,
        enabled: BTreeMap::new(),
        auto_approve_all: false,
        auto_approve_tools: Vec::new(),
        disabled_tools: Vec::new(),
        timeout_ms: None,
        sort_index: 0,
        created_at: None,
        updated_at: None,
    };

    let Some(candidate) = candidate else {
        // Nothing this server offers can run here. The draft is still returned
        // (the form is editable, and the user may know something we don't), but
        // claiming a `runtime_kind` we never selected would be a lie.
        entry.installed_version = server.version.clone();
        return entry;
    };

    entry.runtime_kind = Some(candidate.shape.as_runtime_kind().as_str().to_string());
    entry.installed_version = candidate
        .version
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| server.version.clone());

    match parse_candidate_id(&candidate.id) {
        Some(CandidateOrigin::Remote(index)) => {
            if let Some(remote) = server.remotes.get(index) {
                fill_remote(&mut entry, remote);
            }
        }
        Some(CandidateOrigin::Package(index)) => {
            if let Some(package) = server.packages.get(index) {
                fill_stdio(&mut entry, package);
            }
        }
        None => {}
    }

    // The picker ranks SSE below streamable-http but still offers it; make the
    // deprecation visible on the installed entry too, not only in the picker.
    if candidate.shape == McpRuntimeShape::RemoteSse {
        entry.tags.push("deprecated-transport".to_string());
    }

    entry
}
