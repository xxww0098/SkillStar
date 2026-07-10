import type { Subscription } from "../types";

/**
 * Whether a subscription has any auto-fetched usage surface.
 * DeepSeek extra balance credits (`deepseek-balance:*`) do not count alone —
 * they are not "auto usage" for the empty/manual fallback gate.
 *
 * @see docs/design-usage-card.md Issue 5
 */
export function computeHasAutoUsage(sub: Subscription): boolean {
  const usage = sub.usage;
  if (!usage) return false;
  const deepseekExtraBalances = (usage.credits ?? []).some((c) => c.credit_type.startsWith("deepseek-balance:"));
  const hasCredits = (usage.credits?.length ?? 0) > 0;
  const hasApiKeys = (usage.api_keys?.length ?? 0) > 0;
  return Boolean(
    usage.hourly ||
      usage.weekly ||
      usage.monthly ||
      usage.balance ||
      (hasCredits && !deepseekExtraBalances) ||
      hasApiKeys,
  );
}
