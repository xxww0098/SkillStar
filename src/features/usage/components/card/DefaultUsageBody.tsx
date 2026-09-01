import { useTranslation } from "react-i18next";
import { UsageWindowBar } from "../UsageWindowBar";
import type { UsageBodyProps } from "./bodyRegistry";

/** Fallback body: every available window (hourly → weekly → monthly), densest first. */
export function DefaultUsageBody({ usage, catalogId, density }: UsageBodyProps) {
  const { t } = useTranslation();
  const compact = density === "compact";
  const hasAny = Boolean(usage.hourly || usage.weekly || usage.monthly);

  if (!hasAny) {
    if (usage.error) return null;
    return (
      <p className="py-1 text-[11px] text-zinc-500">
        {usage.plan_name
          ? t("usage.awaitingUsageWithPlan", { plan: usage.plan_name })
          : t("usage.awaitingUsageRefresh")}
      </p>
    );
  }

  return (
    <div className={compact ? "space-y-1.5" : "space-y-2"}>
      {usage.hourly && <UsageWindowBar window={usage.hourly} compact={compact} catalogId={catalogId} />}
      {usage.weekly && <UsageWindowBar window={usage.weekly} compact={compact} catalogId={catalogId} />}
      {usage.monthly && <UsageWindowBar window={usage.monthly} compact={compact} catalogId={catalogId} />}
    </div>
  );
}
