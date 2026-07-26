import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatQuotaNumber } from "../../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage, UsageWindow } from "../../types";
import { ProgressTrack } from "../card/primitives";
import { ResetCountdown } from "../ResetCountdown";

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
      {usage.hourly && (
        <GlmQuotaRow window={usage.hourly} title={t("usage.window5h")} accent={accent} compact={compact} />
      )}
      {usage.weekly && (
        <GlmQuotaRow window={usage.weekly} title={t("usage.window7d")} accent={accent} compact={compact} />
      )}
      {usage.monthly && (
        <GlmQuotaRow
          window={usage.monthly}
          title={t("usage.glmMcpMonthly")}
          accent={accent}
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

/** One short row: title · numbers · reset · % · thin bar. */
function GlmQuotaRow({
  window,
  title,
  accent,
  compact,
  breakdown,
}: {
  window: UsageWindow;
  title: string;
  accent: string;
  compact?: boolean;
  breakdown?: UsageWindow["breakdown"];
}) {
  const { t } = useTranslation();
  const percent = clampPercent(window.percent ?? computePercent(window.used, window.total));
  const remaining = window.total != null ? Math.max(0, window.total - window.used) : null;
  const hasBreakdown = (breakdown?.length ?? 0) > 0;
  const high = percent >= 75;

  return (
    <div
      className={cn(
        "rounded-2xl border border-zinc-200/50 bg-zinc-50/40 transition-colors hover:bg-zinc-50/80",
        compact ? "space-y-1 px-2.5 py-2" : "space-y-1 px-3 py-2",
      )}
    >
      <div className="flex items-center gap-1.5">
        <span className="h-1.5 w-1.5 shrink-0 rounded-[2px]" style={{ backgroundColor: accent }} aria-hidden />
        <p className="min-w-0 shrink-0 text-[11px] font-bold leading-none text-zinc-700">{title}</p>
        {window.total != null ? (
          <p className="min-w-0 flex-1 truncate font-mono text-[10px] tabular-nums text-zinc-500">
            <span className="font-semibold text-zinc-800">{formatQuotaNumber(window.used)}</span>
            <span className="mx-0.5 text-zinc-300">/</span>
            {formatQuotaNumber(window.total)}
            {remaining != null && !compact && (
              <span className="ml-1 text-zinc-400">
                · {t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })}
              </span>
            )}
          </p>
        ) : (
          <span className="flex-1" />
        )}
        <div className="flex shrink-0 items-center gap-1">
          {window.reset_at ? <ResetCountdown resetAt={window.reset_at} usedPercent={percent} mode="rateLimit" /> : null}
          <span
            className={cn(
              "rounded-md px-1.5 py-0.5 font-mono text-[9px] font-bold tabular-nums",
              percent >= 90
                ? "bg-rose-500/10 text-rose-600"
                : percent >= 75
                  ? "bg-amber-500/10 text-amber-600"
                  : "bg-zinc-100 text-zinc-600",
            )}
          >
            {percent}%
          </span>
        </div>
      </div>

      {high ? (
        <ProgressTrack usedPercent={percent} size="compact" tone="brand-urgency" />
      ) : (
        <ProgressTrack usedPercent={percent} size="compact" tone="accent-static" accent={accent} />
      )}

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
    </div>
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

function computePercent(used: number, total: number | null): number {
  if (!total || total <= 0) return 0;
  return Math.round((used / total) * 100);
}
