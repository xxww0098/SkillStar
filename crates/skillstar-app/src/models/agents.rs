//! The agent registry, projected for the renderer.
//!
//! ## Why a projection and not a derive
//!
//! `AgentSpec` holds three function pointers. It cannot derive `TS`, and it
//! should not: the writers are exactly the part of an agent the frontend has no
//! business knowing about. What the frontend needs is the *declarative* half —
//! id, name, binding kind, required wire protocol, config files, and the role
//! list — and that half is what crosses.
//!
//! ## Why this exists at all
//!
//! Every fact in here was, until now, hand-copied into `agentRegistry.ts` as
//! well: the tool-id union, the display names, the `single`/`multi` kind, the
//! required URL field, the config paths, and — for OMP — a second copy of the
//! role list in `ompRoles.ts`. Two tables, one truth, kept in step by a test
//! that pins string literals against each other. Serving the registry over IPC
//! is how the second table stops needing to exist.
//!
//! Per D-034 the `From` impls destructure completely, so a new column on
//! `AgentSpec` fails to compile here until someone decides whether the renderer
//! should see it.

use serde::{Deserialize, Serialize};
use skillstar_models::providers::{RoleCapability, RoleDef};
use skillstar_models::tool_sync::{AgentConfigFileSpec, AgentKind, AgentSpec, agent_specs};
use ts_rs::TS;

/// How an agent loads providers, as the renderer spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "AgentKindDto.ts")]
pub enum AgentKindDto {
    /// One global credential block; binding replaces it wholesale.
    Single,
    /// The config natively lists providers plus an active pointer.
    Multi,
}

/// The wire protocol an agent speaks, as the renderer spells it.
///
/// The renderer uses this to decide whether a provider row can be bound at all,
/// which is why `openai_responses` is its own value rather than being folded in
/// with `openai_chat`: a chat-only relay is bindable to OpenCode and not to
/// Codex, and that difference is the whole reason the field exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "RequiredWireDto.ts")]
pub enum RequiredWireDto {
    OpenaiChat,
    OpenaiResponses,
    AnthropicMessages,
}

/// One role an agent supports, for the shared role panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "RoleDefDto.ts")]
pub struct RoleDefDto {
    /// Canonical role id — the key under which the assignment is stored.
    pub id: String,
    /// What the agent's own config file calls it. Shown in the write preview so
    /// the user can see which line of which file a row controls.
    pub agent_key: String,
    /// Laid out above the fold; the rest fold into a disclosure.
    pub primary: bool,
    /// The role whose model is used when this one is unassigned, or `null` when
    /// the agent resolves it by rules SkillStar does not model.
    ///
    /// The panel renders this as the empty row's placeholder — "inherits
    /// default" or "the agent decides" — which is the piece 02 §9.3 gap 3 found
    /// missing: the knowledge existed and was used only to suppress a nag.
    pub inherits: Option<String>,
    /// Candidate filter for the slot.
    pub requires: RoleCapabilityDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "RoleCapabilityDto.ts")]
pub enum RoleCapabilityDto {
    Any,
    Vision,
    ToolCall,
}

/// One config file the agent owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "AgentConfigFileDto.ts")]
pub struct AgentConfigFileDto {
    pub file_id: String,
    pub label: String,
    pub format: String,
}

/// One agent, as the renderer needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "AgentDescriptorDto.ts")]
pub struct AgentDescriptorDto {
    pub id: String,
    pub display_name: String,
    pub kind: AgentKindDto,
    pub required_wire: RequiredWireDto,
    /// Empty for agents with no role concept — the signal to render a single
    /// provider+model choice rather than a role panel.
    pub roles: Vec<RoleDefDto>,
    pub config_files: Vec<AgentConfigFileDto>,
}

impl From<&RoleDef> for RoleDefDto {
    fn from(def: &RoleDef) -> Self {
        let RoleDef {
            id,
            agent_key,
            primary,
            inherits,
            requires,
        } = *def;
        Self {
            id: id.to_string(),
            agent_key: agent_key.to_string(),
            primary,
            inherits: inherits.map(str::to_string),
            requires: match requires {
                RoleCapability::Any => RoleCapabilityDto::Any,
                RoleCapability::Vision => RoleCapabilityDto::Vision,
                RoleCapability::ToolCall => RoleCapabilityDto::ToolCall,
            },
        }
    }
}

impl From<&AgentConfigFileSpec> for AgentConfigFileDto {
    fn from(file: &AgentConfigFileSpec) -> Self {
        // Not destructured: `resolve` is a function pointer and
        // `default_content` is an on-disk skeleton, neither of which the
        // renderer has any use for. The path itself already reaches the UI
        // through `list_tool_config_files`, which resolves it against the real
        // home.
        Self {
            file_id: file.file_id.to_string(),
            label: file.label.to_string(),
            format: file.format.to_string(),
        }
    }
}

impl From<&'static AgentSpec> for AgentDescriptorDto {
    fn from(spec: &'static AgentSpec) -> Self {
        Self {
            id: spec.id.to_string(),
            display_name: spec.display_name.to_string(),
            kind: match spec.kind {
                AgentKind::Single => AgentKindDto::Single,
                AgentKind::Multi => AgentKindDto::Multi,
            },
            required_wire: match spec.required_wire {
                skillstar_models::providers::RequiredWire::OpenaiChat => {
                    RequiredWireDto::OpenaiChat
                }
                skillstar_models::providers::RequiredWire::OpenaiResponses => {
                    RequiredWireDto::OpenaiResponses
                }
                skillstar_models::providers::RequiredWire::AnthropicMessages => {
                    RequiredWireDto::AnthropicMessages
                }
            },
            roles: spec.roles.iter().map(RoleDefDto::from).collect(),
            config_files: spec.files.iter().map(AgentConfigFileDto::from).collect(),
        }
    }
}

/// Every agent, in the registry's presentation order.
pub fn agent_descriptors() -> Vec<AgentDescriptorDto> {
    agent_specs().iter().map(AgentDescriptorDto::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_row_is_projected_in_order() {
        let descriptors = agent_descriptors();
        assert_eq!(descriptors.len(), agent_specs().len());
        for (dto, spec) in descriptors.iter().zip(agent_specs()) {
            assert_eq!(dto.id, spec.id);
            assert_eq!(dto.display_name, spec.display_name);
            assert_eq!(dto.roles.len(), spec.roles.len());
            assert_eq!(dto.config_files.len(), spec.files.len());
        }
    }

    /// The point of serving this over IPC: the frontend can render OMP's role
    /// panel, Claude's tier panel and Pi's plain picker from one payload.
    #[test]
    fn the_three_role_tiers_survive_the_projection() {
        let by_id = |id: &str| {
            agent_descriptors()
                .into_iter()
                .find(|d| d.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
        };
        assert!(by_id("pi").roles.is_empty());
        assert_eq!(by_id("claude-code").roles.len(), 5);
        assert_eq!(by_id("omp").roles.len(), 10);
    }

    /// Whether the renderer can *say* what an unfilled row will do depends on
    /// this field surviving the trip.
    #[test]
    fn declared_fallbacks_reach_the_renderer() {
        let omp = agent_descriptors()
            .into_iter()
            .find(|d| d.id == "omp")
            .unwrap();
        let fast = omp.roles.iter().find(|r| r.id == "fast").unwrap();
        assert_eq!(fast.inherits.as_deref(), Some("default"));
        assert_eq!(fast.agent_key, "smol");

        // And a role with no documented fallback says so rather than claiming
        // one — "the agent decides" and "inherits default" are different
        // sentences and only one of them is true here.
        let vision = omp.roles.iter().find(|r| r.id == "vision").unwrap();
        assert_eq!(vision.inherits, None);
        assert_eq!(vision.requires, RoleCapabilityDto::Vision);
    }
}
