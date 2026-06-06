import type { TFunction } from "i18next";
import type { AuthMode } from "../types";

// -- Antigravity model name canonicalization --
// Maps raw API model names/constants to user-friendly display names.
// Synced from cockpit-tools src/utils/antigravityModels.ts

interface CanonicalModel {
  id: string;
  displayName: string;
  aliases: string[];
}

const ANTIGRAVITY_CANONICAL_MODELS: CanonicalModel[] = [
  {
    id: "gemini-3.1-pro-high",
    displayName: "Gemini 3.1 Pro (High)",
    aliases: ["gemini-3-pro-high", "MODEL_PLACEHOLDER_M8", "MODEL_PLACEHOLDER_M37"],
  },
  {
    id: "gemini-3.1-pro-low",
    displayName: "Gemini 3.1 Pro (Low)",
    aliases: ["gemini-3-pro-low", "MODEL_PLACEHOLDER_M7", "MODEL_PLACEHOLDER_M36"],
  },
  {
    id: "gemini-3-flash",
    displayName: "Gemini 3 Flash",
    aliases: ["MODEL_PLACEHOLDER_M18"],
  },
  {
    id: "claude-sonnet-4-6",
    displayName: "Claude Sonnet 4.6 (Thinking)",
    aliases: ["claude-sonnet-4-6-thinking", "claude-sonnet-4-5", "claude-sonnet-4-5-thinking", "MODEL_PLACEHOLDER_M35"],
  },
  {
    id: "claude-opus-4-6-thinking",
    displayName: "Claude Opus 4.6 (Thinking)",
    aliases: ["claude-opus-4-6", "claude-opus-4-5-thinking", "MODEL_PLACEHOLDER_M12", "MODEL_PLACEHOLDER_M26"],
  },
  {
    id: "gpt-oss-120b-medium",
    displayName: "GPT-OSS 120B (Medium)",
    aliases: ["MODEL_OPENAI_GPT_OSS_120B_MEDIUM"],
  },
];

function normalizeModelKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

const ANTIGRAVITY_ALIAS_MAP: Map<string, string> = (() => {
  const map = new Map<string, string>();
  for (const model of ANTIGRAVITY_CANONICAL_MODELS) {
    const keys = [model.id, model.displayName, ...model.aliases];
    for (const key of keys) {
      const nk = normalizeModelKey(key);
      if (nk && !map.has(nk)) {
        map.set(nk, model.displayName);
      }
    }
  }
  return map;
})();

/**
 * Convert a raw Antigravity model name (API name or placeholder constant)
 * into a human-readable display name.
 * Falls back to the original value if no canonical mapping is found.
 */
export function canonicalizeAntigravityModelName(raw: string): string {
  const key = normalizeModelKey(raw);
  return ANTIGRAVITY_ALIAS_MAP.get(key) ?? raw;
}

const CATEGORY_KEYS: Record<string, string> = {
  "Auto + Composer": "usage.categoryAutoComposer",
  API: "usage.categoryApi",
};

const WINDOW_KEYS: Record<string, string> = {
  "5h": "usage.window5h",
  "7d": "usage.window7d",
  "30d": "usage.window30d",
  Total: "usage.windowTotal",
  "Monthly credits": "usage.defaultPeriod",
  模型额度: "usage.windowModelQuota",
  "Model quota": "usage.windowModelQuota",
  本月: "usage.defaultPeriod",
};

export function localizeCategoryLabel(label: string, t: TFunction): string {
  const key = CATEGORY_KEYS[label];
  return key ? t(key) : label;
}

export function localizeWindowLabel(label: string, t: TFunction): string {
  const stripped = label.replace(/\s+\$.*$/, "").trim();
  const key = WINDOW_KEYS[stripped] ?? WINDOW_KEYS[label];
  return key ? t(key) : label;
}

export function authModeLabel(mode: AuthMode, t: TFunction): string {
  switch (mode) {
    case "o-auth":
      return t("usage.authBadgeOAuth");
    case "api-key":
      return t("usage.authBadgeApiKey");
    case "manual":
      return t("usage.authBadgeManual");
    case "cookie":
      return t("usage.authBadgeCookie");
  }
}

/** True when window looks like Cursor-style monetary included usage (cents + breakdown). */
export function isMonetaryQuota(window: { label?: string; total: number | null; breakdown?: unknown[] }): boolean {
  if (window.label === "Monthly credits") return window.total != null && window.total > 0;
  return window.total != null && window.total > 100 && (window.breakdown?.length ?? 0) > 0;
}

/** Token/credit style window with absolute used + total (not rate-limit % only). */
export function isAbsoluteQuotaWindow(window: {
  label: string;
  used: number;
  total: number | null;
  breakdown?: unknown[];
}): boolean {
  if (isMonetaryQuota(window)) return false;
  if (window.label === "5h" || window.label === "7d") return false;
  const total = window.total;
  if (total == null || total <= 0) return false;
  if (total === 100 && window.used <= 100) return false;
  return true;
}

export function formatQuotaNumber(n: number): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (Math.abs(n) >= 10_000) return `${(n / 10_000).toFixed(1)}万`;
  if (Math.abs(n) >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(Math.round(n));
}

export function formatRelativeSync(epoch: number, t: TFunction): string {
  if (!epoch || epoch <= 0) return t("usage.lastSyncedNever");
  const diff = Math.floor(Date.now() / 1000) - epoch;
  if (diff < 60) return t("usage.lastSynced", { time: "<1m" });
  const mins = Math.floor(diff / 60);
  if (mins < 60) return t("usage.lastSynced", { time: `${mins}m` });
  const hours = Math.floor(mins / 60);
  if (hours < 48) return t("usage.lastSynced", { time: `${hours}h` });
  const days = Math.floor(hours / 24);
  return t("usage.lastSynced", { time: `${days}d` });
}

function windowUsedPercent(window: { used: number; total: number | null; percent: number | null }): number {
  if (window.percent != null) return window.percent;
  if (window.total && window.total > 0) return Math.round((window.used / window.total) * 100);
  return 0;
}

export type PrimaryResetSource = "monthly" | "hourly" | "weekly";

export interface PrimaryResetInfo {
  resetAt: number;
  usedPercent: number;
  mode: ResetUrgencyMode;
  source: PrimaryResetSource;
}

/** Pick the billing-cycle reset first, then rate-limit windows. */
export function getPrimaryResetInfo(
  usage: {
    monthly: {
      reset_at: number | null;
      used: number;
      total: number | null;
      percent: number | null;
      breakdown?: unknown[];
    } | null;
    hourly: { reset_at: number | null; used: number; total: number | null; percent: number | null } | null;
    weekly: { reset_at: number | null; used: number; total: number | null; percent: number | null } | null;
  } | null,
): PrimaryResetInfo | null {
  if (!usage) return null;

  if (usage.monthly?.reset_at) {
    return {
      resetAt: usage.monthly.reset_at,
      usedPercent: windowUsedPercent(usage.monthly),
      mode: "billing",
      source: "monthly",
    };
  }
  if (usage.hourly?.reset_at) {
    return {
      resetAt: usage.hourly.reset_at,
      usedPercent: windowUsedPercent(usage.hourly),
      mode: "rateLimit",
      source: "hourly",
    };
  }
  if (usage.weekly?.reset_at) {
    return {
      resetAt: usage.weekly.reset_at,
      usedPercent: windowUsedPercent(usage.weekly),
      mode: "rateLimit",
      source: "weekly",
    };
  }
  return null;
}

export function formatUsdCents(cents: number): string {
  if (cents % 100 === 0) return `$${Math.round(cents / 100)}`;
  return `$${(cents / 100).toFixed(2)}`;
}

export type ResetUrgency = "now" | "critical" | "urgent" | "soon" | "normal" | "relaxed";

export type ResetUrgencyMode = "billing" | "rateLimit";

export interface ResetStateOptions {
  /** 0-100, share of quota already consumed */
  usedPercent?: number;
  mode?: ResetUrgencyMode;
}

export interface ResetState {
  relative: string;
  urgency: ResetUrgency;
  diffSec: number;
}

function formatRelativeTime(diffSec: number): string {
  const days = Math.floor(diffSec / 86_400);
  const hours = Math.floor((diffSec % 86_400) / 3_600);
  const minutes = Math.floor((diffSec % 3_600) / 60);
  if (days > 0) return `${days}d${hours > 0 ? `${hours}h` : ""}`;
  if (hours > 0) return `${hours}h${minutes > 0 ? `${minutes}m` : ""}`;
  return `${Math.max(1, minutes)}m`;
}

/** Billing cycle: reset soon + lots of unused included quota = higher urgency. */
function billingResetPressure(diffSec: number, usedPercent: number): number {
  const days = diffSec / 86_400;
  const remainingPct = Math.max(0, 100 - usedPercent);

  let timeScore: number;
  if (days <= 1) timeScore = 96;
  else if (days <= 3) timeScore = 88;
  else if (days <= 5) timeScore = 74;
  else if (days <= 7) timeScore = 58;
  else if (days <= 14) timeScore = 32;
  else timeScore = 10;

  let wasteScore: number;
  if (remainingPct >= 70) wasteScore = 92;
  else if (remainingPct >= 50) wasteScore = 78;
  else if (remainingPct >= 35) wasteScore = 64;
  else if (remainingPct >= 20) wasteScore = 42;
  else wasteScore = 12;

  const wasteWeight = days <= 3 ? 0.58 : days <= 7 ? 0.48 : days <= 14 ? 0.28 : 0.12;
  return timeScore * (1 - wasteWeight) + wasteScore * wasteWeight;
}

/** Rate limit: reset soon + quota almost exhausted = higher urgency. */
function rateLimitResetPressure(diffSec: number, usedPercent: number): number {
  const hours = diffSec / 3_600;

  let timeScore: number;
  if (hours <= 0.5) timeScore = 96;
  else if (hours <= 2) timeScore = 82;
  else if (hours <= 6) timeScore = 62;
  else if (hours <= 24) timeScore = 40;
  else timeScore = 15;

  let usageScore: number;
  if (usedPercent >= 90) usageScore = 95;
  else if (usedPercent >= 75) usageScore = 78;
  else if (usedPercent >= 60) usageScore = 55;
  else if (usedPercent >= 40) usageScore = 32;
  else usageScore = 10;

  const usageWeight = hours <= 6 ? 0.55 : hours <= 24 ? 0.4 : 0.2;
  return timeScore * (1 - usageWeight) + usageScore * usageWeight;
}

function scoreToUrgency(score: number): ResetUrgency {
  if (score >= 82) return "critical";
  if (score >= 66) return "urgent";
  if (score >= 48) return "soon";
  if (score >= 28) return "normal";
  return "relaxed";
}

export function getResetState(epoch: number, options: ResetStateOptions = {}): ResetState {
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  if (diff <= 0) return { relative: "", urgency: "now", diffSec: diff };

  const mode = options.mode ?? "billing";
  const usedPercent = options.usedPercent ?? 0;
  const score =
    mode === "billing" ? billingResetPressure(diff, usedPercent) : rateLimitResetPressure(diff, usedPercent);

  let adjustedScore = score;
  if (mode === "billing") {
    const days = diff / 86_400;
    if (days <= 1) adjustedScore = Math.max(adjustedScore, 84);
    else if (days <= 3) adjustedScore = Math.max(adjustedScore, 68);
  }

  return {
    relative: formatRelativeTime(diff),
    urgency: scoreToUrgency(adjustedScore),
    diffSec: diff,
  };
}

export function isPriorityResetUrgency(urgency: ResetUrgency): boolean {
  return urgency === "soon" || urgency === "urgent" || urgency === "critical";
}

export function isPriorityReset(resetAt: number, usedPercent = 0, mode: ResetUrgencyMode = "billing"): boolean {
  return isPriorityResetUrgency(getResetState(resetAt, { usedPercent, mode }).urgency);
}

export function pickResetTone(urgency: ResetUrgency): { badge: string; text: string } {
  switch (urgency) {
    case "now":
      return {
        badge: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 ring-1 ring-emerald-500/30",
        text: "text-emerald-600 dark:text-emerald-400",
      };
    case "critical":
      return {
        badge: "bg-red-500/20 text-red-500 ring-1 ring-red-500/50 font-semibold animate-pulse",
        text: "text-red-500 font-semibold animate-pulse",
      };
    case "urgent":
      return {
        badge: "bg-red-500/15 text-red-400 ring-1 ring-red-500/35 font-semibold",
        text: "text-red-400 font-semibold",
      };
    case "soon":
      return {
        badge: "bg-orange-500/15 text-orange-500 ring-1 ring-orange-500/35 font-medium",
        text: "text-orange-500 font-medium",
      };
    case "normal":
      return {
        badge: "bg-amber-500/15 text-amber-600 dark:text-amber-400 ring-1 ring-amber-500/25",
        text: "text-amber-600 dark:text-amber-400",
      };
    case "relaxed":
      return {
        badge: "bg-muted/50 text-muted-foreground ring-1 ring-border/40",
        text: "text-muted-foreground/70",
      };
  }
}

/** Remaining quota tone — near reset, unused quota is waste risk, not a "healthy surplus". */
export function pickRemainingTone(
  remainingPct: number,
  resetAt: number | null | undefined,
  usedPercent: number,
): { text: string; bar: string } {
  if (!resetAt) {
    return pickUsageTone(remainingPct);
  }

  const urgency = getResetState(resetAt, { usedPercent, mode: "billing" }).urgency;
  if (urgency === "critical" || urgency === "urgent") {
    return { text: "text-red-500", bar: "bg-red-500" };
  }
  if (urgency === "soon") {
    return { text: "text-orange-500", bar: "bg-orange-500" };
  }
  if (urgency === "normal") {
    return { text: "text-amber-600 dark:text-amber-400", bar: "bg-amber-500" };
  }
  return pickUsageTone(remainingPct);
}

function pickUsageTone(remainingPct: number): { text: string; bar: string } {
  if (remainingPct < 5) return { text: "text-red-500", bar: "bg-red-500" };
  if (remainingPct < 20) return { text: "text-orange-500", bar: "bg-orange-500" };
  if (remainingPct < 40) return { text: "text-amber-600 dark:text-amber-400", bar: "bg-amber-500" };
  return { text: "text-emerald-600 dark:text-emerald-400", bar: "bg-emerald-500" };
}

export function pickConsumedTone(usedPercent: number): { text: string; bar: string } {
  return pickUsageTone(Math.max(0, 100 - usedPercent));
}

export function pickRateLimitUsageTone(usedPercent: number): { text: string; bar: string } {
  if (usedPercent >= 90) return { text: "text-red-500", bar: "bg-red-500" };
  if (usedPercent >= 75) return { text: "text-orange-500", bar: "bg-orange-500" };
  if (usedPercent >= 55) return { text: "text-amber-600 dark:text-amber-400", bar: "bg-amber-500" };
  return { text: "text-muted-foreground", bar: "bg-muted-foreground/35" };
}

export function pickUsedBarTone(usedPercent: number, resetAt: number | null | undefined): string {
  if (!resetAt) {
    return pickUsageTone(Math.max(0, 100 - usedPercent)).bar;
  }

  const urgency = getResetState(resetAt, { usedPercent, mode: "billing" }).urgency;
  if (urgency === "critical" || urgency === "urgent") return "bg-red-500";
  if (urgency === "soon") return "bg-orange-500";
  if (urgency === "normal") return "bg-amber-500";
  if (usedPercent >= 85) return "bg-emerald-500";
  if (usedPercent >= 60) return "bg-emerald-500/80";
  return "bg-muted-foreground/35";
}
