import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatQuotaNumber } from "../../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage, UsageWindow } from "../../types";
import { MeterFigure, UsageMeter } from "../card/primitives";

interface GlmUsagePanelProps {
  usage: SubscriptionUsage;
  brandColorHex?: string;
  brandColor?: string;
  density?: "comfortable" | "compact";
  catalogId?: string;
  context?: { hasPlatformToken?: boolean };
}

/**
 * Dense GLM body: short quota rows + always-visible 24h activity (no collapse).
 */
export function GlmUsagePanel({
  usage,
  brandColorHex,
  brandColor = "4A90E2",
  density = "comfortable",
}: GlmUsagePanelProps) {
  const { t } = useTranslation();
  const hex = brandColorHex ?? brandColor;
  const accent = hex.startsWith("#") ? hex : `#${hex}`;
  const compact = density === "compact";

  const activityCredits = (usage.credits ?? []).filter(
    (c) => c.credit_type === "glm-24h-tokens" || c.credit_type === "glm-24h-calls",
  );
  const modelCredits = (usage.credits ?? []).filter((c) => c.credit_type.startsWith("glm-model:"));
  const toolCredits = (usage.credits ?? []).filter((c) =>
    ["glm-24h-network-search", "glm-24h-web-read", "glm-24h-zread"].includes(c.credit_type),
  );

  const hasQuota = Boolean(usage.hourly || usage.weekly || usage.monthly);
  const hasActivity = activityCredits.length > 0 || modelCredits.length > 0 || toolCredits.length > 0;

  if (!hasQuota && !hasActivity) {
    return null;
  }

  return (
    <div className={cn("space-y-1.5", compact && "space-y-1")}>
      {usage.hourly && <GlmQuotaRow window={usage.hourly} title={t("usage.window5h")} compact={compact} />}
      {usage.weekly && <GlmQuotaRow window={usage.weekly} title={t("usage.window7d")} compact={compact} />}
      {usage.monthly && (
        <GlmQuotaRow
          window={usage.monthly}
          title={t("usage.glmMcpMonthly")}
          compact={compact}
          breakdown={compact ? undefined : usage.monthly.breakdown}
        />
      )}

      {hasActivity && (
        <div
          className="space-y-1 rounded-xl border border-zinc-200/50 px-2.5 py-1.5"
          style={{ backgroundColor: `${accent}06`, borderColor: `${accent}18` }}
        >
          <p className="text-[10px] font-semibold text-zinc-500">{t("usage.glmActivitySection")}</p>

          {activityCredits.map((credit) => (
            <ActivityRow key={credit.credit_type} credit={credit} accent={accent} />
          ))}

          {modelCredits.length > 0 && (
            <div className="space-y-0.5 pt-0.5">
              <p className="text-[9px] font-semibold tracking-wider text-zinc-400 uppercase">
                {t("usage.glmModelBreakdown")}
                <span className="ml-1 font-mono font-normal normal-case tabular-nums">({modelCredits.length})</span>
              </p>
              {modelCredits.map((credit) => (
                <ActivityRow key={credit.credit_type} credit={credit} accent={accent} />
              ))}
            </div>
          )}

          {toolCredits.length > 0 && (
            <div className="space-y-0.5 border-t border-zinc-200/40 pt-1">
              <p className="text-[9px] font-semibold tracking-wider text-zinc-400 uppercase">
                {t("usage.glmToolBreakdown")}
              </p>
              {toolCredits.map((credit) => (
                <ActivityRow key={credit.credit_type} credit={credit} accent={accent} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * One GLM quota window, composed from the shared `UsageMeter` grammar instead
 * of a local copy of its box + used-badge ramp (docs/features/usage §"所有额度条
 * 共享 UsageMeter primitive"). Breakdown rows ride in `children`.
 */
function GlmQuotaRow({
  window,
  title,
  compact,
  breakdown,
}: {
  window: UsageWindow;
  title: string;
  compact?: boolean;
  breakdown?: UsageWindow["breakdown"];
}) {
  const { t } = useTranslation();
  const percent = clampPercent(window.percent ?? computePercent(window.used, window.total));
  const remaining = window.total != null ? Math.max(0, window.total - window.used) : null;
  const hasBreakdown = (breakdown?.length ?? 0) > 0;

  return (
    <UsageMeter
      label={title}
      dot="brand"
      usedPercent={percent}
      compact={compact}
      figure={
        window.total != null ? (
          <MeterFigure value={formatQuotaNumber(window.used)} unit={`/ ${formatQuotaNumber(window.total)}`} />
        ) : null
      }
      caption={window.total != null ? t("usage.used") : null}
      resetAt={window.reset_at}
      // GLM has always shown its own countdown per window; keep that.
      showReset={Boolean(window.reset_at)}
      resetMode="rateLimit"
      footNote={remaining != null ? t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) }) : null}
    >
      {hasBreakdown && (
        <div className="space-y-0.5 border-t border-zinc-200/40 pt-1">
          {breakdown!.map((item, index) => (
            <div key={`${item.label}-${index}`} className="flex items-center justify-between gap-2 px-0.5 text-[10px]">
              <span className="truncate text-zinc-500">{glmCreditLabel(item.label, t)}</span>
              <span className="font-mono font-semibold tabular-nums text-zinc-700">{formatQuotaNumber(item.used)}</span>
            </div>
          ))}
        </div>
      )}
    </UsageMeter>
  );
}

function ActivityRow({ credit, accent }: { credit: CreditInfo; accent: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-between gap-2 px-0.5 py-0.5">
      <span className="min-w-0 truncate text-[10px] text-zinc-600">{glmCreditLabel(credit.credit_type, t)}</span>
      <span className="shrink-0 font-mono text-[10px] font-semibold tabular-nums" style={{ color: accent }}>
        {formatCreditAmount(credit)}
      </span>
    </div>
  );
}

function glmCreditLabel(key: string, t: ReturnType<typeof useTranslation>["t"]): string {
  const map: Record<string, string> = {
    "glm-24h-tokens": t("usage.glm24hTokens"),
    "glm-24h-calls": t("usage.glm24hCalls"),
    "glm-24h-network-search": t("usage.glm24hNetworkSearch"),
    "glm-24h-web-read": t("usage.glm24hWebRead"),
    "glm-24h-zread": t("usage.glm24hZread"),
    "glm-mcp-search": t("usage.glmMcpSearch"),
    "glm-mcp-web-read": t("usage.glmMcpWebRead"),
    "glm-mcp-zread": t("usage.glmMcpZread"),
  };
  if (map[key]) return map[key];
  if (key.startsWith("glm-model:")) {
    return key.slice("glm-model:".length);
  }
  return key.replace(/^glm-/, "").replace(/-/g, " ");
}

function formatCreditAmount(credit: CreditInfo): string {
  const raw = credit.credit_amount?.trim();
  if (!raw) return "—";
  const parsed = Number(raw.replace(/,/g, ""));
  if (!Number.isFinite(parsed)) return raw;
  return formatQuotaNumber(parsed);
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

// `total` is `number | null | undefined`: the backend skips the key entirely
// when the quota ceiling is unknown, so it arrives as `undefined`, not `null`.
function computePercent(used: number, total: number | null | undefined): number {
  if (!total || total <= 0) return 0;
  return Math.round((used / total) * 100);
}
