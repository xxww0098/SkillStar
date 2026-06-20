import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatQuotaNumber, pickRateLimitUsageTone, type ResetUrgencyMode } from "../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage, UsageWindow } from "../types";
import { ResetCountdown } from "./ResetCountdown";

interface GlmUsagePanelProps {
  usage: SubscriptionUsage;
  brandColor?: string;
}

export function GlmUsagePanel({ usage, brandColor = "4A90E2" }: GlmUsagePanelProps) {
  const { t } = useTranslation();
  const accent = `#${brandColor}`;
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
    <div className="space-y-3">
      {hasQuota && (
        <section className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("usage.glmQuotaSection")}
          </p>
          <div className="space-y-2">
            {usage.hourly && (
              <GlmTokenWindow window={usage.hourly} title={t("usage.window5h")} accent={accent} mode="rateLimit" />
            )}
            {usage.weekly && (
              <GlmTokenWindow window={usage.weekly} title={t("usage.window7d")} accent={accent} mode="rateLimit" />
            )}
            {usage.monthly && <GlmMcpWindow window={usage.monthly} accent={accent} />}
          </div>
        </section>
      )}

      {hasActivity && (
        <section className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("usage.glmActivitySection")}
          </p>
          <div
            className="rounded-2xl border p-3 space-y-2"
            style={{ backgroundColor: `${accent}06`, borderColor: `${accent}18` }}
          >
            {activityCredits.map((credit) => (
              <ActivityRow key={credit.credit_type} credit={credit} accent={accent} />
            ))}
            {modelCredits.length > 0 && (
              <div className="space-y-1.5 pt-1 border-t border-zinc-200/50">
                <p className="text-[9px] font-semibold uppercase tracking-wider text-zinc-400">
                  {t("usage.glmModelBreakdown")}
                </p>
                {modelCredits.map((credit) => (
                  <ActivityRow key={credit.credit_type} credit={credit} accent={accent} compact />
                ))}
              </div>
            )}
            {toolCredits.length > 0 && (
              <div className="space-y-1.5 pt-1 border-t border-zinc-200/50">
                <p className="text-[9px] font-semibold uppercase tracking-wider text-zinc-400">
                  {t("usage.glmToolBreakdown")}
                </p>
                {toolCredits.map((credit) => (
                  <ActivityRow key={credit.credit_type} credit={credit} accent={accent} compact />
                ))}
              </div>
            )}
          </div>
        </section>
      )}
    </div>
  );
}

function GlmTokenWindow({
  window,
  title,
  accent,
  mode,
}: {
  window: UsageWindow;
  title: string;
  accent: string;
  mode: ResetUrgencyMode;
}) {
  const { t } = useTranslation();
  const percent = clampPercent(window.percent ?? computePercent(window.used, window.total));
  const remaining = window.total != null ? Math.max(0, window.total - window.used) : null;
  const tone = pickRateLimitUsageTone(percent);

  return (
    <div className="rounded-2xl border border-zinc-200/60 bg-zinc-50/50 p-3 space-y-2.5">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[11px] font-bold text-zinc-800 leading-none">{title}</p>
          <p className="text-[9px] text-zinc-400 mt-1">{t("usage.rateLimitWindow")}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {window.reset_at ? <ResetCountdown resetAt={window.reset_at} usedPercent={percent} mode={mode} /> : null}
          <span
            className={cn("text-[9px] font-bold font-mono px-1.5 py-0.5 rounded-md tabular-nums", tone.text)}
            style={{ backgroundColor: `${accent}12` }}
          >
            {percent}%
          </span>
        </div>
      </div>

      {window.total != null ? (
        <div className="flex items-baseline gap-1.5">
          <span className="text-lg font-bold font-mono tabular-nums text-zinc-900 leading-none">
            {formatQuotaNumber(window.used)}
          </span>
          <span className="text-[10px] text-zinc-300">/</span>
          <span className="text-[11px] font-semibold font-mono tabular-nums text-zinc-500">
            {formatQuotaNumber(window.total)}
          </span>
          {remaining != null && (
            <span className="ml-auto text-[9px] text-zinc-400">
              {t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })}
            </span>
          )}
        </div>
      ) : (
        <p className="text-[10px] text-zinc-500">{t("usage.usedPercent", { percent })}</p>
      )}

      <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/30">
        <div
          className={cn("h-full rounded-full transition-all duration-500 ease-out", tone.bar)}
          style={{
            width: `${Math.max(2, percent)}%`,
            background: percent >= 75 ? undefined : `linear-gradient(90deg, ${accent}, ${accent}cc)`,
          }}
        />
      </div>
    </div>
  );
}

function GlmMcpWindow({ window, accent }: { window: UsageWindow; accent: string }) {
  const { t } = useTranslation();
  const percent = clampPercent(window.percent ?? computePercent(window.used, window.total));
  const tone = pickRateLimitUsageTone(percent);
  const remaining = window.total != null ? Math.max(0, window.total - window.used) : null;

  return (
    <div className="rounded-2xl border border-zinc-200/60 bg-zinc-50/50 p-3 space-y-2.5">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] font-bold text-zinc-800">{t("usage.glmMcpMonthly")}</p>
        <span className={cn("text-[9px] font-bold font-mono px-1.5 py-0.5 rounded-md tabular-nums", tone.text)}>
          {percent}%
        </span>
      </div>

      {window.total != null && (
        <div className="flex items-baseline gap-1.5">
          <span className="text-base font-bold font-mono tabular-nums text-zinc-900">
            {formatQuotaNumber(window.used)}
          </span>
          <span className="text-[10px] text-zinc-300">/</span>
          <span className="text-[11px] font-semibold font-mono tabular-nums text-zinc-500">
            {formatQuotaNumber(window.total)}
          </span>
          {remaining != null && (
            <span className="ml-auto text-[9px] text-zinc-400">
              {t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })}
            </span>
          )}
        </div>
      )}

      <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/30">
        <div
          className={cn("h-full rounded-full transition-all duration-500", tone.bar)}
          style={{ width: `${Math.max(2, percent)}%`, backgroundColor: percent >= 75 ? undefined : accent }}
        />
      </div>

      {(window.breakdown?.length ?? 0) > 0 && (
        <div className="space-y-1.5 pt-1 border-t border-zinc-200/50">
          {window.breakdown!.map((item, index) => (
            <div
              key={`${item.label}-${index}`}
              className="flex items-center justify-between gap-2 rounded-lg bg-white/60 px-2 py-1.5 text-[10px]"
            >
              <span className="text-zinc-600">{glmCreditLabel(item.label, t)}</span>
              <span className="font-mono font-semibold tabular-nums text-zinc-800">{formatQuotaNumber(item.used)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ActivityRow({ credit, accent, compact = false }: { credit: CreditInfo; accent: string; compact?: boolean }) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-2 rounded-lg bg-white/55 px-2.5",
        compact ? "py-1" : "py-1.5",
      )}
    >
      <span className={cn("text-zinc-600", compact ? "text-[9px]" : "text-[10px]")}>
        {glmCreditLabel(credit.credit_type, t)}
      </span>
      <span
        className={cn("font-mono font-semibold tabular-nums", compact ? "text-[10px]" : "text-[11px]")}
        style={{ color: accent }}
      >
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
  if (credit.credit_type === "glm-24h-tokens" || credit.credit_type.startsWith("glm-model:")) {
    return formatQuotaNumber(parsed);
  }
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
