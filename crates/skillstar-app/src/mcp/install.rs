//! The install plan: everything the frontend needs to ask for consent *before*
//! anything is written or launched.
//!
//! Two requirements shape this type.
//!
//! **The command confirmation (research §7 P1-6, a spec MUST).** Deeplink-style
//! one-click installs are a demonstrated attack surface (CursorJack), and the
//! only effective mitigation is showing the user the *complete, untruncated,
//! already-resolved* command before it runs. [`McpInstallPlan::command_preview`]
//! is that string, [`McpInstallPlan::resolved_command_path`] is the exact binary
//! `PATH` resolved to, and [`McpInstallPlan::uses_shell`] is pinned `false`
//! because `StdioTransport::spawn` execs the program directly — a registry
//! author's argument string is never handed to `sh -c`.
//!
//! **Secret triage (research §7 P0-4).** Every input keeps its full `Input`
//! semantics so the form can render a password box for `isSecret`, a select for
//! `choices`, a file picker for `format: "filepath"`, and so on. Where those
//! values then land is [`McpSecretPolicy`] — see its docs for the deliberate
//! limitation.
//!
//! **What this module validates, and what it does not.** [`prepare_install`]
//! enforces *requiredness* only: a required input left blank is refused, and so
//! is a set of answers that no longer derives what the user approved. The rest
//! of the publisher's `Input` semantics — `choices` membership, `format`
//! (`number` / `boolean` / `filepath`) — are checked by the renderer, which is
//! also the thing that can offer a select or a file picker instead of a text
//! box. A caller with no renderer (the CLI the spec anticipates) therefore gets
//! the requiredness and re-confirmation guarantees, but must apply `choices` and
//! `format` itself: [`McpInstallInput::input`] carries them verbatim for exactly
//! that reason.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use skillstar_marketplace::{McpInput, McpInputVariable, McpRegistryServer};
use skillstar_models::mcp::{MCP_TOOL_IDS, McpServerEntry, resolve_mcp_config_path, resolve_runtime};
use ts_rs::TS;

use super::draft::{Answers, prefill, registry_to_entry_answered, registry_to_entry_for, sanitize_key};
use super::runtime::{
    CandidateOrigin, McpRuntimeSelection, parse_candidate_id, select_runtime, select_runtime_with,
};

/// Which part of the launch spec an input feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallInputScope.ts")]
pub enum McpInstallInputScope {
    /// A process environment variable (`environmentVariables[]`).
    Environment,
    /// An HTTP header sent to a remote server (`headers[]`).
    Header,
    /// A `{curly_braces}` substitution inside the remote's url.
    UrlVariable,
    /// An argument passed to the runner (`runtimeArguments[]`).
    RuntimeArgument,
    /// An argument passed to the server itself (`packageArguments[]`).
    PackageArgument,
}

/// One `{curly_braces}` hole inside an input's publisher-pinned `value`.
///
/// Seeded here rather than by the form, so the token list and the semantics
/// behind each token have a single derivation. Nesting stops at this level:
/// [`McpInputVariable`] has no `value` of its own, so a variable never carries
/// variables.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallInputVariable.ts")]
pub struct McpInstallInputVariable {
    /// Token name, i.e. `TOKEN` for `{TOKEN}`.
    pub name: String,
    /// The publisher's declared semantics for this token, verbatim. A token
    /// the publisher left undeclared is reported as a required plain string —
    /// the template needs it either way.
    pub variable: McpInputVariable,
    /// The value the form starts at. Empty means the user must supply it.
    pub prefilled: String,
}

/// One field the install form has to render.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallInput.ts")]
pub struct McpInstallInput {
    /// Env var name, header name, url variable name, or the argument's flag /
    /// value hint. **Not an identity**: a positional argument with neither a
    /// name nor a `valueHint` falls back to the literal `"argument"`, so two of
    /// them share this string. Address an input by `(scope, index)` instead.
    pub key: String,
    pub scope: McpInstallInputScope,
    /// Position within `scope`, in the order the publisher declared it. This is
    /// what tells two otherwise identical inputs apart.
    pub index: u32,
    /// The publisher's declared semantics, verbatim: `isRequired`, `isSecret`,
    /// `format`, `choices`, `default`, `placeholder`, the `value` template and
    /// its nested `variables`.
    pub input: McpInput,
    /// What the draft was prefilled with. Empty means the user must supply it.
    pub prefilled: String,
    /// `true` when the form must collect a value before install can proceed.
    pub must_ask: bool,
    /// The `{curly_braces}` this input's pinned `value` leaves for the user, in
    /// template order, de-duplicated. Empty when there is no template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<McpInstallInputVariable>,
}

/// One value the user supplied, addressed the way [`McpInstallPlan::inputs`]
/// numbered the form.
///
/// `key` is deliberately not part of the address: a positional argument the
/// publisher gave neither a name nor a `valueHint` falls back to the literal
/// `"argument"`, so two of them would collide. `(scope, index, variable)` does
/// not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallAnswer.ts")]
pub struct McpInstallAnswer {
    pub scope: McpInstallInputScope,
    /// [`McpInstallInput::index`] — the ordinal within `scope`.
    pub index: u32,
    /// `None` addresses the input's own value; `Some(name)` addresses one
    /// `{curly_brace}` inside its publisher-pinned template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    pub value: String,
}

/// A required field the answers do not cover yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallMissingInput.ts")]
pub struct McpInstallMissingInput {
    /// The publisher's label, for the message. Not an identity — see
    /// [`McpInstallInput::key`].
    pub key: String,
    pub scope: McpInstallInputScope,
    pub index: u32,
    /// Set when what is missing is one `{curly_brace}` inside a template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
}

/// What [`preview_install`] returns: the entry as it would be written, the
/// command line as it would run, and what is still missing.
///
/// The two are one derivation, not two: `command_preview` is rendered from
/// `entry.args`, so the string the user approves and the object that gets
/// written cannot drift apart.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallPreview.ts")]
pub struct McpInstallPreview {
    /// The entry these answers produce. `enabled` is still empty — picking
    /// targets is the caller's step.
    pub entry: McpServerEntry,
    /// The complete command line, **never truncated**, for display only.
    /// `None` for a remote server, which executes nothing locally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_preview: Option<String>,
    /// Everything the confirmation step put in front of the user, folded into
    /// one opaque comparable string — see [`approval_target`]. The caller
    /// echoes this back to [`prepare_install`] verbatim; deriving it a second
    /// time at the edge is what let the two drift apart.
    pub approval_target: String,
    /// Required inputs still unanswered. Empty means install may proceed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<McpInstallMissingInput>,
}

/// Why [`prepare_install`] refused, kept as two reasons on purpose: one asks
/// the user to supply something, the other asks them to look again. Collapsing
/// them into one message would leave the user guessing which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "reason", rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallRejection.ts")]
pub enum McpInstallRejection {
    /// Required inputs the answers still leave blank. Carries which ones, so
    /// the message can name them rather than say "something is missing".
    #[serde(rename_all = "camelCase")]
    MissingInputs {
        missing: Vec<McpInstallMissingInput>,
    },
    /// What these answers derive *now* is not what the user approved — not
    /// only the command line or the resolved url, but the environment, the
    /// headers and the config key that were on screen beside it.
    ///
    /// The catalog row is re-read at submit time, and a registry sync can
    /// rewrite it while the preview sits on screen. Installing anyway would
    /// install something nobody looked at, which is exactly the property the
    /// confirmation step exists to provide.
    CommandChanged,
}

/// Where a secret value physically ends up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSecretStorage.ts")]
pub enum McpSecretStorage {
    /// SkillStar's own user-level store plus each enabled tool's user-level
    /// config file. Never a project-scoped file.
    UserLevelConfig,
}

/// The secret-handling contract for one install.
///
/// **The deliberate limitation.** Research §7 P0-4 asks for secrets in the OS
/// credential store. A credential store is available (`keyring` is already a
/// workspace dependency, used by `skillstar-skills::github_auth` and `skillstar-sync`),
/// but it cannot be used here: the process that needs the secret is the *agent
/// tool* — Claude Code, Codex, Cursor — reading its own config file. A secret
/// only in SkillStar's keychain is a secret the MCP server never receives, so
/// the entry would install and then silently fail to start.
///
/// What P0-4 is actually protecting against is a secret reaching a
/// version-controlled file, i.e. a project-scoped `.mcp.json`. SkillStar writes
/// no project-scoped MCP config at all — every target in `MCP_TOOL_IDS`
/// resolves under the user's home directory — so that exposure does not exist,
/// and [`Self::writes_project_scoped_config`] is computed from the real
/// resolved paths rather than asserted.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSecretPolicy.ts")]
pub struct McpSecretPolicy {
    pub storage: McpSecretStorage,
    /// Keys of the inputs the publisher marked `isSecret`. The UI must render
    /// these masked and must not echo them back into logs or previews.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_keys: Vec<String>,
    /// True if any tool this install can write to keeps its MCP config inside
    /// the project (and would therefore likely be committed). Always false on
    /// the current target set; recomputed here so it stops being false the
    /// moment a project-scoped target is added.
    pub writes_project_scoped_config: bool,
    /// Plain-language summary for the confirmation step.
    pub note: String,
}

/// Everything the install confirmation step needs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallPlan.ts")]
pub struct McpInstallPlan {
    pub server_id: String,
    /// Sanitized config key the entry will be stored under.
    pub server_name: String,
    /// `server.json`'s reverse-DNS name.
    pub namespace: String,
    /// Every runtime shape, ranked — so the confirmation UI can offer the
    /// alternatives rather than only announcing the pick.
    pub selection: McpRuntimeSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_runtime_id: Option<String>,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Absolute path `command` resolves to on this machine — the binary that
    /// will actually be executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_command_path: Option<String>,
    /// The complete command line, **never truncated**, for display only. The
    /// UI must show this in full before install; nothing re-parses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_preview: Option<String>,
    /// Always `false`: the launcher is exec'd directly, never through a shell.
    pub uses_shell: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<McpInstallInput>,
    pub secret_policy: McpSecretPolicy,
    /// Caveats to show alongside the confirmation (deprecated transport,
    /// deprecated server, superseded version, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// The prefilled entry `create_mcp_server` will receive once the user has
    /// filled in the blanks.
    pub draft: McpServerEntry,
}

/// Build the confirmation payload for installing `server`.
///
/// `runtime_id` overrides the recommendation with any
/// [`super::runtime::McpRuntimeCandidate::id`]; an unknown id falls back to the
/// recommendation rather than failing, so a stale id from an older snapshot
/// degrades to the default instead of breaking install.
pub fn build_install_plan(server: &McpRegistryServer, runtime_id: Option<&str>) -> McpInstallPlan {
    build_plan(server, runtime_id, select_runtime(server))
}

/// [`build_install_plan`] with the `PATH` lookup injected (tests).
pub fn build_install_plan_with(
    server: &McpRegistryServer,
    runtime_id: Option<&str>,
    runtime_available: &mut dyn FnMut(&str) -> bool,
) -> McpInstallPlan {
    let selection = select_runtime_with(server, runtime_available);
    build_plan(server, runtime_id, selection)
}

fn build_plan(
    server: &McpRegistryServer,
    runtime_id: Option<&str>,
    selection: McpRuntimeSelection,
) -> McpInstallPlan {
    let candidate = selection.resolve(runtime_id).cloned();
    let draft = registry_to_entry_for(server, candidate.as_ref());

    let mut warnings: Vec<String> = candidate
        .as_ref()
        .map(|c| c.warnings.clone())
        .unwrap_or_default();
    // The selected shape may be prefilled-but-blocked (a missing toolchain).
    // Say so on the plan itself rather than only on the candidate, so a
    // confirmation dialog that shows warnings cannot miss it.
    if let Some(reason) = candidate.as_ref().and_then(|c| c.blocked_reason.clone()) {
        warnings.push(reason);
    }
    if selection.recommended_id.is_none() {
        warnings.push(
            "None of this server's published runtime shapes can run on this machine as-is. Review the candidates below before installing."
                .to_string(),
        );
    }
    if server.status != skillstar_marketplace::McpServerStatus::Active {
        warnings.push(format!(
            "The registry marks this server '{}'.",
            server.status.as_db_str()
        ));
    }
    if !server.is_latest {
        warnings.push("The registry knows of a newer version of this server.".to_string());
    }

    let inputs = collect_inputs(server, candidate.as_ref().map(|c| c.id.as_str()));
    let secret_keys: Vec<String> = inputs
        .iter()
        .filter(|i| i.input.is_secret)
        .map(|i| i.key.clone())
        .collect();

    let resolved_command_path = draft
        .command
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .and_then(|command| resolve_runtime(command).ok())
        .map(|path| path.display().to_string());
    let command_preview = draft
        .command
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .map(|command| render_command(command, &draft.args));

    McpInstallPlan {
        server_id: server.id.clone(),
        server_name: sanitize_key(&server.name),
        namespace: server.namespace.clone(),
        selected_runtime_id: candidate.as_ref().map(|c| c.id.clone()),
        selection,
        transport: draft.transport.clone(),
        command: draft.command.clone(),
        args: draft.args.clone(),
        resolved_command_path,
        command_preview,
        uses_shell: false,
        url: draft.url.clone(),
        inputs,
        secret_policy: secret_policy(secret_keys),
        warnings,
        draft,
    }
}

/// Fold `answers` into the draft and re-render the command line.
///
/// **Pure**: no `PATH` lookup, no filesystem, no clock — so the install wizard
/// can call it on every keystroke (debounced) and a future CLI can reuse it.
/// The one expensive judgement, "which of this server's shapes can run here",
/// already happened in [`build_install_plan`]; `runtime_id` is
/// [`McpInstallPlan::selected_runtime_id`], the answer it settled on. Recovering
/// that candidate needs only its id, so availability is assumed rather than
/// probed — an id that does not resolve degrades to rank order, the same
/// fillable-but-blocked draft the plan returns when nothing is installable.
///
/// Answers are substituted while the argument list is still structured and
/// ordered, so `args` is generated **once** rather than patched afterwards.
pub fn preview_install(
    server: &McpRegistryServer,
    runtime_id: Option<&str>,
    answers: &[McpInstallAnswer],
) -> McpInstallPreview {
    let selection = select_runtime_with(server, &mut |_| true);
    let candidate = selection.resolve(runtime_id);
    let answers = Answers::new(answers);

    let entry = registry_to_entry_answered(server, candidate, answers);
    let command_preview = entry
        .command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
        .map(|command| render_command(command, &entry.args));

    McpInstallPreview {
        missing: missing_inputs(server, candidate.map(|c| c.id.as_str()), answers),
        approval_target: approval_target(&entry, command_preview.as_deref()),
        command_preview,
        entry,
    }
}

/// Validate one set of answers against what the user approved and stamp the
/// targets, or refuse with a reason they can act on.
///
/// A thin wrapper over [`preview_install`] rather than a second derivation:
/// `missing` is already that call's required-input verdict, and
/// `approved_target` is compared against the [`McpInstallPreview::approval_target`]
/// it just derived. Two derivations here would be the very drift this seam
/// removes.
///
/// **Pure**, like [`preview_install`]: no lock, no store, no projection. The
/// write lock is Tauri state and cannot move into a domain crate, so the caller
/// keeps the IO and this keeps the rules — which is what lets the rules be
/// driven straight from a test, and reused by a CLI that has no Tauri at all.
///
/// `approved_target` is the true, unmasked [`McpInstallPreview::approval_target`]
/// the confirmation step was rendered from. Masking is a display concern and
/// belongs at the edge that displays it; masking before comparing would make
/// every install with a secret in its environment fail to match.
pub fn prepare_install(
    server: &McpRegistryServer,
    runtime_id: Option<&str>,
    answers: &[McpInstallAnswer],
    enabled: BTreeMap<String, bool>,
    approved_target: &str,
) -> Result<McpServerEntry, McpInstallRejection> {
    let preview = preview_install(server, runtime_id, answers);

    // Checked before the missing-input verdict: when the row has changed under
    // the user, "fill in field X" would be an instruction about a form they
    // have never seen. Re-confirming comes first. A genuinely blank required
    // field renders the same command it did on screen, so this order hides
    // nothing.
    if preview.approval_target != approved_target.trim() {
        return Err(McpInstallRejection::CommandChanged);
    }
    if !preview.missing.is_empty() {
        return Err(McpInstallRejection::MissingInputs {
            missing: preview.missing,
        });
    }

    let mut entry = preview.entry;
    entry.enabled = enabled;
    Ok(entry)
}

/// Everything the user actually confirmed, in one comparable string.
///
/// Not just the command line. The confirmation step renders the environment and
/// the header tables beside it, and the config key every tool's file is written
/// under comes from `entry.name` — so all four have to still hold at submit
/// time. Comparing the command line alone let a registry sync add
/// `environmentVariables: [{ name: "HTTP_PROXY", … }]` to a row whose `args` it
/// never touched: byte-identical command, and a proxy the user never saw
/// written into every enabled tool's config.
///
/// A remote shape executes nothing locally, so there is no command line to show
/// it — the resolved url takes that slot.
///
/// JSON rather than a joined string because the values are publisher-controlled:
/// a newline or a `=` inside one must not be able to forge a row and make two
/// different approvals compare equal. The maps are `BTreeMap`s, so the encoding
/// is order-stable. The result is opaque — nothing parses it, and it is never
/// shown to the user, who reads [`McpInstallPreview::command_preview`] instead.
fn approval_target(entry: &McpServerEntry, command_preview: Option<&str>) -> String {
    let target = command_preview
        .or(entry.url.as_deref())
        .unwrap_or_default()
        .trim();
    serde_json::json!({
        "name": entry.name,
        "target": target,
        "env": entry.env,
        "headers": entry.headers,
    })
    .to_string()
}

/// The required fields `answers` still leaves blank.
///
/// Requiredness follows the same rule the form shows: `must_ask` for a plain
/// field, and for a publisher-pinned template the requiredness of each
/// `{curly_brace}` — the template itself is not editable, so it cannot be the
/// thing that is missing.
fn missing_inputs(
    server: &McpRegistryServer,
    candidate_id: Option<&str>,
    answers: Answers<'_>,
) -> Vec<McpInstallMissingInput> {
    let mut out = Vec::new();
    for input in collect_inputs(server, candidate_id) {
        let index = input.index as usize;
        let blank = |variable: Option<&str>, seed: &str| {
            answers
                .get(input.scope, index, variable)
                .unwrap_or(seed)
                .trim()
                .is_empty()
        };

        if input.variables.is_empty() {
            if input.must_ask && blank(None, &input.prefilled) {
                out.push(McpInstallMissingInput {
                    key: input.key.clone(),
                    scope: input.scope,
                    index: input.index,
                    variable: None,
                });
            }
            continue;
        }

        for variable in &input.variables {
            if !(variable.variable.is_required || variable.variable.is_secret) {
                continue;
            }
            if blank(Some(&variable.name), &variable.prefilled) {
                out.push(McpInstallMissingInput {
                    key: input.key.clone(),
                    scope: input.scope,
                    index: input.index,
                    variable: Some(variable.name.clone()),
                });
            }
        }
    }
    out
}

/// Render the command line exactly as it will run. Arguments that contain
/// whitespace or quotes are shown single-quoted so the user can see where each
/// one starts and ends — the string is for reading, never for re-parsing.
fn render_command(command: &str, args: &[String]) -> String {
    let mut out = String::from(command);
    for arg in args {
        out.push(' ');
        if arg.is_empty() || arg.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"') {
            out.push('\'');
            out.push_str(&arg.replace('\'', "'\\''"));
            out.push('\'');
        } else {
            out.push_str(arg);
        }
    }
    out
}

/// Collect the fields the form must render, each numbered within its own scope.
///
/// The ordinal is the identity: `key` is the publisher's label and a positional
/// argument may not have one, so numbering per scope in declaration order is
/// what keeps two hint-less positionals apart.
fn collect_inputs(server: &McpRegistryServer, candidate_id: Option<&str>) -> Vec<McpInstallInput> {
    let mut out = Vec::new();
    match candidate_id.and_then(parse_candidate_id) {
        Some(CandidateOrigin::Remote(index)) => {
            let Some(remote) = server.remotes.get(index) else {
                return out;
            };
            for (ordinal, header) in remote.headers.iter().enumerate() {
                out.push(make_input(
                    header.name.clone(),
                    McpInstallInputScope::Header,
                    ordinal,
                    &header.input,
                ));
            }
            for (ordinal, variable) in remote.variables.iter().enumerate() {
                out.push(make_input(
                    variable.name.clone(),
                    McpInstallInputScope::UrlVariable,
                    ordinal,
                    &variable.input,
                ));
            }
        }
        Some(CandidateOrigin::Package(index)) => {
            let Some(package) = server.packages.get(index) else {
                return out;
            };
            for (ordinal, env) in package.environment_variables.iter().enumerate() {
                out.push(make_input(
                    env.name.clone(),
                    McpInstallInputScope::Environment,
                    ordinal,
                    &env.input,
                ));
            }
            for (args, scope) in [
                (
                    &package.runtime_arguments,
                    McpInstallInputScope::RuntimeArgument,
                ),
                (
                    &package.package_arguments,
                    McpInstallInputScope::PackageArgument,
                ),
            ] {
                for (ordinal, arg) in args.iter().enumerate() {
                    // Positional arguments have no name; `valueHint` is the
                    // publisher's own label for them.
                    let key = arg
                        .name
                        .clone()
                        .or_else(|| arg.value_hint.clone())
                        .unwrap_or_else(|| "argument".to_string());
                    out.push(make_input(key, scope, ordinal, &arg.input));
                }
            }
        }
        None => {}
    }
    out
}

fn make_input(
    key: String,
    scope: McpInstallInputScope,
    index: usize,
    input: &McpInput,
) -> McpInstallInput {
    McpInstallInput {
        key,
        scope,
        index: u32::try_from(index).unwrap_or(u32::MAX),
        prefilled: prefill(input),
        must_ask: input.needs_user_value(),
        variables: seed_variables(input),
        input: input.clone(),
    }
}

/// Every `{curly_brace}` token in a template, in order, de-duplicated.
///
/// A token is `[A-Za-z0-9_.-]+` between braces; anything else is literal text,
/// and a `{` that does not open a token is skipped rather than swallowing the
/// rest of the string — `{a {B}}` still yields `B`.
pub(super) fn template_tokens(template: &str) -> Vec<String> {
    fn is_token_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
    }

    let bytes = template.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'{' {
            cursor += 1;
            continue;
        }
        let start = cursor + 1;
        let mut end = start;
        while end < bytes.len() && is_token_byte(bytes[end]) {
            end += 1;
        }
        if end > start && bytes.get(end) == Some(&b'}') {
            // Every byte in play is ASCII, so these are char boundaries.
            let token = &template[start..end];
            if !out.iter().any(|seen| seen == token) {
                out.push(token.to_string());
            }
            cursor = end + 1;
        } else {
            cursor += 1;
        }
    }
    out
}

/// The holes an input's pinned `value` leaves for the user to fill.
///
/// Tokens come from the template, not from the declared `variables` map: a
/// publisher may declare a variable the template never mentions (no field for
/// it) or leave a token undeclared (a field is still needed). Resolution is one
/// level deep and stays that way — see [`McpInstallInputVariable`].
fn seed_variables(input: &McpInput) -> Vec<McpInstallInputVariable> {
    let Some(template) = input.value.as_deref().filter(|v| !v.trim().is_empty()) else {
        return Vec::new();
    };
    template_tokens(template)
        .into_iter()
        .map(|name| {
            let variable =
                input
                    .variables
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| McpInputVariable {
                        is_required: true,
                        ..Default::default()
                    });
            McpInstallInputVariable {
                prefilled: variable_seed(&variable),
                variable,
                name,
            }
        })
        .collect()
}

/// [`prefill`]'s precedence, applied to one variable: anything required or
/// secret is blanked so the form has to ask, otherwise the publisher's
/// `default` — and a `choices` list with exactly one entry is not a choice.
fn variable_seed(variable: &McpInputVariable) -> String {
    if variable.is_required || variable.is_secret {
        return String::new();
    }
    variable
        .default
        .clone()
        .unwrap_or_else(|| match variable.choices.as_slice() {
            [only] => only.clone(),
            _ => String::new(),
        })
}

fn secret_policy(secret_keys: Vec<String>) -> McpSecretPolicy {
    // Recomputed rather than hardcoded: the claim "no secret reaches a
    // version-controlled file" is only true while every target's config lives
    // outside the project, and this is what makes that check fail loudly when
    // a project-scoped target is added.
    let writes_project_scoped_config = MCP_TOOL_IDS
        .iter()
        .filter_map(|tool_id| resolve_mcp_config_path(tool_id).ok())
        .any(|path| !path.is_absolute());

    McpSecretPolicy {
        storage: McpSecretStorage::UserLevelConfig,
        note: if secret_keys.is_empty() {
            "This server declares no secret inputs.".to_string()
        } else {
            "Secret values are stored in SkillStar's user-level MCP store and written into each enabled tool's user-level config file (under your home directory). SkillStar writes no project-scoped MCP config, so no secret reaches a version-controlled file. The values are not encrypted at rest: the agent tools read them as plain text, which is what makes the server work."
                .to_string()
        },
        secret_keys,
        writes_project_scoped_config,
    }
}
