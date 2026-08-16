//! Role routing as a domain concept, not an OMP feature.
//!
//! ## Why this module exists
//!
//! v3 had the same idea in two incompatible places: OMP's `modelRoles` lived in
//! an untyped settings bag under an OMP-private `OmpRoleTarget`, and Claude's
//! tiered models lived in `provider.meta` as three loose strings. Neither could
//! see the other, so "which model plays which part" could never be asked
//! generically — the concept existed twice and generalised zero times.
//!
//! v4 already unified the *value* side: [`super::ModelRef`] is the one triple
//! every agent's writer consumes. This module supplies the missing half, the
//! *key* side: a canonical vocabulary of role ids, and a [`RoleDef`] row letting
//! each agent declare which of them it can actually project and under what name
//! its own config file spells them.
//!
//! ## The rule that keeps this honest
//!
//! **An agent declares a role only if its writer writes it.** A declared role is
//! a promise to the user that configuring it changes something on disk; a role
//! the writer silently ignores is exactly the defect this work package was sent
//! to fix. `every_declared_role_reaches_disk` in the agent registry enforces the
//! rule by assigning every declared role and grepping the bytes that come out.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

use super::binding::ModelRef;

/// Canonical role ids that carry cross-agent meaning.
///
/// These are constants, not an enum, because the on-disk schema is an open map.
/// A closed enum makes adding a role a three-file edit and drifts the first time
/// one of the three is missed (Continue's `summarize` role has been out of sync
/// with its enum for exactly this reason); an open map costs nothing and cannot
/// drift. Anything outside this list is an *extra* role: preserved verbatim,
/// written by whichever agent understands it, ignored by the rest.
pub const ROLE_DEFAULT: &str = "default";
/// Cheap, fast turns. OpenCode `small_model`, Crush `small`, Claude's Haiku
/// tier, OMP `smol`.
pub const ROLE_FAST: &str = "fast";
/// Planning mode.
pub const ROLE_PLAN: &str = "plan";
/// Multimodal turns. Capability-driven rather than preference-driven: sending an
/// image to a text-only model fails outright, it does not merely do worse.
pub const ROLE_VISION: &str = "vision";
/// Sub-agent fan-out.
pub const ROLE_SUBAGENT: &str = "subagent";

/// The canonical five, in presentation order.
pub const CANONICAL_ROLE_IDS: &[&str] = &[
    ROLE_DEFAULT,
    ROLE_FAST,
    ROLE_PLAN,
    ROLE_VISION,
    ROLE_SUBAGENT,
];

/// Whether a role constrains which models may fill it.
///
/// The filter belongs to the *role*, not to the picker: "vision" means "this
/// slot is used for image input", which is a fact about the slot that every
/// surface offering the slot should apply the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "RoleCapability.ts")]
pub enum RoleCapability {
    /// Any model the provider offers.
    Any,
    /// Must accept image input.
    Vision,
    /// Must support tool calls.
    ToolCall,
}

/// One role an agent supports.
///
/// Static data: the registry rows are `&'static` and hold function-free values,
/// so this is a plain table an agent's row points at. The frontend gets it
/// through `AgentDescriptorDto`, which is what stops the UI from carrying its
/// own copy of every agent's role list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleDef {
    /// Canonical role id (one of [`CANONICAL_ROLE_IDS`]) or an agent-private id.
    pub id: &'static str,
    /// What this agent's config file calls the role.
    ///
    /// OMP writes `modelRoles.smol`, Claude Code writes the env key
    /// `ANTHROPIC_DEFAULT_HAIKU_MODEL`, OpenCode writes `small_model` — three
    /// spellings of [`ROLE_FAST`]. Storing the agent's spelling here is what
    /// lets the writers stop hardcoding a translation table each.
    pub agent_key: &'static str,
    /// Laid out above the fold. Secondary roles fold into a disclosure — the
    /// layout Continue and SkillStar arrived at independently.
    pub primary: bool,
    /// Which role this one falls back to when unassigned, when the agent
    /// documents such a fallback.
    ///
    /// `None` does **not** mean "nothing happens": it means the agent resolves
    /// the role by its own rules, which SkillStar must not guess at. The UI says
    /// "inherits `default`" for `Some`, and "the agent decides" for `None`.
    /// Either way the user stops having to guess, which is the point (02 §9.3
    /// gap 3).
    ///
    /// Resolution happens at **read** time, never at write time: copying the
    /// default's value into the role on save makes "explicitly set to the same
    /// model" and "inherited" indistinguishable on disk, and the user can no
    /// longer get the original back by clearing the field.
    pub inherits: Option<&'static str>,
    /// Candidate filter for this slot.
    pub requires: RoleCapability,
}

impl RoleDef {
    /// A primary role with no capability filter and no documented fallback.
    pub const fn primary(id: &'static str, agent_key: &'static str) -> Self {
        Self {
            id,
            agent_key,
            primary: true,
            inherits: None,
            requires: RoleCapability::Any,
        }
    }

    /// A secondary (folded) role with no capability filter.
    pub const fn secondary(id: &'static str, agent_key: &'static str) -> Self {
        Self {
            id,
            agent_key,
            primary: false,
            inherits: None,
            requires: RoleCapability::Any,
        }
    }

    pub const fn inheriting(mut self, role: &'static str) -> Self {
        self.inherits = Some(role);
        self
    }

    pub const fn requiring(mut self, capability: RoleCapability) -> Self {
        self.requires = capability;
        self
    }
}

/// How a role's effective value was arrived at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleOrigin {
    /// The user assigned this role directly.
    Assigned,
    /// Unassigned; the value shown comes from the role named here.
    Inherited(&'static str),
}

/// Resolve a role to the model that will actually serve it, walking the
/// declared `inherits` chain.
///
/// Returns `None` when the role is unassigned and nothing in its chain is
/// assigned either — which is the case where SkillStar deliberately writes
/// nothing and lets the target tool apply its own default.
///
/// The chain walk is bounded by the number of declared roles, so a registry
/// that accidentally declared a cycle degrades to "unresolved" instead of
/// hanging. `registry_role_chains_terminate` rules the cycle out at build time
/// anyway; this is the belt to that pair of braces.
pub fn resolve_role<'a>(
    roles: &'a BTreeMap<String, ModelRef>,
    defs: &[RoleDef],
    role_id: &str,
) -> Option<(&'a ModelRef, RoleOrigin)> {
    if let Some(target) = roles.get(role_id).filter(|t| t.is_complete()) {
        return Some((target, RoleOrigin::Assigned));
    }
    let mut current = role_id;
    for _ in 0..defs.len() {
        let next = defs.iter().find(|d| d.id == current)?.inherits?;
        if let Some(target) = roles.get(next).filter(|t| t.is_complete()) {
            return Some((target, RoleOrigin::Inherited(next)));
        }
        current = next;
    }
    None
}

/// Why a configured role never reached disk.
///
/// The user typed something, saw it accepted, and nothing happened — v3's
/// `resolve_omp_roles` dropped roles on three separate conditions and told
/// nobody. Every drop now carries a reason back to the caller so the row that
/// vanished can be marked instead of silently disagreeing with the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleDropReason {
    /// The role's provider is not bound to this agent, so the agent's config
    /// has no block for it and the role would name a provider it cannot resolve.
    ProviderNotBound,
    /// The provider is bound but produced no block — no endpoint for the
    /// protocol this agent speaks.
    ProviderHasNoEndpoint,
    /// The provider id names a row that no longer exists in the store.
    ProviderMissing,
    /// The role has no model, so writing it would blank whatever the user
    /// already had on disk.
    NoModel,
    /// This agent has no key for that role. Extra roles are legitimate for OMP
    /// (its schema is an open map) and meaningless for Claude Code (its
    /// vocabulary is a fixed set of env keys).
    RoleNotSupported,
    /// The name would corrupt the target file's grammar (a `/` in an OMP role
    /// name breaks the `provider/model` value syntax).
    InvalidRoleName,
}

/// One role that was configured but not written, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedRole {
    /// The canonical role id as stored, not the agent's spelling — the UI has to
    /// match it back to the row the user edited.
    pub role: String,
    pub reason: RoleDropReason,
    /// The provider the role pointed at, so the message can name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

impl DroppedRole {
    pub fn new(role: impl Into<String>, reason: RoleDropReason) -> Self {
        Self {
            role: role.into(),
            reason,
            provider_id: None,
        }
    }

    pub fn for_provider(
        role: impl Into<String>,
        reason: RoleDropReason,
        provider_id: impl Into<String>,
    ) -> Self {
        Self {
            role: role.into(),
            reason,
            provider_id: Some(provider_id.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Vec<RoleDef> {
        vec![
            RoleDef::primary(ROLE_DEFAULT, "default"),
            RoleDef::primary(ROLE_FAST, "smol").inheriting(ROLE_DEFAULT),
            RoleDef::secondary(ROLE_SUBAGENT, "task").inheriting(ROLE_FAST),
            RoleDef::secondary(ROLE_VISION, "vision").requiring(RoleCapability::Vision),
        ]
    }

    fn roles(pairs: &[(&str, &str)]) -> BTreeMap<String, ModelRef> {
        pairs
            .iter()
            .map(|(role, model)| (role.to_string(), ModelRef::new("p1", *model)))
            .collect()
    }

    #[test]
    fn an_assigned_role_resolves_to_itself() {
        let roles = roles(&[(ROLE_DEFAULT, "big"), (ROLE_FAST, "small")]);
        let (target, origin) = resolve_role(&roles, &defs(), ROLE_FAST).unwrap();
        assert_eq!(target.model, "small");
        assert_eq!(origin, RoleOrigin::Assigned);
    }

    #[test]
    fn an_unassigned_role_walks_its_whole_chain() {
        // subagent → fast → default, with only `default` assigned.
        let roles = roles(&[(ROLE_DEFAULT, "big")]);
        let (target, origin) = resolve_role(&roles, &defs(), ROLE_SUBAGENT).unwrap();
        assert_eq!(target.model, "big");
        assert_eq!(
            origin,
            RoleOrigin::Inherited(ROLE_DEFAULT),
            "the UI must name where the value came from, not just that it has one"
        );
    }

    #[test]
    fn a_role_with_no_declared_fallback_stays_unresolved() {
        // `vision` inherits nothing: OMP resolves it by its own priority chain,
        // and pretending it falls back to `default` would state a fallback the
        // tool does not implement.
        let roles = roles(&[(ROLE_DEFAULT, "big")]);
        assert!(resolve_role(&roles, &defs(), ROLE_VISION).is_none());
    }

    #[test]
    fn an_incomplete_assignment_does_not_shadow_the_fallback() {
        // A row the user opened but never filled must not stop `fast` from
        // showing what would actually be used.
        let mut roles = roles(&[(ROLE_DEFAULT, "big")]);
        roles.insert(ROLE_FAST.to_string(), ModelRef::new("p1", "   "));
        let (target, origin) = resolve_role(&roles, &defs(), ROLE_FAST).unwrap();
        assert_eq!(target.model, "big");
        assert_eq!(origin, RoleOrigin::Inherited(ROLE_DEFAULT));
    }

    #[test]
    fn an_unknown_role_id_resolves_to_nothing_rather_than_guessing() {
        let roles = roles(&[(ROLE_DEFAULT, "big")]);
        assert!(resolve_role(&roles, &defs(), "commit").is_none());
    }
}
