//! The install preview: the user's answers folded into the draft *before* the
//! arguments are flattened.
//!
//! Split out of the parent module only for size; it shares its fixtures and
//! its injected-`PATH` discipline. `preview_install` needs no injection at all
//! — it is pure, which is the property these tests are here to keep.

use std::collections::BTreeMap;

use skillstar_marketplace::{McpArgument, McpArgumentKind, McpInput, McpInputVariable};

use super::super::install::{McpInstallAnswer, McpInstallMissingInput, preview_install};
use super::*;

fn named(name: &str, input: McpInput) -> McpArgument {
    McpArgument {
        kind: McpArgumentKind::Named,
        name: Some(name.into()),
        input,
        ..Default::default()
    }
}

fn positional(input: McpInput) -> McpArgument {
    McpArgument {
        kind: McpArgumentKind::Positional,
        input,
        ..Default::default()
    }
}

fn answer(scope: McpInstallInputScope, index: u32, value: &str) -> McpInstallAnswer {
    McpInstallAnswer {
        scope,
        index,
        variable: None,
        value: value.into(),
    }
}

fn variable_answer(
    scope: McpInstallInputScope,
    index: u32,
    variable: &str,
    value: &str,
) -> McpInstallAnswer {
    McpInstallAnswer {
        scope,
        index,
        variable: Some(variable.into()),
        value: value.into(),
    }
}

/// A one-package server whose `packageArguments[]` are `args`.
fn with_package_arguments(name: &str, args: Vec<McpArgument>) -> McpRegistryServer {
    let mut s = server(name);
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.package_arguments = args;
    s.packages = vec![pkg];
    s
}


/// The regression this seam exists for.
///
/// The renderer used to receive `args` already flattened and splice each answer
/// back in by `indexOf(flag)`, so an argument the publisher gave a `default`
/// was emitted once by the draft builder and inserted a second time by the
/// form: `--port 3000 3000`. Substituting into the structured list generates it
/// once, whether or not the user ever touched the field.
#[test]
fn a_named_argument_with_a_default_is_emitted_exactly_once() {
    let s = with_package_arguments(
        "named-default",
        vec![named(
            "--port",
            McpInput {
                default: Some("3000".into()),
                ..Default::default()
            },
        )],
    );

    let preview = preview_install(&s, Some("package:0"), &[]);
    assert_eq!(
        preview.entry.args,
        vec!["-y", "@acme/x@1.2.0", "--port", "3000"]
    );
    assert_eq!(
        preview.command_preview.as_deref(),
        Some("npx -y @acme/x@1.2.0 --port 3000")
    );
    assert!(preview.missing.is_empty());
}

/// Same bug, positional shape: the value used to be appended a second time
/// (`/tmp /tmp`) because a positional has no flag to splice after.
#[test]
fn a_positional_argument_with_a_default_is_emitted_exactly_once() {
    let s = with_package_arguments(
        "positional-default",
        vec![positional(McpInput {
            default: Some("/tmp".into()),
            ..Default::default()
        })],
    );

    let preview = preview_install(&s, Some("package:0"), &[]);
    assert_eq!(preview.entry.args, vec!["-y", "@acme/x@1.2.0", "/tmp"]);
}

#[test]
fn an_answer_replaces_the_prefill_instead_of_joining_it() {
    let s = with_package_arguments(
        "answered",
        vec![named(
            "--port",
            McpInput {
                default: Some("3000".into()),
                ..Default::default()
            },
        )],
    );

    let preview = preview_install(
        &s,
        Some("package:0"),
        &[answer(McpInstallInputScope::PackageArgument, 0, "9000")],
    );
    assert_eq!(
        preview.entry.args,
        vec!["-y", "@acme/x@1.2.0", "--port", "9000"]
    );
}

/// A publisher-pinned `value` is not the user's to edit, but its
/// `{curly_braces}` are. Those used to be scanned and substituted in the
/// renderer, which never reached the entry — the braces shipped verbatim.
#[test]
fn a_templated_argument_ships_with_its_variable_substituted() {
    let s = with_package_arguments(
        "templated",
        vec![positional(McpInput {
            value: Some("--cfg={PATH}".into()),
            variables: BTreeMap::from([(
                "PATH".to_string(),
                McpInputVariable {
                    is_required: true,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })],
    );

    // Unanswered: the hole stays visible, and the plan says which one it is.
    let bare = preview_install(&s, Some("package:0"), &[]);
    assert_eq!(
        bare.entry.args.last().map(String::as_str),
        Some("--cfg={PATH}")
    );
    assert_eq!(
        bare.missing,
        vec![McpInstallMissingInput {
            key: "argument".into(),
            scope: McpInstallInputScope::PackageArgument,
            index: 0,
            variable: Some("PATH".into()),
        }]
    );

    let filled = preview_install(
        &s,
        Some("package:0"),
        &[variable_answer(
            McpInstallInputScope::PackageArgument,
            0,
            "PATH",
            "/etc/acme.toml",
        )],
    );
    assert_eq!(
        filled.entry.args.last().map(String::as_str),
        Some("--cfg=/etc/acme.toml")
    );
    assert!(filled.missing.is_empty());
}

/// Both positionals answer to the key `"argument"`, so only the ordinal tells
/// them apart. Filling the first must not fill the second.
#[test]
fn two_hint_less_positional_arguments_take_their_own_answers() {
    let s = with_package_arguments(
        "two-positionals",
        vec![
            positional(McpInput {
                is_required: true,
                ..Default::default()
            }),
            positional(McpInput {
                is_required: true,
                ..Default::default()
            }),
        ],
    );

    let half = preview_install(
        &s,
        Some("package:0"),
        &[answer(McpInstallInputScope::PackageArgument, 0, "/src")],
    );
    assert_eq!(half.entry.args, vec!["-y", "@acme/x@1.2.0", "/src"]);
    assert_eq!(
        half.missing,
        vec![McpInstallMissingInput {
            key: "argument".into(),
            scope: McpInstallInputScope::PackageArgument,
            index: 1,
            variable: None,
        }]
    );

    let both = preview_install(
        &s,
        Some("package:0"),
        &[
            answer(McpInstallInputScope::PackageArgument, 0, "/src"),
            answer(McpInstallInputScope::PackageArgument, 1, "/dst"),
        ],
    );
    assert_eq!(both.entry.args, vec!["-y", "@acme/x@1.2.0", "/src", "/dst"]);
    assert!(both.missing.is_empty());
}

/// Environment variables and headers belong to different scopes even when they
/// share an ordinal, and a value nobody supplied is left out rather than pinned
/// into every tool's config as an empty string.
#[test]
fn environment_values_land_by_scope_and_blank_ones_are_dropped() {
    let mut s = server("env-scope");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![
        env("OPTIONAL", McpInput::default()),
        env("TOKEN", secret_input()),
    ];
    s.packages = vec![pkg];

    let preview = preview_install(
        &s,
        Some("package:0"),
        &[answer(McpInstallInputScope::Environment, 1, "sk-live-42")],
    );

    assert_eq!(
        preview.entry.env.get("TOKEN").map(String::as_str),
        Some("sk-live-42")
    );
    assert!(
        !preview.entry.env.contains_key("OPTIONAL"),
        "an unanswered optional must not be written as an empty string: {:?}",
        preview.entry.env
    );
    // A secret bound for the environment never reaches the command line.
    let command = preview.command_preview.expect("stdio installs preview");
    assert!(
        !command.contains("sk-live-42"),
        "a secret leaked into the command preview: {command}"
    );
}

#[test]
fn header_answers_land_on_the_remote_and_url_variables_are_substituted() {
    let mut s = server("remote-answers");
    let mut rem = remote("http", "https://{region}.acme.dev/{tenant}/mcp");
    rem.headers = vec![
        env(
            "Authorization",
            McpInput {
                value: Some("Bearer {TOKEN}".into()),
                variables: BTreeMap::from([(
                    "TOKEN".to_string(),
                    McpInputVariable {
                        is_secret: true,
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        ),
        env("X-Trace", McpInput::default()),
    ];
    rem.variables = vec![
        env(
            "region",
            McpInput {
                is_required: true,
                ..Default::default()
            },
        ),
        env(
            "tenant",
            McpInput {
                is_required: true,
                ..Default::default()
            },
        ),
    ];
    s.remotes = vec![rem];

    let preview = preview_install(
        &s,
        Some("remote:0"),
        &[
            variable_answer(McpInstallInputScope::Header, 0, "TOKEN", "abc123"),
            answer(McpInstallInputScope::UrlVariable, 0, "eu"),
        ],
    );

    assert_eq!(
        preview.entry.headers.get("Authorization").map(String::as_str),
        Some("Bearer abc123")
    );
    assert!(
        !preview.entry.headers.contains_key("X-Trace"),
        "an unanswered optional header must be dropped: {:?}",
        preview.entry.headers
    );
    // The answered variable is substituted; the one nobody answered keeps its
    // braces, so the gap is visible rather than silently collapsed.
    assert_eq!(
        preview.entry.url.as_deref(),
        Some("https://eu.acme.dev/{tenant}/mcp")
    );
    assert_eq!(
        preview.missing,
        vec![McpInstallMissingInput {
            key: "tenant".into(),
            scope: McpInstallInputScope::UrlVariable,
            index: 1,
            variable: None,
        }]
    );
    // Nothing runs locally, so there is no command to confirm.
    assert_eq!(preview.command_preview, None);
}

/// The preview is the same derivation the plan uses, so an untouched form
/// re-renders the plan's own string byte for byte. This is what makes "the
/// command changed because of your answers" a meaningful thing to say.
#[test]
fn an_untouched_form_previews_exactly_what_the_plan_showed() {
    let s = with_package_arguments(
        "untouched",
        vec![named(
            "--root",
            McpInput {
                default: Some("/Users/me/My Documents".into()),
                ..Default::default()
            },
        )],
    );

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let preview = preview_install(&s, plan.selected_runtime_id.as_deref(), &[]);
    assert_eq!(preview.command_preview, plan.command_preview);
    assert_eq!(preview.entry.args, plan.draft.args);
}
