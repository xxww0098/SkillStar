import type { Skill } from "../../../types";

/** The single meaning of "has an update": a remote skill with something to
 *  pull. Local skills have no upstream. Shared by the sidebar badge, the
 *  toolbar count, the updates filter and batch update so a count never
 *  promises skills the filter cannot show. */
export function hasPendingUpdate(skill: Pick<Skill, "update_available" | "skill_type">): boolean {
  return Boolean(skill.update_available) && skill.skill_type !== "local";
}
