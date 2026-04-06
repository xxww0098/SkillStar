/**
 * Skill update refresh — simplified to auto mode only.
 *
 * Frontend auto-refresh uses a visibility-aware interval:
 * - Foreground: every 5 minutes
 * - Background: every 15 minutes
 */

const MINUTE_MS = 60 * 1000;

export function resolveSkillUpdateRefreshIntervalMs(
  isVisible = typeof document === "undefined" ? true : !document.hidden,
): number {
  return isVisible ? 5 * MINUTE_MS : 15 * MINUTE_MS;
}

export function getSkillUpdateRefreshIntervalMs(): number {
  return resolveSkillUpdateRefreshIntervalMs();
}
