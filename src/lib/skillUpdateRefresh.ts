export type SkillUpdateRefreshMode = "auto" | "1m" | "5m" | "10m" | "30m";

export const SKILL_UPDATE_REFRESH_STORAGE_KEY = "skillstar:skill-update-refresh";
export const SKILL_UPDATE_REFRESH_CHANGED_EVENT = "skillstar:skill-update-refresh-changed";

const DEFAULT_MODE: SkillUpdateRefreshMode = "auto";
const MINUTE_MS = 60 * 1000;

function normalizeSkillUpdateRefreshMode(
  value: string | null | undefined
): SkillUpdateRefreshMode {
  switch (value) {
    case "1m":
    case "5m":
    case "10m":
    case "30m":
    case "auto":
      return value;
    default:
      return DEFAULT_MODE;
  }
}

export function readSkillUpdateRefreshMode(): SkillUpdateRefreshMode {
  try {
    return normalizeSkillUpdateRefreshMode(
      localStorage.getItem(SKILL_UPDATE_REFRESH_STORAGE_KEY)
    );
  } catch {
    return DEFAULT_MODE;
  }
}

export function writeSkillUpdateRefreshMode(
  mode: SkillUpdateRefreshMode
): SkillUpdateRefreshMode {
  const normalized = normalizeSkillUpdateRefreshMode(mode);
  try {
    localStorage.setItem(SKILL_UPDATE_REFRESH_STORAGE_KEY, normalized);
    window.dispatchEvent(
      new CustomEvent<SkillUpdateRefreshMode>(
        SKILL_UPDATE_REFRESH_CHANGED_EVENT,
        { detail: normalized }
      )
    );
  } catch {
    // ignore storage write errors
  }
  return normalized;
}

export function resolveSkillUpdateRefreshIntervalMs(
  mode: SkillUpdateRefreshMode,
  isVisible = typeof document === "undefined" ? true : !document.hidden
): number {
  switch (mode) {
    case "1m":
      return MINUTE_MS;
    case "5m":
      return 5 * MINUTE_MS;
    case "10m":
      return 10 * MINUTE_MS;
    case "30m":
      return 30 * MINUTE_MS;
    case "auto":
    default:
      // Auto mode: faster checks while active, slower in background.
      return isVisible ? 5 * MINUTE_MS : 15 * MINUTE_MS;
  }
}

export function getSkillUpdateRefreshIntervalMs(): number {
  return resolveSkillUpdateRefreshIntervalMs(readSkillUpdateRefreshMode());
}

