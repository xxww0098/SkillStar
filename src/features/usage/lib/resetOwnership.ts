import type { UsageWindow } from "../types";
import { isAbsoluteQuotaWindow, isBreakdownQuotaWindow, isMonetaryQuota, type PrimaryResetInfo } from "./usageLabels";

/**
 * True when `UsageWindowBar` will draw its own `ResetCountdown` (simple bar path).
 * Codex-style 5h/7d bars defer countdown to the card header/meta.
 *
 * @see docs/design-usage-card.md K13
 */
export function windowRendersOwnReset(window: UsageWindow | null | undefined): boolean {
  if (!window?.reset_at) return false;
  if (window.label === "5h" || window.label === "7d") return false;
  if (isMonetaryQuota(window) || isBreakdownQuotaWindow(window) || isAbsoluteQuotaWindow(window)) {
    return false;
  }
  return true;
}

/**
 * Whether the body (window bar / vendor panel path) already owns the primary
 * reset countdown, so MetaStrip should suppress its twin.
 */
export function computeBodyOwnsPrimaryReset(
  usage: {
    hourly?: UsageWindow | null;
    weekly?: UsageWindow | null;
    monthly?: UsageWindow | null;
  } | null,
  resetInfo: PrimaryResetInfo | null,
  ownsPrimaryReset: boolean | "infer",
): boolean {
  if (ownsPrimaryReset === true) return true;
  if (ownsPrimaryReset === false) return false;
  if (!resetInfo || !usage) return false;
  if (resetInfo.source === "weekly") return windowRendersOwnReset(usage.weekly);
  if (resetInfo.source === "hourly") return windowRendersOwnReset(usage.hourly);
  if (resetInfo.source === "monthly") return windowRendersOwnReset(usage.monthly);
  return false;
}
