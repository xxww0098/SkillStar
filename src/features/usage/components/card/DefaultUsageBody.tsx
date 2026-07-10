import { UsageWindowBar } from "../UsageWindowBar";
import type { UsageBodyProps } from "./bodyRegistry";

/** Fallback body: generic hourly / weekly / monthly bars. */
export function DefaultUsageBody({ usage, density }: UsageBodyProps) {
  const compact = density === "compact";
  return (
    <>
      {usage.hourly && <UsageWindowBar window={usage.hourly} compact={compact} />}
      {usage.weekly && <UsageWindowBar window={usage.weekly} compact={compact} />}
      {usage.monthly && <UsageWindowBar window={usage.monthly} compact={compact} />}
    </>
  );
}
