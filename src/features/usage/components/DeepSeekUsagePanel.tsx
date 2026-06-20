import { AlertTriangle, BarChart3, Brain, CheckCircle2, Zap } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { CreditInfo, DeepSeekAnalytics, DeepSeekDailyUsage, SubscriptionUsage } from "../types";

interface DeepSeekUsagePanelProps {
  usage: SubscriptionUsage;
  brandColor?: string;
  hasPlatformToken?: boolean;
}

export function DeepSeekUsagePanel({
  usage,
  brandColor = "1A56DB",
  hasPlatformToken = false,
}: DeepSeekUsagePanelProps) {
  const { t } = useTranslation();
  const accent = `#${brandColor}`;
  const balance = usage.balance;
  const extraBalances = (usage.credits ?? []).filter((credit) => credit.credit_type.startsWith("deepseek-balance:"));
  const analytics = usage.deepseek_analytics ?? null;

  if (!balance) {
    return null;
  }

  const available = balance.is_available ?? true;
  const showBreakdown = balance.granted > 0 || balance.topped_up > 0;

  return (
    <div className="space-y-3">
      <div
        className={cn(
          "flex items-center justify-between gap-2 rounded-xl border px-3 py-2",
          available ? "border-emerald-200/70 bg-emerald-50/70" : "border-amber-200/70 bg-amber-50/70",
        )}
      >
        <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-600">
          {t("usage.deepseekAccountStatus")}
        </span>
        <span
          className={cn(
            "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold",
            available ? "text-emerald-700" : "text-amber-700",
          )}
        >
          {available ? <CheckCircle2 className="h-3 w-3" /> : <AlertTriangle className="h-3 w-3" />}
          {available ? t("usage.deepseekAvailable") : t("usage.deepseekUnavailable")}
        </span>
      </div>

      <div
        className="rounded-2xl border p-3.5 flex flex-col relative overflow-hidden"
        style={{ backgroundColor: `${accent}08`, borderColor: `${accent}1A` }}
      >
        <div
          className="pointer-events-none absolute top-0 right-0 h-16 w-16 rounded-full blur-xl"
          style={{ backgroundColor: `${accent}12` }}
        />
        <p className="text-[10px] font-semibold uppercase tracking-wider" style={{ color: accent }}>
          {t("usage.deepseekTotalBalance")}
        </p>
        <p className="mt-1 text-2xl font-bold font-mono tabular-nums leading-none" style={{ color: accent }}>
          {formatCurrencyAmount(balance.total, balance.currency, t("usage.numberUnit10k"))}
        </p>
        <p className="mt-1 text-[9px] text-zinc-400">{t("usage.deepseekPaygHint")}</p>
      </div>

      {analytics ? (
        <DeepSeekAnalyticsSection analytics={analytics} accent={accent} />
      ) : (
        <DeepSeekAnalyticsHint hasPlatformToken={hasPlatformToken} accent={accent} />
      )}

      {showBreakdown && (
        <div
          className="rounded-2xl border p-3 space-y-2"
          style={{ backgroundColor: `${accent}04`, borderColor: `${accent}14` }}
        >
          <p className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("usage.deepseekBalanceBreakdown")}
          </p>
          {balance.granted > 0 && (
            <BalanceBreakdownRow
              label={t("usage.deepseekGrantedBalance")}
              amount={balance.granted}
              currency={balance.currency}
              accent={accent}
              unit10k={t("usage.numberUnit10k")}
            />
          )}
          {balance.topped_up > 0 && (
            <BalanceBreakdownRow
              label={t("usage.deepseekToppedUpBalance")}
              amount={balance.topped_up}
              currency={balance.currency}
              accent={accent}
              unit10k={t("usage.numberUnit10k")}
            />
          )}
        </div>
      )}

      {extraBalances.length > 0 && (
        <div className="rounded-2xl border border-zinc-200/60 bg-zinc-50/50 p-3 space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("usage.deepseekOtherCurrencies")}
          </p>
          {extraBalances.map((credit) => (
            <ExtraBalanceRow
              key={credit.credit_type}
              credit={credit}
              accent={accent}
              unit10k={t("usage.numberUnit10k")}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function DeepSeekAnalyticsHint({ hasPlatformToken, accent }: { hasPlatformToken: boolean; accent: string }) {
  const { t } = useTranslation();
  return (
    <div
      className="rounded-2xl border border-dashed p-3 text-[10px] leading-relaxed text-zinc-500"
      style={{ borderColor: `${accent}30`, backgroundColor: `${accent}04` }}
    >
      {hasPlatformToken ? t("usage.deepseekAnalyticsPending") : t("usage.deepseekAnalyticsHint")}
    </div>
  );
}

function DeepSeekAnalyticsSection({ analytics, accent }: { analytics: DeepSeekAnalytics; accent: string }) {
  const { t } = useTranslation();
  const flash = analytics.models.find((m) => m.key === "flash") ?? null;
  const pro = analytics.models.find((m) => m.key === "pro") ?? null;
  const maxTokens = Math.max(flash?.total_tokens ?? 0, pro?.total_tokens ?? 0, 1);

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2">
        <MetricCard label={t("usage.deepseekTodayCost")} value={formatMoney(analytics.today_cost)} accent={accent} />
        <MetricCard label={t("usage.deepseekMonthCost")} value={formatMoney(analytics.month_cost)} accent={accent} />
      </div>

      {(flash || pro) && (
        <div className="space-y-2">
          {flash && <ModelUsageRow model={flash} maxTokens={maxTokens} variant="flash" accent={accent} />}
          {pro && <ModelUsageRow model={pro} maxTokens={maxTokens} variant="pro" accent={accent} />}
        </div>
      )}

      <UsageTrendChart daily={analytics.daily} accent={accent} />
    </div>
  );
}

function MetricCard({ label, value, accent }: { label: string; value: string; accent: string }) {
  return (
    <div className="rounded-xl border px-3 py-2" style={{ borderColor: `${accent}20`, backgroundColor: `${accent}06` }}>
      <p className="text-[9px] font-semibold uppercase tracking-wider text-zinc-500">{label}</p>
      <p className="mt-1 font-mono text-sm font-bold tabular-nums" style={{ color: accent }}>
        {value}
      </p>
    </div>
  );
}

function ModelUsageRow({
  model,
  maxTokens,
  variant,
  accent,
}: {
  model: DeepSeekAnalytics["models"][number];
  maxTokens: number;
  variant: "flash" | "pro";
  accent: string;
}) {
  const { t } = useTranslation();
  const isFlash = variant === "flash";
  const width = `${Math.max(4, (model.total_tokens / maxTokens) * 100)}%`;
  const inputTotal = model.cache_hit_tokens + model.cache_miss_tokens;
  const hitRate = inputTotal > 0 ? Math.round((model.cache_hit_tokens / inputTotal) * 100) : null;

  return (
    <div
      className="rounded-xl border px-3 py-2.5"
      style={{ borderColor: `${accent}18`, backgroundColor: `${accent}05` }}
    >
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg",
            isFlash ? "bg-sky-500/15 text-sky-600" : "bg-violet-500/15 text-violet-600",
          )}
        >
          {isFlash ? <Zap className="h-4 w-4" /> : <Brain className="h-4 w-4" />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <p className="text-[11px] font-bold text-zinc-800">{model.name}</p>
            <p className="font-mono text-[11px] font-semibold tabular-nums" style={{ color: accent }}>
              {formatMoney(model.cost)}
            </p>
          </div>
          <p className="mt-0.5 text-[10px] text-zinc-500">
            {formatInt(model.total_tokens)} {t("usage.deepseekTokens")}
          </p>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-zinc-200/80">
            <div className={cn("h-full rounded-full", isFlash ? "bg-sky-500" : "bg-violet-500")} style={{ width }} />
          </div>
          {hitRate !== null && (
            <p className="mt-1 text-[9px] text-zinc-400">{t("usage.deepseekCacheHitRate", { rate: hitRate })}</p>
          )}
        </div>
      </div>
    </div>
  );
}

function UsageTrendChart({ daily, accent }: { daily: DeepSeekDailyUsage[]; accent: string }) {
  const { t } = useTranslation();
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const points = useMemo(() => recentUsageDays(daily), [daily]);
  const maxVal = Math.max(...points.map((p) => p.total), 1);
  const sumHit = points.reduce((sum, p) => sum + p.hit, 0);
  const sumMiss = points.reduce((sum, p) => sum + p.miss, 0);
  const sumTotal = points.reduce((sum, p) => sum + p.total, 0);
  const hitRate = sumHit + sumMiss > 0 ? Math.round((sumHit / (sumHit + sumMiss)) * 100) : 0;

  if (points.every((p) => p.total === 0)) {
    return (
      <div className="rounded-xl border border-dashed px-3 py-4 text-center text-[10px] text-zinc-400">
        {t("usage.deepseekNoTrendData")}
      </div>
    );
  }

  return (
    <div className="rounded-2xl border p-3" style={{ borderColor: `${accent}18`, backgroundColor: `${accent}04` }}>
      <div className="mb-3 flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-600">
          <BarChart3 className="h-3.5 w-3.5" style={{ color: accent }} />
          {t("usage.deepseekTrendTitle")}
        </div>
        <span className="text-[9px] text-zinc-400">
          {t("usage.deepseekTrendSummary", { rate: hitRate, total: formatTokensShort(sumTotal) })}
        </span>
      </div>

      <div className="flex h-28 items-end gap-1.5" onMouseLeave={() => setHoveredIdx(null)}>
        {points.map((point, idx) => (
          <div key={point.date} className="relative flex min-w-0 flex-1 flex-col items-center gap-1">
            {hoveredIdx === idx && point.total > 0 && (
              <div className="absolute bottom-full z-10 mb-1 w-36 rounded-lg border border-zinc-200 bg-white/95 p-2 text-[9px] shadow-lg backdrop-blur-sm">
                <p className="font-semibold text-zinc-700">{point.date}</p>
                <p className="mt-0.5 font-mono tabular-nums text-zinc-800">{formatInt(point.total)} tokens</p>
                <p className="mt-1 text-emerald-600">
                  {t("usage.deepseekCacheHit")}: {formatInt(point.hit)}
                </p>
                <p className="text-amber-600">
                  {t("usage.deepseekCacheMiss")}: {formatInt(point.miss)}
                </p>
                <p className="text-violet-600">
                  {t("usage.deepseekResponse")}: {formatInt(point.response)}
                </p>
              </div>
            )}
            <span className="text-[8px] font-mono tabular-nums text-zinc-400">
              {point.total > 0 ? formatTokensShort(point.total) : "0"}
            </span>
            <div className="flex h-20 w-full items-end">
              <div
                className="mx-auto flex w-4/5 min-h-[3px] flex-col justify-end overflow-hidden rounded-sm bg-zinc-200/50"
                style={{ height: `${point.total > 0 ? Math.max(8, (point.total / maxVal) * 100) : 8}%` }}
                onMouseEnter={() => setHoveredIdx(idx)}
              >
                {point.total > 0 ? (
                  <>
                    {point.hit > 0 && <div className="w-full bg-emerald-400" style={{ flexGrow: point.hit }} />}
                    {point.miss > 0 && <div className="w-full bg-amber-400" style={{ flexGrow: point.miss }} />}
                    {point.response > 0 && (
                      <div className="w-full bg-violet-400" style={{ flexGrow: point.response }} />
                    )}
                  </>
                ) : (
                  <div className="h-full w-full bg-zinc-300/40" />
                )}
              </div>
            </div>
            <span className="text-[8px] text-zinc-500">{mmdd(point.date)}</span>
          </div>
        ))}
      </div>

      <div className="mt-2 flex flex-wrap gap-3 text-[9px] text-zinc-500">
        <span className="inline-flex items-center gap-1">
          <i className="inline-block h-2 w-2 rounded-full bg-emerald-400" />
          {t("usage.deepseekCacheHit")}
        </span>
        <span className="inline-flex items-center gap-1">
          <i className="inline-block h-2 w-2 rounded-full bg-amber-400" />
          {t("usage.deepseekCacheMiss")}
        </span>
        <span className="inline-flex items-center gap-1">
          <i className="inline-block h-2 w-2 rounded-full bg-violet-400" />
          {t("usage.deepseekResponse")}
        </span>
      </div>
    </div>
  );
}

function recentUsageDays(days: DeepSeekDailyUsage[], count = 7) {
  const today = todayStr();
  const source = new Map(days.filter((day) => day.date <= today).map((day) => [day.date, day]));
  const now = new Date();
  return Array.from({ length: count }, (_, index) => {
    const date = dateKey(addDays(now, index - count + 1));
    const row = source.get(date);
    if (!row) {
      return { date, hit: 0, miss: 0, response: 0, total: 0 };
    }
    const hit = row.flash_cache_hit + row.pro_cache_hit;
    const miss = row.flash_cache_miss + row.pro_cache_miss;
    const response = row.flash_response + row.pro_response;
    return { date, hit, miss, response, total: hit + miss + response };
  });
}

function BalanceBreakdownRow({
  label,
  amount,
  currency,
  accent,
  unit10k,
}: {
  label: string;
  amount: number;
  currency: string;
  accent: string;
  unit10k: string;
}) {
  return (
    <div className="flex items-center justify-between gap-2 rounded-lg bg-white/60 px-2.5 py-1.5">
      <span className="text-[10px] text-zinc-600">{label}</span>
      <span className="font-mono text-[11px] font-semibold tabular-nums" style={{ color: accent }}>
        {formatCurrencyAmount(amount, currency, unit10k)}
      </span>
    </div>
  );
}

function ExtraBalanceRow({ credit, accent, unit10k }: { credit: CreditInfo; accent: string; unit10k: string }) {
  const currency = credit.credit_type.slice("deepseek-balance:".length) || "USD";
  const amount = Number.parseFloat(credit.credit_amount ?? "");
  const display = Number.isFinite(amount)
    ? formatCurrencyAmount(amount, currency, unit10k)
    : (credit.credit_amount ?? "—");

  return (
    <div className="flex items-center justify-between gap-2 rounded-lg bg-white/60 px-2.5 py-1.5">
      <span className="text-[10px] text-zinc-600">{currency}</span>
      <span className="font-mono text-[11px] font-semibold tabular-nums" style={{ color: accent }}>
        {display}
      </span>
    </div>
  );
}

function formatCurrencyAmount(amount: number, currency: string, unit10k: string): string {
  if (!Number.isFinite(amount)) return "—";
  const symbol = currency === "CNY" ? "¥" : currency === "USD" ? "$" : "";
  const formatted = Math.abs(amount) >= 10_000 ? `${(amount / 10_000).toFixed(2)}${unit10k}` : amount.toFixed(2);
  return `${symbol}${formatted}`;
}

function formatMoney(amount: number): string {
  if (!Number.isFinite(amount)) return "—";
  return `¥${amount.toFixed(2)}`;
}

function formatInt(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

function formatTokensShort(n: number): string {
  if (n >= 1e8) return `${(n / 1e6).toFixed(0)}M`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(Math.round(n));
}

function todayStr(): string {
  const now = new Date();
  return dateKey(now);
}

function dateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function addDays(date: Date, offset: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + offset);
  return next;
}

function mmdd(date: string): string {
  const parts = date.split("-");
  return parts.length === 3 ? `${Number(parts[1])}/${Number(parts[2])}` : date;
}
