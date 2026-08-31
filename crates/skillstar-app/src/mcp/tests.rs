//! Unit tests for the MCP cross-domain use cases.
//!
//! The `PATH` lookup is injected everywhere (`select_runtime_with` /
//! `build_install_plan_with`), so the ranking rules are pinned against a
//! scripted machine rather than against whatever happens to be installed on the
//! runner. Nothing here spawns a process, opens a socket, or writes a file.

use std::collections::BTreeMap;

use skillstar_marketplace::{
    McpArgument, McpArgumentKind, McpInput, McpInputFormat, McpInputVariable, McpKeyValueInput,
    McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
    McpServerStatus, McpTransportSpec,
};

use super::draft::{registry_to_entry_for, sanitize_key};
use super::install::{McpInstallInputScope, McpSecretStorage, build_install_plan_with};
use super::presets::{curated_server_to_preset, load_curated_servers, list_mcp_presets_with};
use super::runtime::{McpRuntimeShape, select_runtime_with};

mod install_preview_tests;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn server(name: &str) -> McpRegistryServer {
    McpRegistryServer {
        id: format!("{name}-id"),
        name: name.into(),
        namespace: format!("io.github.acme/{name}"),
        description: "desc".into(),
        repo_url: "https://github.com/acme/x".into(),
        kind: McpServerKind::Unknown,
        version: Some("9.9.9".into()),
        registry_source: Some("official".into()),
        ..Default::default()
    }
}

fn package(registry_type: &str, runtime: &str, identifier: &str) -> McpRegistryPackageSummary {
    McpRegistryPackageSummary {
        runtime: runtime.into(),
        identifier: identifier.into(),
        version: Some("1.2.0".into()),
        registry_type: Some(registry_type.into()),
        ..Default::default()
    }
}

fn remote(transport: &str, url: &str) -> McpRegistryRemoteSummary {
    McpRegistryRemoteSummary {
        transport: transport.into(),
        url: url.into(),
        transport_type: Some(if transport == "sse" {
            "sse".into()
        } else {
            "streamable-http".into()
        }),
        ..Default::default()
    }
}

fn env(name: &str, input: McpInput) -> McpKeyValueInput {
    McpKeyValueInput {
        name: name.into(),
        input,
    }
}

fn secret_input() -> McpInput {
    McpInput {
        is_required: true,
        is_secret: true,
        ..Default::default()
    }
}

/// A machine where every named command exists.
fn everything_installed(_: &str) -> bool {
    true
}

// ---------------------------------------------------------------------------
// Runtime shape selection (research §6.4)
// ---------------------------------------------------------------------------

#[test]
fn ranks_streamable_http_above_sse_above_oci_above_mcpb_above_plain_packages() {
    let mut s = server("ranked");
    // Deliberately published worst-first: array order must not decide.
    s.packages = vec![
        package("npm", "npx", "@acme/x"),
        package("mcpb", "mcpb", "acme.mcpb"),
        package("oci", "docker", "acme/x"),
    ];
    s.remotes = vec![
        remote("sse", "https://acme.dev/sse"),
        remote("http", "https://acme.dev/mcp"),
    ];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let shapes: Vec<McpRuntimeShape> = selection.candidates.iter().map(|c| c.shape).collect();
    assert_eq!(
        shapes,
        vec![
            McpRuntimeShape::RemoteStreamableHttp,
            McpRuntimeShape::RemoteSse,
            McpRuntimeShape::PackageOci,
            McpRuntimeShape::PackagePlain,
            // MCPB has no installer, so it sorts behind every usable shape
            // regardless of its rank.
            McpRuntimeShape::PackageMcpb,
        ]
    );
    assert_eq!(selection.recommended_id.as_deref(), Some("remote:1"));
}

#[test]
fn sse_is_offered_but_always_flagged_as_deprecated() {
    let mut s = server("sse-only");
    s.remotes = vec![remote("sse", "https://acme.dev/sse")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let candidate = &selection.candidates[0];
    assert!(candidate.installable);
    assert!(
        candidate.warnings.iter().any(|w| w.contains("deprecated")),
        "SSE must carry a deprecation warning: {:?}",
        candidate.warnings
    );

    let entry = registry_to_entry_for(&s, Some(candidate));
    assert_eq!(entry.transport, "sse");
    assert!(entry.tags.iter().any(|t| t == "deprecated-transport"));
}

#[test]
fn a_missing_local_toolchain_loses_to_one_that_is_installed() {
    let mut s = server("mixed");
    // OCI outranks npm on paper, but this machine has no docker.
    s.packages = vec![
        package("oci", "docker", "acme/x"),
        package("npm", "npx", "@acme/x"),
    ];

    let selection = select_runtime_with(&s, &mut |command| command == "npx");
    assert_eq!(selection.recommended_id.as_deref(), Some("package:1"));

    let docker = selection
        .candidate("package:0")
        .expect("oci candidate listed");
    assert_eq!(docker.runtime_available, Some(false));
    assert!(!docker.installable);
    assert!(docker.blocked_reason.as_deref().unwrap().contains("docker"));
}

#[test]
fn remote_candidates_never_claim_a_local_runtime() {
    let mut s = server("remote");
    s.remotes = vec![remote("http", "https://acme.dev/mcp")];

    let selection = select_runtime_with(&s, &mut |_| panic!("remote must not probe PATH"));
    let candidate = &selection.candidates[0];
    assert_eq!(candidate.runtime_command, None);
    assert_eq!(candidate.runtime_available, None);
}

#[test]
fn mcpb_is_listed_but_blocked_and_flags_a_missing_hash() {
    let mut s = server("bundle");
    s.packages = vec![package("mcpb", "mcpb", "acme.mcpb")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let candidate = &selection.candidates[0];
    assert!(!candidate.installable);
    assert!(
        candidate
            .blocked_reason
            .as_deref()
            .unwrap()
            .contains("fileSha256")
    );
    assert!(
        candidate.warnings.iter().any(|w| w.contains("fileSha256")),
        "a bundle without a declared hash must say so"
    );
    assert_eq!(selection.recommended_id, None);
}

#[test]
fn mcpb_with_a_declared_hash_drops_the_missing_hash_warning() {
    let mut s = server("bundle");
    let mut pkg = package("mcpb", "mcpb", "acme.mcpb");
    pkg.file_sha256 = Some("a".repeat(64));
    s.packages = vec![pkg];

    let selection = select_runtime_with(&s, &mut everything_installed);
    assert!(selection.candidates[0].warnings.is_empty());
}

#[test]
fn cargo_without_a_runtime_hint_is_blocked_because_there_is_no_one_shot_runner() {
    let mut s = server("crate");
    s.packages = vec![package("cargo", "cargo", "acme-mcp")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let candidate = &selection.candidates[0];
    assert!(!candidate.installable);
    assert!(
        candidate
            .blocked_reason
            .as_deref()
            .unwrap()
            .contains("cargo install")
    );
}

#[test]
fn cargo_with_a_runtime_hint_is_installable_through_that_hint() {
    let mut s = server("crate");
    let mut pkg = package("cargo", "acme-mcp", "acme-mcp");
    pkg.runtime_hint = Some("acme-mcp".into());
    s.packages = vec![pkg];

    let selection = select_runtime_with(&s, &mut |command| command == "acme-mcp");
    assert!(selection.candidates[0].installable);
}

#[test]
fn a_package_declaring_a_non_stdio_transport_is_flagged() {
    let mut s = server("http-package");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.transport = Some(McpTransportSpec {
        transport_type: "streamable-http".into(),
        url: Some("http://127.0.0.1:8080/mcp".into()),
        headers: Vec::new(),
    });
    s.packages = vec![pkg];

    let selection = select_runtime_with(&s, &mut everything_installed);
    assert!(
        selection.candidates[0]
            .warnings
            .iter()
            .any(|w| w.contains("stdio")),
        "a package that speaks HTTP must not install silently as stdio"
    );
}

#[test]
fn a_server_with_no_shapes_recommends_nothing() {
    let selection = select_runtime_with(&server("empty"), &mut everything_installed);
    assert!(selection.candidates.is_empty());
    assert_eq!(selection.recommended_id, None);
}

// ---------------------------------------------------------------------------
// Draft mapping + provenance fingerprint (audit B.1-a / C.1)
// ---------------------------------------------------------------------------

#[test]
fn npm_package_becomes_an_npx_stdio_entry() {
    let mut s = server("filesystem");
    let mut pkg = package("npm", "npx", "@modelcontextprotocol/server-filesystem");
    pkg.environment_variables = vec![
        env(
            "ROOT",
            McpInput {
                default: Some("/tmp".into()),
                ..Default::default()
            },
        ),
        env("API_KEY", secret_input()),
    ];
    s.packages = vec![pkg];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));

    assert_eq!(entry.transport, "stdio");
    assert_eq!(entry.command.as_deref(), Some("npx"));
    assert_eq!(
        entry.args,
        vec!["-y", "@modelcontextprotocol/server-filesystem@1.2.0"]
    );
    assert_eq!(entry.env.get("ROOT").map(String::as_str), Some("/tmp"));
    // The form must ask for the secret, so the draft carries no value for it —
    // and carries no *row* for it either: an empty string here would be pinned
    // into every tool's config, and would make the plan's draft disagree with
    // the answered preview, which drops blanks.
    assert!(
        !entry.env.contains_key("API_KEY"),
        "a value nobody supplied must be left out, not written blank: {:?}",
        entry.env
    );
}

#[test]
fn oci_package_becomes_a_docker_run_entry() {
    let mut s = server("everything");
    s.packages = vec![package("oci", "docker", "mcp/everything")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));
    assert_eq!(entry.command.as_deref(), Some("docker"));
    assert_eq!(
        entry.args,
        vec!["run", "-i", "--rm", "mcp/everything:1.2.0"]
    );
}

#[test]
fn remote_headers_keep_their_template_and_url_variables_are_substituted() {
    let mut s = server("netdata");
    let mut r = remote("http", "https://{region}.netdata.cloud/api/v1/mcp");
    r.headers = vec![env(
        "Authorization",
        McpInput {
            value: Some("Bearer {TOKEN}".into()),
            is_secret: true,
            ..Default::default()
        },
    )];
    r.variables = vec![env(
        "region",
        McpInput {
            default: Some("eu".into()),
            ..Default::default()
        },
    )];
    s.remotes = vec![r];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));
    assert_eq!(entry.transport, "http");
    assert_eq!(
        entry.url.as_deref(),
        Some("https://eu.netdata.cloud/api/v1/mcp")
    );
    // A publisher-set `value` survives even on a secret field: it is the
    // format, and its `{TOKEN}` hole is what the user fills.
    assert_eq!(
        entry.headers.get("Authorization").map(String::as_str),
        Some("Bearer {TOKEN}")
    );
    assert!(entry.command.is_none());
}

#[test]
fn the_provenance_fingerprint_is_filled_from_the_registry_row() {
    let mut s = server("provenance");
    s.packages = vec![package("npm", "npx", "@acme/x")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));

    assert_eq!(
        entry.registry_name.as_deref(),
        Some("io.github.acme/provenance")
    );
    assert_eq!(entry.source_id.as_deref(), Some("official"));
    // The chosen package's version, not the server-level one.
    assert_eq!(entry.installed_version.as_deref(), Some("1.2.0"));
    assert_eq!(entry.runtime_kind.as_deref(), Some("package-plain"));
    // The config key stays sanitized and distinct from the registry name.
    assert_eq!(entry.name, "provenance");
}

#[test]
fn a_curated_row_without_a_registry_source_falls_back_to_its_publisher_bucket() {
    let mut s = server("curated");
    s.registry_source = None;
    s.source = Some("bigmodel".into());
    s.packages = vec![package("npm", "npx", "@z_ai/mcp-server")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));
    assert_eq!(entry.source_id.as_deref(), Some("bigmodel"));
}

#[test]
fn a_server_with_no_runnable_shape_claims_no_runtime_kind() {
    let mut s = server("unrunnable");
    s.packages = vec![package("mcpb", "mcpb", "acme.mcpb")];

    let selection = select_runtime_with(&s, &mut everything_installed);
    assert_eq!(selection.draft_candidate().map(|c| c.id.as_str()), None);
    let entry = registry_to_entry_for(&s, selection.draft_candidate());
    assert_eq!(entry.runtime_kind, None);
    assert_eq!(entry.command, None);
    // The server-level version is still recorded — it is what the registry said.
    assert_eq!(entry.installed_version.as_deref(), Some("9.9.9"));
}

#[test]
fn named_arguments_contribute_their_flag_and_hints_never_reach_the_command_line() {
    let mut s = server("args");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.package_arguments = vec![
        McpArgument {
            kind: McpArgumentKind::Named,
            name: Some("--port".into()),
            input: McpInput {
                default: Some("8080".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        McpArgument {
            kind: McpArgumentKind::Positional,
            value_hint: Some("PATH_TO_DIRECTORY".into()),
            input: McpInput {
                is_required: true,
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    s.packages = vec![pkg];

    let selection = select_runtime_with(&s, &mut everything_installed);
    let entry = registry_to_entry_for(&s, selection.resolve(None));
    assert_eq!(entry.args, vec!["-y", "@acme/x@1.2.0", "--port", "8080"]);
    assert!(
        !entry.args.iter().any(|a| a == "PATH_TO_DIRECTORY"),
        "a value hint is a label, not a value"
    );
}

#[test]
fn sanitizes_config_keys() {
    assert_eq!(sanitize_key("mcp-server"), "mcp-server");
    assert_eq!(sanitize_key("foo.bar baz"), "foo-bar-baz");
    assert_eq!(sanitize_key("--"), "mcp-server");
}

// ---------------------------------------------------------------------------
// Install plan (research §7 P1-6 / P0-4)
// ---------------------------------------------------------------------------

#[test]
fn the_command_preview_is_complete_and_never_shell_executed() {
    let mut s = server("preview");
    let mut pkg = package("npm", "npx", "@acme/server");
    pkg.package_arguments = vec![McpArgument {
        kind: McpArgumentKind::Positional,
        input: McpInput {
            value: Some("/Users/me/My Documents".into()),
            ..Default::default()
        },
        ..Default::default()
    }];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    assert!(!plan.uses_shell);
    let preview = plan.command_preview.expect("stdio installs must preview");
    // Every argument appears, untruncated, with whitespace made visible.
    assert_eq!(
        preview,
        "npx -y @acme/server@1.2.0 '/Users/me/My Documents'"
    );
    assert_eq!(
        plan.args.last().map(String::as_str),
        Some("/Users/me/My Documents")
    );
}

#[test]
fn the_install_plan_carries_full_input_semantics_for_the_form() {
    let mut s = server("inputs");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![
        env("TOKEN", secret_input()),
        env(
            "MODE",
            McpInput {
                is_required: true,
                choices: vec!["fast".into(), "safe".into()],
                ..Default::default()
            },
        ),
        env(
            "DATA_DIR",
            McpInput {
                format: McpInputFormat::Filepath,
                default: Some("/tmp".into()),
                ..Default::default()
            },
        ),
    ];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let by_key = |key: &str| {
        plan.inputs
            .iter()
            .find(|i| i.key == key)
            .unwrap_or_else(|| panic!("{key} missing from the install plan"))
    };

    let token = by_key("TOKEN");
    assert_eq!(token.scope, McpInstallInputScope::Environment);
    assert!(token.input.is_secret && token.must_ask);
    assert_eq!(token.prefilled, "");

    let mode = by_key("MODE");
    assert_eq!(mode.input.choices, vec!["fast", "safe"]);

    let data_dir = by_key("DATA_DIR");
    assert_eq!(data_dir.input.format, McpInputFormat::Filepath);
    assert_eq!(data_dir.prefilled, "/tmp");
    assert!(!data_dir.must_ask);

    assert_eq!(plan.secret_policy.secret_keys, vec!["TOKEN".to_string()]);
    assert_eq!(
        plan.secret_policy.storage,
        McpSecretStorage::UserLevelConfig
    );
}

/// Two positional arguments the publisher gave neither a name nor a
/// `valueHint` both fall back to the key `"argument"`. Their ordinal within the
/// scope is what the form must address them by — without it, filling the first
/// one also fills the second.
#[test]
fn two_positional_arguments_without_a_value_hint_stay_separately_addressable() {
    let mut s = server("positionals");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.package_arguments = vec![
        McpArgument {
            kind: McpArgumentKind::Positional,
            input: McpInput {
                is_required: true,
                description: Some("source".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        McpArgument {
            kind: McpArgumentKind::Positional,
            input: McpInput {
                is_required: true,
                description: Some("destination".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let positionals: Vec<_> = plan
        .inputs
        .iter()
        .filter(|i| i.scope == McpInstallInputScope::PackageArgument)
        .collect();

    assert_eq!(positionals.len(), 2);
    // The key collides — that is exactly the point.
    assert_eq!(positionals[0].key, "argument");
    assert_eq!(positionals[1].key, "argument");
    // The ordinal does not, so `(scope, index)` still names one of the two.
    assert_eq!(
        (positionals[0].scope, positionals[0].index),
        (McpInstallInputScope::PackageArgument, 0)
    );
    assert_eq!(
        (positionals[1].scope, positionals[1].index),
        (McpInstallInputScope::PackageArgument, 1)
    );
    assert_eq!(
        positionals[1].input.description.as_deref(),
        Some("destination")
    );
}

/// The ordinal counts within a scope, not across the whole plan, so a form that
/// groups by scope can index straight into its own section.
#[test]
fn the_ordinal_restarts_at_zero_for_every_scope() {
    let mut s = server("ordinals");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("A", McpInput::default()), env("B", McpInput::default())];
    pkg.runtime_arguments = vec![McpArgument {
        kind: McpArgumentKind::Named,
        name: Some("--verbose".into()),
        ..Default::default()
    }];
    pkg.package_arguments = vec![McpArgument {
        kind: McpArgumentKind::Named,
        name: Some("--port".into()),
        ..Default::default()
    }];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let numbering: Vec<_> = plan
        .inputs
        .iter()
        .map(|i| (i.scope, i.index, i.key.as_str()))
        .collect();

    assert_eq!(
        numbering,
        vec![
            (McpInstallInputScope::Environment, 0, "A"),
            (McpInstallInputScope::Environment, 1, "B"),
            (McpInstallInputScope::RuntimeArgument, 0, "--verbose"),
            (McpInstallInputScope::PackageArgument, 0, "--port"),
        ]
    );
}

/// A publisher-pinned `value` is not the user's to edit, but the
/// `{curly_braces}` inside it are. The plan seeds that sub-form so the frontend
/// never scans the template itself.
#[test]
fn a_pinned_template_ships_its_variables_seeded_with_full_semantics() {
    let mut s = server("template");
    let mut rem = remote("http", "https://acme.dev/mcp");
    rem.headers = vec![McpKeyValueInput {
        name: "Authorization".into(),
        input: McpInput {
            value: Some("{SCHEME} {TOKEN} @{REGION} {TOKEN}".into()),
            variables: BTreeMap::from([
                (
                    "TOKEN".to_string(),
                    McpInputVariable {
                        is_required: true,
                        is_secret: true,
                        ..Default::default()
                    },
                ),
                (
                    "REGION".to_string(),
                    McpInputVariable {
                        default: Some("us".into()),
                        choices: vec!["us".into(), "eu".into()],
                        ..Default::default()
                    },
                ),
                ("UNUSED".to_string(), McpInputVariable::default()),
            ]),
            ..Default::default()
        },
    }];
    s.remotes = vec![rem];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let header = plan
        .inputs
        .iter()
        .find(|i| i.scope == McpInstallInputScope::Header)
        .expect("the header is an input");

    // Template order, de-duplicated, and a variable the template never mentions
    // gets no field.
    assert_eq!(
        header
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        vec!["SCHEME", "TOKEN", "REGION"]
    );

    // An undeclared token still needs a value, so it is reported as required.
    let scheme = &header.variables[0];
    assert!(scheme.variable.is_required && !scheme.variable.is_secret);
    assert_eq!(scheme.variable.format, McpInputFormat::String);
    assert_eq!(scheme.prefilled, "");

    // A secret is never seeded — the form has to ask.
    let token = &header.variables[1];
    assert!(token.variable.is_secret);
    assert_eq!(token.prefilled, "");

    // An optional variable keeps the publisher's default and its closed set.
    let region = &header.variables[2];
    assert_eq!(region.variable.choices, vec!["us", "eu"]);
    assert_eq!(region.prefilled, "us");
}

/// Variables resolve exactly one level: `McpInputVariable` has no `value` of
/// its own, so nothing recurses even when a seeded value itself looks like a
/// template.
#[test]
fn variables_resolve_only_one_level_deep() {
    let mut s = server("one-level");
    let mut rem = remote("http", "https://acme.dev/mcp");
    rem.headers = vec![McpKeyValueInput {
        name: "Authorization".into(),
        input: McpInput {
            value: Some("Bearer {OUTER}".into()),
            variables: BTreeMap::from([(
                "OUTER".to_string(),
                McpInputVariable {
                    default: Some("{INNER}".into()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        },
    }];
    s.remotes = vec![rem];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let header = plan
        .inputs
        .iter()
        .find(|i| i.scope == McpInstallInputScope::Header)
        .expect("the header is an input");

    assert_eq!(header.variables.len(), 1);
    assert_eq!(header.variables[0].prefilled, "{INNER}");
}

/// An input with no pinned `value` has nothing to substitute, so it carries no
/// variables even when the publisher declared some.
#[test]
fn an_input_without_a_template_carries_no_variables() {
    let mut s = server("no-template");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env(
        "TOKEN",
        McpInput {
            is_required: true,
            variables: BTreeMap::from([("STRAY".to_string(), McpInputVariable::default())]),
            ..Default::default()
        },
    )];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    assert!(plan.inputs[0].variables.is_empty());
}

/// Pins the claim `McpSecretPolicy`'s docs make: no MCP target keeps its config
/// inside the project, so no secret can reach a version-controlled file. If a
/// project-scoped target is ever added, this fails instead of the guarantee
/// quietly becoming false.
///
/// Path *resolution* only — nothing is read from or written to a home
/// directory, and the assertion holds identically whether
/// `SKILLSTAR_TOOL_SYNC_HOME` is set or the real home is resolved.
#[test]
fn no_mcp_target_writes_a_project_scoped_config() {
    let mut s = server("secrets");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("TOKEN", secret_input())];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    assert!(!plan.secret_policy.writes_project_scoped_config);
}

#[test]
fn an_explicit_runtime_id_overrides_the_recommendation() {
    let mut s = server("override");
    s.remotes = vec![remote("http", "https://acme.dev/mcp")];
    s.packages = vec![package("npm", "npx", "@acme/x")];

    let recommended = build_install_plan_with(&s, None, &mut everything_installed);
    assert_eq!(recommended.selected_runtime_id.as_deref(), Some("remote:0"));
    assert_eq!(recommended.transport, "http");

    let overridden = build_install_plan_with(&s, Some("package:0"), &mut everything_installed);
    assert_eq!(overridden.selected_runtime_id.as_deref(), Some("package:0"));
    assert_eq!(overridden.transport, "stdio");
    // Alternatives stay on the plan so the UI can offer a way back.
    assert_eq!(overridden.selection.candidates.len(), 2);
}

#[test]
fn an_unknown_runtime_id_degrades_to_the_recommendation() {
    let mut s = server("stale-id");
    s.packages = vec![package("npm", "npx", "@acme/x")];

    let plan = build_install_plan_with(&s, Some("package:42"), &mut everything_installed);
    assert_eq!(plan.selected_runtime_id.as_deref(), Some("package:0"));
}

#[test]
fn deprecated_and_superseded_servers_warn_before_install() {
    let mut s = server("old");
    s.status = McpServerStatus::Deprecated;
    s.is_latest = false;
    s.packages = vec![package("npm", "npx", "@acme/x")];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    assert!(plan.warnings.iter().any(|w| w.contains("deprecated")));
    assert!(plan.warnings.iter().any(|w| w.contains("newer version")));
}

/// A missing toolchain must still yield a *fillable* draft: the entry is
/// correct, it just cannot start until the user installs the runtime. The plan
/// is where that blocker is stated.
#[test]
fn a_missing_toolchain_still_prefills_the_draft_and_states_the_blocker() {
    let mut s = server("blocked");
    s.packages = vec![package("oci", "docker", "acme/x")];

    let plan = build_install_plan_with(&s, None, &mut |_| false);
    assert_eq!(plan.selection.recommended_id, None);
    assert_eq!(plan.selected_runtime_id.as_deref(), Some("package:0"));
    assert_eq!(plan.draft.command.as_deref(), Some("docker"));
    assert!(plan.warnings.iter().any(|w| w.contains("docker")));
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("can run on this machine"))
    );
    assert_eq!(plan.selection.candidates.len(), 1);
}

/// A shape that cannot be expressed as a launch spec at all must *not* be
/// prefilled from — `command: "mcpb"` would be a command line that is simply
/// wrong. An unusable-here toolchain and an unusable-anywhere shape are
/// different failures.
#[test]
fn a_structurally_unusable_shape_is_never_prefilled_from() {
    let mut s = server("mixed-blocked");
    s.packages = vec![
        package("mcpb", "mcpb", "acme.mcpb"),
        package("npm", "npx", "@acme/x"),
    ];

    let plan = build_install_plan_with(&s, None, &mut |_| false);
    // MCPB ranks higher but is skipped; the npm package is merely missing npx.
    assert_eq!(plan.selected_runtime_id.as_deref(), Some("package:1"));
    assert_eq!(plan.draft.command.as_deref(), Some("npx"));
}


// ---------------------------------------------------------------------------
// Preset mapping
// ---------------------------------------------------------------------------

#[test]
fn curated_rows_become_presets_with_required_env_and_publisher_tags() {
    let mut s = server("bigmodel-vision");
    s.recommended = true;
    s.source = Some("bigmodel".into());
    let mut pkg = package("npm", "npx", "@z_ai/mcp-server");
    pkg.environment_variables = vec![env("Z_AI_API_KEY", secret_input())];
    pkg.required_env = vec!["Z_AI_API_KEY".into()];
    s.packages = vec![pkg];

    let preset = curated_server_to_preset(&s);
    assert_eq!(preset.id, "bigmodel-vision-id");
    assert_eq!(preset.transport, "stdio");
    assert_eq!(preset.required_env, vec!["Z_AI_API_KEY".to_string()]);
    assert!(preset.tags.contains(&"recommended".to_string()));
    assert!(preset.tags.contains(&"bigmodel".to_string()));
    assert!(
        !preset.env.contains_key("Z_AI_API_KEY"),
        "a preset must not pretend to know a secret, nor pin it blank: {:?}",
        preset.env
    );
    // `required_env` is what tells the UI to ask for it; `env` only carries
    // values the registry actually supplied.
    assert!(preset.env.is_empty());
    let _: BTreeMap<String, String> = preset.env;
}

/// The chip routes on this marker alone. "Try the wizard, fall back to the
/// form if the row does not resolve" would hand a built-in preset's entry
/// point to a transient catalog read.
#[test]
fn only_curated_presets_carry_a_catalog_row_id() {
    let mut s = server("curated-one");
    s.recommended = true;

    assert_eq!(
        curated_server_to_preset(&s).catalog_id.as_deref(),
        Some("curated-one-id"),
        "a curated preset's id is its catalog row id, so the wizard can resolve it"
    );
    for preset in skillstar_models::mcp::get_mcp_presets() {
        assert_eq!(
            preset.catalog_id, None,
            "built-in preset '{}' has no catalog row and must keep the form path",
            preset.id
        );
    }
}

/// Pins the A.3-f regression: presets used to be *either* the curated rows *or*
/// the built-in catalog, and since only one curated row carries
/// `recommended: true`, the UI ended up with a single chip.
#[test]
fn recommended_curated_rows_join_the_builtin_catalog_instead_of_replacing_it() {
    let builtin = skillstar_models::mcp::get_mcp_presets();
    let mut promoted = server("acme-promoted");
    promoted.recommended = true;
    let ordinary = server("acme-ordinary");

    let merged = list_mcp_presets_with(|| Ok(vec![promoted, ordinary]));

    assert_eq!(merged.len(), builtin.len() + 1);
    assert!(merged.iter().any(|p| p.id == "acme-promoted-id"));
    assert!(
        !merged.iter().any(|p| p.id == "acme-ordinary-id"),
        "only promoted curated rows belong in the preset chips"
    );
    for preset in &builtin {
        assert!(
            merged.iter().any(|p| p.id == preset.id),
            "built-in preset '{}' must still reach the UI",
            preset.id
        );
    }
}

/// The host bootstraps the snapshot runtime (db path, data root, skill
/// loaders); this crate only opens what it configured. Nothing in the read path
/// fails when that bootstrap never ran — the default runtime points at a temp
/// dir, so every query succeeds and returns nothing, and the curated chips
/// simply vanish. Pinned as a refusal instead, which the caller already turns
/// into a logged warning plus the built-in catalog.
#[test]
fn an_unconfigured_snapshot_runtime_is_refused_rather_than_read_as_empty() {
    assert!(
        skillstar_marketplace::snapshot::runtime_is_default(),
        "no test in this crate configures the snapshot runtime; if one now does, \
         this test needs its own isolation rather than a relaxed assertion"
    );

    let err = load_curated_servers()
        .expect_err("an unconfigured host must not read as 'no curated rows'");
    assert!(
        err.to_string().contains("never configured"),
        "the refusal has to name the cause, not just fail: {err}"
    );
}

/// A missing or corrupt snapshot DB degrades to "no curated additions", never
/// to "no presets at all".
#[test]
fn a_failed_curated_read_still_serves_the_builtin_catalog() {
    let builtin = skillstar_models::mcp::get_mcp_presets();

    let merged = list_mcp_presets_with(|| Err(anyhow::anyhow!("snapshot db is unreadable")));

    assert_eq!(
        merged.iter().map(|p| &p.id).collect::<Vec<_>>(),
        builtin.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}
