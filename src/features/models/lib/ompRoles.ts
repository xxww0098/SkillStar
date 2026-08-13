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
import { OMP_THINKING_LEVELS, type OmpThinkingLevel } from "../../../types";

export const OMP_TOOL_ID = "omp";

export interface OmpRoleDef {
  /** Role key written to `modelRoles.<id>`. */
  id: string;
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
  { id: "default", flag: "--model", key: "default", primary: true },
  { id: "smol", flag: "--smol", key: "smol", primary: true },
  { id: "slow", flag: "--slow", key: "slow", primary: true },
  { id: "plan", flag: "--plan", key: "plan", primary: true },
  { id: "vision", key: "vision", primary: false },
  { id: "designer", key: "designer", primary: false },
  { id: "commit", key: "commit", primary: false },
  { id: "tiny", key: "tiny", primary: false },
  { id: "task", key: "task", primary: false },
  { id: "advisor", key: "advisor", primary: false },
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
 * `shouldInheritDefaultBeforePriority`. Leaving these blank is safe — the UI
 * says so rather than nagging the user to fill them in.
 */
export const OMP_ROLES_INHERITING_DEFAULT = ["smol", "slow", "designer"];

export { OMP_THINKING_LEVELS };
export type { OmpThinkingLevel };

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
