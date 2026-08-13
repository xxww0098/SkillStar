//! Runtime shape selection: which of a registry server's `remotes[]` /
//! `packages[]` should actually be installed on *this* machine.
//!
//! The previous behaviour was "take `packages[0]`, else `remotes[0]`" — array
//! order decided what ran, which is neither a safety nor an availability
//! judgement (audit C.1). The ordering below follows
//! `docs/others/mcp-modern-design-research.md` §6.4:
//!
//! | rank | shape | why it ranks there |
//! | --- | --- | --- |
//! | 0 | `remotes[]` `streamable-http` | no toolchain, no local code execution, standard OAuth |
//! | 1 | `remotes[]` `sse` | works, but the transport is deprecated — always flagged |
//! | 2 | `packages[]` `oci` | local, but container-isolated: the safest local shape |
//! | 3 | `packages[]` `mcpb` | prebuilt bundle; the client (not the registry) must verify `fileSha256` |
//! | 4 | `packages[]` npm / pypi / nuget / cargo | needs the matching toolchain |
//!
//! Rank alone is not the answer, because `runtimeHint` is a *hint*: a machine
//! without Docker cannot run the OCI package no matter how well it ranks. Every
//! stdio candidate is therefore checked against the real `PATH` (through
//! `skillstar_models::mcp::resolve_runtime`, the same resolver that later
//! launches the process — so the check and the launch can never disagree), and
//! an unavailable candidate sorts below every available one.
//!
//! The selector returns **every** candidate with its rank, warnings and
//! availability, plus the id of the recommended one. Nothing here decides for
//! the user: the recommendation is a default, and any candidate id can be
//! passed back to override it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use skillstar_marketplace::{McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer};
use skillstar_models::mcp::{McpRuntimeKind, resolve_runtime};
use ts_rs::TS;

/// Which `server.json` shape a candidate came from. Mirrors
/// [`McpRuntimeKind`]'s vocabulary (minus `Manual`, which is not a registry
/// shape) so the value stored on an installed entry and the value shown in the
/// picker are the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpRuntimeShape.ts")]
pub enum McpRuntimeShape {
    RemoteStreamableHttp,
    RemoteSse,
    PackageOci,
    PackageMcpb,
    /// A language-registry package: npm, pypi, nuget, cargo, or anything else
    /// the registry starts publishing. `registryType` carries the detail.
    PackagePlain,
}

impl McpRuntimeShape {
    /// Preference rank; lower wins. See the module docs for the rationale.
    pub fn rank(self) -> u32 {
        match self {
            Self::RemoteStreamableHttp => 0,
            Self::RemoteSse => 1,
            Self::PackageOci => 2,
            Self::PackageMcpb => 3,
            Self::PackagePlain => 4,
        }
    }

    /// The provenance token written to `McpServerEntry::runtime_kind`.
    pub fn as_runtime_kind(self) -> McpRuntimeKind {
        match self {
            Self::RemoteStreamableHttp => McpRuntimeKind::RemoteStreamableHttp,
            Self::RemoteSse => McpRuntimeKind::RemoteSse,
            Self::PackageOci => McpRuntimeKind::PackageOci,
            Self::PackageMcpb => McpRuntimeKind::PackageMcpb,
            Self::PackagePlain => McpRuntimeKind::PackagePlain,
        }
    }

    fn transport(self) -> &'static str {
        match self {
            Self::RemoteStreamableHttp => "http",
            Self::RemoteSse => "sse",
            _ => "stdio",
        }
    }
}

/// One installable shape of a registry server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpRuntimeCandidate.ts")]
pub struct McpRuntimeCandidate {
    /// Stable handle (`remote:0`, `package:2`) the UI passes back to override
    /// the recommendation. Encodes the array this candidate came from and its
    /// index there, so it stays valid for as long as the snapshot row does.
    pub id: String,
    pub shape: McpRuntimeShape,
    /// Transport the installed entry will use: `http`, `sse` or `stdio`.
    pub transport: String,
    /// `npm` / `pypi` / `oci` / `mcpb` / … verbatim from the registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_type: Option<String>,
    /// Package identifier, for package candidates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Endpoint, for remote candidates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Launcher this candidate needs (`npx`, `uvx`, `docker`, …). `None` for
    /// remote candidates, which need nothing installed locally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_command: Option<String>,
    /// Whether that launcher was found on this machine's `PATH`. `None` when
    /// there is nothing to look for (remote candidates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_available: Option<bool>,
    /// Preference rank — see [`McpRuntimeShape::rank`]. Exposed so the UI can
    /// explain *why* one candidate is recommended over another.
    pub rank: u32,
    /// Can SkillStar install this candidate as-is? `false` never means "broken
    /// server" — it means this particular shape is unusable here, and
    /// [`Self::blocked_reason`] says why.
    pub installable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    /// Non-blocking caveats the UI must show (deprecated transport, unverified
    /// bundle hash, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl McpRuntimeCandidate {
    /// Can a prefilled draft be built from this shape at all?
    ///
    /// A missing local toolchain is a property of *this machine*, not of the
    /// shape: the entry it produces is correct and will start as soon as the
    /// user installs the runtime, so the form should still be prefilled. Every
    /// other block means the shape cannot be expressed as a launch spec at all
    /// — an MCPB bundle SkillStar cannot download, a cargo crate with no
    /// one-shot runner, a package with no runner command — and prefilling from
    /// it would produce a command line that is simply wrong.
    pub fn is_draftable(&self) -> bool {
        self.installable || self.runtime_available == Some(false)
    }
}

/// Every shape a server offers, ranked, with the default pick.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpRuntimeSelection.ts")]
pub struct McpRuntimeSelection {
    pub server_id: String,
    /// Best first. Installable candidates always precede unusable ones.
    pub candidates: Vec<McpRuntimeCandidate>,
    /// [`McpRuntimeCandidate::id`] of the default pick, or `None` when nothing
    /// this server offers can run here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_id: Option<String>,
}

impl McpRuntimeSelection {
    pub fn candidate(&self, id: &str) -> Option<&McpRuntimeCandidate> {
        self.candidates.iter().find(|c| c.id == id)
    }

    /// The shape a prefilled draft should be built from: the recommendation
    /// when there is one, otherwise the best-ranked shape that can still be
    /// expressed as a launch spec.
    ///
    /// Falling back matters because "nothing is installable" is usually "this
    /// machine has no `npx` yet", and answering that with a blank form helps
    /// nobody. The install plan is where the blocker is *stated*; the draft is
    /// where it is *fillable*.
    pub fn draft_candidate(&self) -> Option<&McpRuntimeCandidate> {
        self.recommended_id
            .as_deref()
            .and_then(|id| self.candidate(id))
            .or_else(|| self.candidates.iter().find(|c| c.is_draftable()))
    }

    /// The candidate `id` resolves to, falling back to
    /// [`Self::draft_candidate`] when `id` is absent or unknown.
    pub fn resolve(&self, id: Option<&str>) -> Option<&McpRuntimeCandidate> {
        id.and_then(|id| self.candidate(id))
            .or_else(|| self.draft_candidate())
    }
}

/// Where a candidate came from, recovered from its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateOrigin {
    Remote(usize),
    Package(usize),
}

pub(crate) fn parse_candidate_id(id: &str) -> Option<CandidateOrigin> {
    let (kind, index) = id.split_once(':')?;
    let index: usize = index.parse().ok()?;
    match kind {
        "remote" => Some(CandidateOrigin::Remote(index)),
        "package" => Some(CandidateOrigin::Package(index)),
        _ => None,
    }
}

/// Rank every shape `server` offers against this machine's `PATH`.
pub fn select_runtime(server: &McpRegistryServer) -> McpRuntimeSelection {
    // One resolution per distinct launcher: `resolve_runtime` walks `PATH`,
    // and a server with six npm packages would otherwise walk it six times.
    let mut cache: HashMap<String, bool> = HashMap::new();
    select_runtime_with(server, &mut |command| {
        *cache
            .entry(command.to_string())
            .or_insert_with(|| resolve_runtime(command).is_ok())
    })
}

/// [`select_runtime`] with the `PATH` lookup injected, so the ranking rules are
/// testable without depending on what happens to be installed.
pub fn select_runtime_with(
    server: &McpRegistryServer,
    runtime_available: &mut dyn FnMut(&str) -> bool,
) -> McpRuntimeSelection {
    let mut candidates: Vec<McpRuntimeCandidate> = Vec::new();
    for (index, remote) in server.remotes.iter().enumerate() {
        candidates.push(remote_candidate(index, remote));
    }
    for (index, package) in server.packages.iter().enumerate() {
        candidates.push(package_candidate(index, package, runtime_available));
    }

    // Availability outranks preference: a well-ranked shape whose toolchain is
    // missing is not a better default than one that actually runs.
    candidates.sort_by_key(|c| (!c.installable, c.rank));
    let recommended_id = candidates
        .iter()
        .find(|c| c.installable)
        .map(|c| c.id.clone());

    McpRuntimeSelection {
        server_id: server.id.clone(),
        candidates,
        recommended_id,
    }
}

fn remote_candidate(index: usize, remote: &McpRegistryRemoteSummary) -> McpRuntimeCandidate {
    let shape = if remote.transport == "sse" {
        McpRuntimeShape::RemoteSse
    } else {
        McpRuntimeShape::RemoteStreamableHttp
    };
    let mut warnings = Vec::new();
    if shape == McpRuntimeShape::RemoteSse {
        warnings.push(
            "The SSE transport is deprecated. Prefer a streamable-http endpoint when the publisher offers one."
                .to_string(),
        );
    }
    McpRuntimeCandidate {
        id: format!("remote:{index}"),
        shape,
        transport: shape.transport().to_string(),
        registry_type: None,
        identifier: None,
        version: None,
        url: Some(remote.url.clone()),
        runtime_command: None,
        runtime_available: None,
        rank: shape.rank(),
        installable: !remote.url.trim().is_empty(),
        blocked_reason: remote
            .url
            .trim()
            .is_empty()
            .then(|| "This remote entry declares no url.".to_string()),
        warnings,
    }
}

fn package_candidate(
    index: usize,
    package: &McpRegistryPackageSummary,
    runtime_available: &mut dyn FnMut(&str) -> bool,
) -> McpRuntimeCandidate {
    let registry_type = package
        .registry_type
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let shape = match registry_type.as_str() {
        "oci" | "docker" => McpRuntimeShape::PackageOci,
        "mcpb" => McpRuntimeShape::PackageMcpb,
        _ => McpRuntimeShape::PackagePlain,
    };

    let mut warnings = Vec::new();
    // A package may declare an HTTP transport of its own — the process is still
    // launched locally, but it then speaks HTTP on a port instead of stdio.
    // SkillStar's store has no way to express "launch this, then connect
    // there", so say so rather than silently installing a broken entry.
    if let Some(transport) = &package.transport
        && !transport.transport_type.eq_ignore_ascii_case("stdio")
        && !transport.transport_type.trim().is_empty()
    {
        warnings.push(format!(
            "This package declares a '{}' transport; SkillStar launches local packages over stdio.",
            transport.transport_type
        ));
    }

    let command = package.runtime.trim().to_string();
    let mut runtime_command = (!command.is_empty()).then(|| command.clone());
    let mut available = None;
    let mut blocked_reason = None;

    match shape {
        McpRuntimeShape::PackageMcpb => {
            // MCPB is a downloadable bundle, not something a runner launches.
            // The registry explicitly does not verify `fileSha256` — the client
            // must — and SkillStar has no download-and-verify step yet, so this
            // shape is listed (the user should see it exists) but not offered.
            runtime_command = None;
            blocked_reason = Some(
                "MCPB bundles must be downloaded and checked against fileSha256 before they can run; SkillStar has no bundle installer yet."
                    .to_string(),
            );
            if package.file_sha256.is_none() {
                warnings.push(
                    "The publisher declared no fileSha256 for this bundle, so its download could not be verified even manually."
                        .to_string(),
                );
            }
        }
        _ if command.is_empty() => {
            blocked_reason =
                Some("This package declares no runner command or runtimeHint.".to_string());
        }
        // `cargo install` is a persistent install followed by invoking the
        // produced binary by name — there is no `npx`-style one-shot runner, so
        // cargo rows normally ship without a `runtimeHint` and `cargo <crate>`
        // would simply not be a valid command line.
        _ if registry_type == "cargo" && package.runtime_hint.is_none() => {
            blocked_reason = Some(
                "cargo has no one-shot runner like npx. Run `cargo install` yourself, then add the resulting binary as a manual stdio server."
                    .to_string(),
            );
        }
        _ => {
            let found = runtime_available(&command);
            available = Some(found);
            if !found {
                blocked_reason = Some(format!(
                    "'{command}' was not found on this machine. Install the toolchain it belongs to, or pick another runtime shape."
                ));
            }
        }
    }

    McpRuntimeCandidate {
        id: format!("package:{index}"),
        shape,
        transport: shape.transport().to_string(),
        registry_type: package.registry_type.clone(),
        identifier: (!package.identifier.trim().is_empty()).then(|| package.identifier.clone()),
        version: package.version.clone(),
        url: None,
        runtime_command,
        runtime_available: available,
        rank: shape.rank(),
        installable: blocked_reason.is_none(),
        blocked_reason,
        warnings,
    }
}
