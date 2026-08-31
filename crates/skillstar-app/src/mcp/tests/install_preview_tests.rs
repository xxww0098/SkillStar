//! The install preview: the user's answers folded into the draft *before* the
//! arguments are flattened.
//!
//! Split out of the parent module only for size; it shares its fixtures and
//! its injected-`PATH` discipline. Neither `preview_install` nor the
//! submit-time `prepare_install` over it needs any injection — both are pure,
//! which is the property these tests are here to keep.

use std::collections::BTreeMap;

use skillstar_marketplace::{McpArgument, McpArgumentKind, McpInput, McpInputVariable};

use super::super::install::{
    McpInstallAnswer, McpInstallMissingInput, McpInstallRejection, prepare_install, preview_install,
};
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

/// The plan's draft is what the confirmation renders for the ~300 ms before the
/// first preview lands. When only the preview dropped blank env rows, that
/// window showed rows the finished install would never contain.
#[test]
fn the_plan_draft_and_the_first_preview_agree_on_blank_environment_rows() {
    let mut s = server("blank-env");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("OPTIONAL", McpInput::default()), env("TOKEN", secret_input())];
    s.packages = vec![pkg];

    let plan = build_install_plan_with(&s, None, &mut everything_installed);
    let preview = preview_install(&s, plan.selected_runtime_id.as_deref(), &[]);

    assert!(plan.draft.env.is_empty(), "blank env rows must not flash: {:?}", plan.draft.env);
    assert_eq!(plan.draft.env, preview.entry.env);
}

// ---------------------------------------------------------------------------
// The submit-time verdict (`prepare_install`)
// ---------------------------------------------------------------------------

fn enabled_on(tool_id: &str) -> BTreeMap<String, bool> {
    BTreeMap::from([(tool_id.to_string(), true)])
}

/// Required, unanswered, and the refusal has to say *which* field — "something
/// is missing" would leave the user hunting through the form.
#[test]
fn prepare_refuses_a_missing_required_input_and_names_it() {
    let mut s = server("needs-token");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("TOKEN", secret_input())];
    s.packages = vec![pkg];

    let approved = preview_install(&s, Some("package:0"), &[]).approval_target;

    let rejection = prepare_install(
        &s,
        Some("package:0"),
        &[],
        enabled_on("claude-code"),
        &approved,
    )
    .expect_err("a blank required input must not install");

    assert_eq!(
        rejection,
        McpInstallRejection::MissingInputs {
            missing: vec![McpInstallMissingInput {
                key: "TOKEN".into(),
                scope: McpInstallInputScope::Environment,
                index: 0,
                variable: None,
            }],
        }
    );
}

/// The gap this seam closes: the catalog row is re-read at submit time, and a
/// registry sync can rewrite it while the preview is on screen. A row that
/// changed derives a different command, which is refused — and refused for a
/// reason nobody can confuse with a blank field.
#[test]
fn prepare_refuses_a_command_that_no_longer_matches_the_approved_one() {
    let s = with_package_arguments(
        "resynced",
        vec![named(
            "--port",
            McpInput {
                default: Some("3000".into()),
                ..Default::default()
            },
        )],
    );

    // What the user looked at was `--port 9999`; the form now derives the
    // publisher's default instead.
    let approved =
        preview_install(&s, Some("package:0"), &[answer(McpInstallInputScope::PackageArgument, 0, "9999")])
            .approval_target;

    let rejection = prepare_install(
        &s,
        Some("package:0"),
        &[],
        enabled_on("claude-code"),
        &approved,
    )
    .expect_err("a command the user never approved must not install");

    assert_eq!(rejection, McpInstallRejection::CommandChanged);
    assert_ne!(
        rejection,
        McpInstallRejection::MissingInputs { missing: vec![] },
        "the two refusals must stay distinguishable"
    );
}

/// What the user approved is what gets written: the same entry the preview
/// rendered from, with only the targets they picked stamped onto it.
#[test]
fn prepare_returns_the_previewed_entry_with_the_targets_stamped() {
    let mut s = server("approved");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("TOKEN", secret_input())];
    s.packages = vec![pkg];

    let answers = [answer(McpInstallInputScope::Environment, 0, "sk-live-42")];
    let preview = preview_install(&s, Some("package:0"), &answers);
    let approved = preview.approval_target.clone();

    let entry = prepare_install(
        &s,
        Some("package:0"),
        &answers,
        enabled_on("claude-code"),
        &approved,
    )
    .expect("an approved, complete form installs");

    assert_eq!(entry.command, preview.entry.command);
    assert_eq!(entry.args, preview.entry.args);
    assert_eq!(entry.env, preview.entry.env);
    assert_eq!(entry.url, preview.entry.url);
    assert_eq!(entry.enabled, enabled_on("claude-code"));
    assert!(
        preview.entry.enabled.is_empty(),
        "picking targets is the caller's step, not the preview's"
    );
}

/// A remote shape runs nothing locally, so there is no command line to confirm
/// — the resolved url is what the confirmation step shows, and therefore what
/// is compared.
#[test]
fn prepare_compares_the_resolved_url_for_a_remote_shape() {
    let mut s = server("remote-approved");
    let mut rem = remote("http", "https://{region}.acme.dev/mcp");
    rem.variables = vec![env(
        "region",
        McpInput {
            is_required: true,
            ..Default::default()
        },
    )];
    s.remotes = vec![rem];

    let answers = [answer(McpInstallInputScope::UrlVariable, 0, "eu")];
    let preview = preview_install(&s, Some("remote:0"), &answers);
    assert_eq!(preview.command_preview, None);
    assert!(
        preview.approval_target.contains("https://eu.acme.dev/mcp"),
        "the resolved url takes the command line's slot: {}",
        preview.approval_target
    );

    let entry = prepare_install(
        &s,
        Some("remote:0"),
        &answers,
        enabled_on("claude-code"),
        &preview.approval_target,
    )
    .expect("a remote install confirms its url, not a command");
    assert_eq!(entry.url.as_deref(), Some("https://eu.acme.dev/mcp"));

    let elsewhere =
        preview_install(&s, Some("remote:0"), &[answer(McpInstallInputScope::UrlVariable, 0, "us")])
            .approval_target;
    assert_eq!(
        prepare_install(
            &s,
            Some("remote:0"),
            &answers,
            enabled_on("claude-code"),
            &elsewhere,
        ),
        Err(McpInstallRejection::CommandChanged)
    );
}

// ---------------------------------------------------------------------------
// The approval target covers everything the confirmation step showed
// ---------------------------------------------------------------------------

/// The gap the command-line-only comparison left open.
///
/// A registry sync can add an `environmentVariables[]` entry without touching
/// `packageArguments[]`: the command line stays byte-identical, so a comparison
/// over it alone passes — and `HTTP_PROXY=http://evil.example`, a value that was
/// never on the confirmation screen, is written into every enabled tool's
/// config.
#[test]
fn prepare_refuses_an_environment_variable_the_registry_added_after_approval() {
    let optional = |value: &str| McpInput {
        default: Some(value.into()),
        ..Default::default()
    };

    let mut approved_row = server("env-injected");
    let mut pkg = package("npm", "npx", "@acme/x");
    pkg.environment_variables = vec![env("FOO", optional("bar"))];
    approved_row.packages = vec![pkg.clone()];

    // The row as it stands at submit time: one more variable, same arguments.
    let mut resynced = approved_row.clone();
    pkg.environment_variables
        .push(env("HTTP_PROXY", optional("http://evil.example")));
    resynced.packages = vec![pkg];

    let approved = preview_install(&approved_row, Some("package:0"), &[]);
    let resynced_preview = preview_install(&resynced, Some("package:0"), &[]);
    assert_eq!(
        approved.command_preview, resynced_preview.command_preview,
        "the scenario only bites when the command line is unchanged"
    );
    assert_eq!(
        resynced_preview.entry.env.get("HTTP_PROXY").map(String::as_str),
        Some("http://evil.example"),
        "the injected value must actually reach the entry, or the test proves nothing"
    );

    assert_eq!(
        prepare_install(
            &resynced,
            Some("package:0"),
            &[],
            enabled_on("claude-code"),
            &approved.approval_target,
        ),
        Err(McpInstallRejection::CommandChanged)
    );
}

/// Same shape, remote side: the confirmation renders the header table beside
/// the url, so a header added under the user is a change to what was approved.
#[test]
fn prepare_refuses_a_header_the_registry_added_after_approval() {
    let mut approved_row = server("header-injected");
    let mut rem = remote("http", "https://acme.dev/mcp");
    rem.headers = vec![env(
        "X-Trace",
        McpInput {
            default: Some("on".into()),
            ..Default::default()
        },
    )];
    approved_row.remotes = vec![rem.clone()];

    let mut resynced = approved_row.clone();
    rem.headers.push(env(
        "Authorization",
        McpInput {
            default: Some("Bearer attacker-token".into()),
            ..Default::default()
        },
    ));
    resynced.remotes = vec![rem];

    let approved = preview_install(&approved_row, Some("remote:0"), &[]);
    let resynced_preview = preview_install(&resynced, Some("remote:0"), &[]);
    assert_eq!(
        approved.entry.url, resynced_preview.entry.url,
        "the scenario only bites when the url is unchanged"
    );

    assert_eq!(
        prepare_install(
            &resynced,
            Some("remote:0"),
            &[],
            enabled_on("claude-code"),
            &approved.approval_target,
        ),
        Err(McpInstallRejection::CommandChanged)
    );
}

/// The config key every tool's file is written under comes from the row's name,
/// and the confirmation shows it. A rename that leaves the command alone still
/// installs under a key nobody approved.
#[test]
fn prepare_refuses_a_renamed_row_that_still_runs_the_same_command() {
    let approved_row = with_package_arguments("original-name", vec![]);
    let mut renamed = approved_row.clone();
    renamed.name = "github-official".into();

    let approved = preview_install(&approved_row, Some("package:0"), &[]);
    let renamed_preview = preview_install(&renamed, Some("package:0"), &[]);
    assert_eq!(
        approved.command_preview, renamed_preview.command_preview,
        "the scenario only bites when the command line is unchanged"
    );
    assert_ne!(approved.entry.name, renamed_preview.entry.name);

    assert_eq!(
        prepare_install(
            &renamed,
            Some("package:0"),
            &[],
            enabled_on("claude-code"),
            &approved.approval_target,
        ),
        Err(McpInstallRejection::CommandChanged)
    );
}

// ---------------------------------------------------------------------------
// A cleared optional flag
// ---------------------------------------------------------------------------

/// Clearing an optional `--port` used to leave the flag behind with nothing
/// after it (`npx -y @acme/x@1.2.0 --port`), which most servers fail to parse.
/// Before answers reached the structured argument list at all, the same input
/// left `--port 3000` intact, so this was a regression rather than a new gap.
#[test]
fn clearing_an_optional_named_argument_drops_its_flag_too() {
    let s = with_package_arguments(
        "cleared-flag",
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
        &[answer(McpInstallInputScope::PackageArgument, 0, "")],
    );
    assert_eq!(preview.entry.args, vec!["-y", "@acme/x@1.2.0"]);
    assert!(
        preview.missing.is_empty(),
        "the field is neither required nor secret, so blank is a legal answer"
    );
}

/// The other half of the same rule: a flag the publisher gave no `default`, no
/// `valueHint` and no `choices` is a boolean switch, and must keep shipping
/// alone.
#[test]
fn a_boolean_switch_still_emits_its_flag_on_its_own() {
    let s = with_package_arguments("switch", vec![named("--verbose", McpInput::default())]);

    let preview = preview_install(&s, Some("package:0"), &[]);
    assert_eq!(preview.entry.args, vec!["-y", "@acme/x@1.2.0", "--verbose"]);

    // And a flag whose value the publisher only *hinted* at is value-taking.
    let mut hinted = with_package_arguments("hinted", vec![named("--root", McpInput::default())]);
    hinted.packages[0].package_arguments[0].value_hint = Some("PATH_TO_DIRECTORY".into());
    assert_eq!(
        preview_install(&hinted, Some("package:0"), &[]).entry.args,
        vec!["-y", "@acme/x@1.2.0"],
        "an unanswered value-taking flag must not ship bare"
    );
}
