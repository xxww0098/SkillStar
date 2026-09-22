import type { Skill } from "../../../types";

/** The single meaning of "has an update": a remote skill with content to
 *  pull. Local skills have no upstream. Used by the update-all CTA, batch
 *  update and selection-bar update so nothing unupdatable is ever pushed
 *  through the update path. */
export function hasPendingUpdate(skill: Pick<Skill, "update_available" | "skill_type">): boolean {
  return Boolean(skill.update_available) && skill.skill_type !== "local";
}

/** The single meaning of "needs attention": a remote skill the user should
 *  act on — a content update, or an upstream that removed / renamed it. The
 *  sidebar badge, the toolbar chip count and the toolbar filter all read this
 *  one predicate, so the number the badge promises is exactly what the filter
 *  shows (removed and renamed skills carry their own migrate / resolve
 *  actions on the card). Update-all stays content-only: see
 *  hasPendingUpdate. */
export function needsAttention(skill: Pick<Skill, "update_available" | "skill_type" | "upstream_change">): boolean {
  return skill.skill_type !== "local" && (hasPendingUpdate(skill) || Boolean(skill.upstream_change));
}
