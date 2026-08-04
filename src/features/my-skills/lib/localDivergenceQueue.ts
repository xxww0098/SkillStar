import type { ResolveSkillUpdateResult, SkillUpdateBlocked } from "../../../types";

/** Replace only the resolved checkout's blockers; unrelated update groups stay queued. */
export function reconcileBlockedUpdates(
  current: SkillUpdateBlocked[],
  resolvedName: string,
  result: ResolveSkillUpdateResult,
): SkillUpdateBlocked[] {
  const replacementNames = new Set(result.remaining_blocked.map((blocked) => blocked.name));
  const resolvedNames = new Set([resolvedName]);
  if (result.update) {
    resolvedNames.add(result.update.skill.name);
    for (const sibling of result.update.siblings_cleared) resolvedNames.add(sibling);
  }

  const unrelated = current.filter(
    (blocked) => !resolvedNames.has(blocked.name) && !replacementNames.has(blocked.name),
  );
  return [...result.remaining_blocked, ...unrelated];
}
