import { useTranslation } from "react-i18next";
import { UsageWindowBar } from "../UsageWindowBar";
import type { UsageBodyProps } from "./bodyRegistry";

/** Fallback body: every available window (hourly → weekly → monthly), densest first. */
export function DefaultUsageBody({ usage, density }: UsageBodyProps) {
  const { t } = useTranslation();
  const compact = density === "compact";
  const hasAny = Boolean(usage.hourly || usage.weekly || usage.monthly);

  if (!hasAny) {
    if (usage.error) return null;
    return (
      <p className="py-1 text-[11px] text-zinc-400 italic">
        {usage.plan_name
          ? t("usage.awaitingUsageWithPlan", { plan: usage.plan_name })
          : t("usage.awaitingUsageRefresh")}
      </p>
    );
  }

  return (
    <div className={compact ? "space-y-2" : "space-y-3"}>
      {usage.hourly && <UsageWindowBar window={usage.hourly} compact={compact} />}
      {usage.weekly && <UsageWindowBar window={usage.weekly} compact={compact} />}
      {usage.monthly && <UsageWindowBar window={usage.monthly} compact={compact} />}
    </div>
  );
}
