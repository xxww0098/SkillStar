/**
 * OMP (Oh My Pi) model roles — the vocabulary its config.yml `modelRoles` map
 * uses to route each request to a different model.
 *
 * This mirrors `OMP_MODEL_ROLES` / `OMP_THINKING_LEVELS` in
 * `crates/skillstar-models/src/tool_sync/types.rs`; the Rust registry is the SSOT
 * for what gets written to disk, this file only decides what the UI surfaces and
 * in what order. `ompRoles.test.ts` pins the two lists together.
 *
 * `primary` marks the four roles OMP exposes as CLI flags — the ones worth
 * configuring first. The rest are shown behind a "more roles" disclosure.
 */
import { OMP_THINKING_LEVELS, type ModelCatalogEntry, type OmpThinkingLevel } from "../../../types";
import type { Effort } from "../../../types/generated/Effort";
import type { Reasoning } from "../../../types/generated/Reasoning";

export const OMP_TOOL_ID = "omp";

export interface OmpRoleDef {
  /**
   * Canonical role id — the key the assignment is **stored** under, shared with
   * every other agent.
   *
   * Not what `config.yml` calls it. The store speaks canonical ids since v4
   * (`smol` migrated to `fast`), and a panel keyed by the on-disk spelling
   * would read an empty slot for every migrated user while quietly writing a
   * second, duplicate role beside theirs.
   */
  id: string;
  /** What `modelRoles.<key>` calls it on disk. Mirrors `RoleDef.agent_key`. */
  agentKey: string;
  /** The CLI flag that overrides this role for one run, when one exists. */
  flag?: string;
  /** i18n key suffix under `models.ompRoles.<key>`. */
  key: string;
  /** Shown above the fold. */
  primary: boolean;
}

/**
 * Order matches OMP's own `MODEL_ROLE_IDS`, with the flag-backed roles first so
 * the panel reads top-down as "normal → cheap → deep → planning".
 */
export const OMP_ROLE_DEFS: OmpRoleDef[] = [
  { id: "default", agentKey: "default", flag: "--model", key: "default", primary: true },
  { id: "fast", agentKey: "smol", flag: "--smol", key: "smol", primary: true },
  { id: "slow", agentKey: "slow", flag: "--slow", key: "slow", primary: true },
  { id: "plan", agentKey: "plan", flag: "--plan", key: "plan", primary: true },
  { id: "vision", agentKey: "vision", key: "vision", primary: false },
  { id: "designer", agentKey: "designer", key: "designer", primary: false },
  { id: "commit", agentKey: "commit", key: "commit", primary: false },
  { id: "tiny", agentKey: "tiny", key: "tiny", primary: false },
  { id: "subagent", agentKey: "task", key: "task", primary: false },
  { id: "advisor", agentKey: "advisor", key: "advisor", primary: false },
];

export const OMP_PRIMARY_ROLES = OMP_ROLE_DEFS.filter((r) => r.primary);
export const OMP_SECONDARY_ROLES = OMP_ROLE_DEFS.filter((r) => !r.primary);

/**
 * Roles OMP cycles with Ctrl+P by default (`cycleOrder` defaults to
 * `["smol","default","slow"]`). Surfacing this stops users wondering why a
 * configured `plan` model never shows up in the cycle.
 */
export const OMP_DEFAULT_CYCLE_ORDER = ["smol", "default", "slow"];

/**
 * Roles that fall back to `default` when unassigned, per OMP's
 * `shouldInheritDefaultBeforePriority`.
 *
 * Superseded by `RoleDefDto.inherits`, which the backend registry serves for
 * every agent rather than just this one. Kept as the value used while the
 * descriptor query is still in flight, so the panel never briefly claims a role
 * has no fallback when it does.
 */
export const OMP_ROLES_INHERITING_DEFAULT = ["fast", "slow", "designer"];

export { OMP_THINKING_LEVELS };
export type { OmpThinkingLevel };

/**
 * The thinking levels worth offering for one model.
 *
 * Mirrors `omp_thinking_levels_for` in
 * `crates/skillstar-models/src/tool_sync/types.rs`, which is the SSOT — the same
 * narrowing has to happen on the write side, and the picker exists to stop the
 * user choosing something the writer will then quietly not honour.
 *
 * `null` means the catalogue said nothing about this model, which is not the
 * same as "it has no reasoning mode": narrowing on absent data would hide levels
 * that work. Only an explicit capability narrows.
 */
export function ompThinkingLevelsFor(reasoning: Reasoning | null | undefined): OmpThinkingLevel[] {
  if (!reasoning) return [...OMP_THINKING_LEVELS];

  const levels: OmpThinkingLevel[] = ["inherit"];
  switch (reasoning.kind) {
    case "none":
      break;
    case "toggle":
      if (reasoning.can_disable) levels.push("off");
      levels.push("auto");
      break;
    case "effort": {
      if (reasoning.can_disable) levels.push("off");
      // Ordered by the grammar rather than by the catalogue's order, so the
      // picker reads low → high however the source listed them.
      for (const level of OMP_THINKING_LEVELS) {
        if (reasoning.values.includes(level as Effort) && !levels.includes(level)) levels.push(level);
      }
      levels.push("auto");
      break;
    }
    case "budget_tokens":
      // OMP's `:suffix` grammar has no place for a token count, so a budget
      // model gets the tiers OMP does map onto budgets.
      levels.push("off", "low", "medium", "high", "max", "auto");
      break;
  }
  return levels;
}

/** The reasoning capability recorded for one model id, if the catalogue has it. */
export function modelReasoning(catalog: ModelCatalogEntry[], modelId: string): Reasoning | null {
  if (!modelId.trim()) return null;
  return catalog.find((entry) => entry.id === modelId.trim())?.reasoning ?? null;
}

/** `inherit` and an unset suffix mean the same thing to OMP. */
export function isDefaultThinking(level: OmpThinkingLevel | null | undefined): boolean {
  return !level || level === "inherit";
}

/**
 * Preview of the value SkillStar will write for a role, e.g.
 * `skillstar_a1b2c3d4/deepseek-chat:xhigh`. Returns `null` for an incomplete
 * assignment, which is exactly when nothing gets written.
 *
 * The managed key mirrors `skillstar_managed_key` in `multi_provider.rs`:
 * first 8 chars of the provider id, lowercased, non-alphanumerics to `_`.
 * Rust counts `char`s (Unicode scalar values), so spread before slicing — a
 * plain `.slice(8)` counts UTF-16 code units and would disagree on astral
 * characters.
 */
export function previewRoleValue(providerId: string, model: string, thinking?: OmpThinkingLevel | null): string | null {
  if (!providerId || !model.trim()) return null;
  const key = `skillstar_${[...providerId]
    .slice(0, 8)
    .join("")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "_")}`;
  const suffix = isDefaultThinking(thinking) ? "" : `:${thinking}`;
  return `${key}/${model.trim()}${suffix}`;
}

/**
 * The `omp` invocation matching the current role map, for the "copy command"
 * affordance. Only flag-backed roles can be expressed on the command line.
 */
export function buildOmpLaunchCommand(models: Record<string, string | undefined>): string {
  const parts = ["omp"];
  for (const role of OMP_PRIMARY_ROLES) {
    const model = models[role.id]?.trim();
    if (model && role.flag) parts.push(`${role.flag} "${model}"`);
  }
  return parts.join(" ");
}
