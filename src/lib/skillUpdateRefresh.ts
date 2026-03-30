export type SkillUpdateRefreshMode = "auto" | "1m" | "5m" | "10m" | "30m" | "patrol";
export type PatrolInterval = "15s" | "30s" | "60s" | "120s";

export const SKILL_UPDATE_REFRESH_STORAGE_KEY = "skillstar:skill-update-refresh";
export const SKILL_UPDATE_REFRESH_CHANGED_EVENT = "skillstar:skill-update-refresh-changed";
export const PATROL_INTERVAL_STORAGE_KEY = "skillstar:patrol-interval";
export const PATROL_INTERVAL_CHANGED_EVENT = "skillstar:patrol-interval-changed";

const DEFAULT_MODE: SkillUpdateRefreshMode = "auto";
const DEFAULT_PATROL_INTERVAL: PatrolInterval = "30s";
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
    case "patrol":
      return value;
    default:
      return DEFAULT_MODE;
  }
}

function normalizePatrolInterval(
  value: string | null | undefined
): PatrolInterval {
  switch (value) {
    case "15s":
    case "30s":
    case "60s":
    case "120s":
      return value;
    default:
      return DEFAULT_PATROL_INTERVAL;
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

export function readPatrolInterval(): PatrolInterval {
  try {
    return normalizePatrolInterval(
      localStorage.getItem(PATROL_INTERVAL_STORAGE_KEY)
    );
  } catch {
    return DEFAULT_PATROL_INTERVAL;
  }
}

export function writePatrolInterval(
  interval: PatrolInterval
): PatrolInterval {
  const normalized = normalizePatrolInterval(interval);
  try {
    localStorage.setItem(PATROL_INTERVAL_STORAGE_KEY, normalized);
    window.dispatchEvent(
      new CustomEvent<PatrolInterval>(PATROL_INTERVAL_CHANGED_EVENT, {
        detail: normalized,
      })
    );
  } catch {
    // ignore storage write errors
  }
  return normalized;
}

export function resolvePatrolIntervalMs(
  interval: PatrolInterval,
  isVisible = typeof document === "undefined" ? true : !document.hidden
): number {
  const base: Record<PatrolInterval, number> = {
    "15s": 15_000,
    "30s": 30_000,
    "60s": 60_000,
    "120s": 120_000,
  };
  // 3× slower when window is not visible
  return isVisible ? base[interval] : base[interval] * 3;
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
    case "patrol":
      // Patrol mode uses its own per-skill timer; disable burst interval.
      return Infinity;
    case "auto":
    default:
      // Auto mode: faster checks while active, slower in background.
      return isVisible ? 5 * MINUTE_MS : 15 * MINUTE_MS;
  }
}

export function getSkillUpdateRefreshIntervalMs(): number {
  return resolveSkillUpdateRefreshIntervalMs(readSkillUpdateRefreshMode());
}
