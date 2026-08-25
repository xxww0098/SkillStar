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

use serde::{Deserialize, Serialize};
use skillstar_marketplace::{McpInput, McpRegistryServer};
use skillstar_models::mcp::{
    MCP_TOOL_IDS, McpServerEntry, resolve_mcp_config_path, resolve_runtime,
};
use ts_rs::TS;

use super::draft::{prefill, registry_to_entry_for, sanitize_key};
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

/// One field the install form has to render.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallInput.ts")]
pub struct McpInstallInput {
    /// Env var name, header name, url variable name, or the argument's flag /
    /// value hint.
    pub key: String,
    pub scope: McpInstallInputScope,
    /// The publisher's declared semantics, verbatim: `isRequired`, `isSecret`,
    /// `format`, `choices`, `default`, `placeholder`, the `value` template and
    /// its nested `variables`.
    pub input: McpInput,
    /// What the draft was prefilled with. Empty means the user must supply it.
    pub prefilled: String,
    /// `true` when the form must collect a value before install can proceed.
    pub must_ask: bool,
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
/// workspace dependency, used by `skillstar-github-auth` and `skillstar-sync`),
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

fn collect_inputs(server: &McpRegistryServer, candidate_id: Option<&str>) -> Vec<McpInstallInput> {
    let mut out = Vec::new();
    match candidate_id.and_then(parse_candidate_id) {
        Some(CandidateOrigin::Remote(index)) => {
            let Some(remote) = server.remotes.get(index) else {
                return out;
            };
            for header in &remote.headers {
                out.push(make_input(
                    header.name.clone(),
                    McpInstallInputScope::Header,
                    &header.input,
                ));
            }
            for variable in &remote.variables {
                out.push(make_input(
                    variable.name.clone(),
                    McpInstallInputScope::UrlVariable,
                    &variable.input,
                ));
            }
        }
        Some(CandidateOrigin::Package(index)) => {
            let Some(package) = server.packages.get(index) else {
                return out;
            };
            for env in &package.environment_variables {
                out.push(make_input(
                    env.name.clone(),
                    McpInstallInputScope::Environment,
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
                for arg in args {
                    // Positional arguments have no name; `valueHint` is the
                    // publisher's own label for them.
                    let key = arg
                        .name
                        .clone()
                        .or_else(|| arg.value_hint.clone())
                        .unwrap_or_else(|| "argument".to_string());
                    out.push(make_input(key, scope, &arg.input));
                }
            }
        }
        None => {}
    }
    out
}

fn make_input(key: String, scope: McpInstallInputScope, input: &McpInput) -> McpInstallInput {
    McpInstallInput {
        key,
        scope,
        prefilled: prefill(input),
        must_ask: input.needs_user_value(),
        input: input.clone(),
    }
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
