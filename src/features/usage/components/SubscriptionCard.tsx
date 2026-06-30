import { motion } from "framer-motion";
import {
  BadgeCheck,
  Check,
  Copy,
  ExternalLink,
  GripVertical,
  Pencil,
  RefreshCw,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { isTauri } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { ExternalAnchor } from "@/components/ui/ExternalAnchor";
import { cn } from "@/lib/utils";
import { formatCurrencyAmount, monthlyEquivalentPrice } from "../lib/pricing";
import { formatUsageErrorForDisplay } from "../lib/usageErrors";
import { authModeLabel, formatQuotaNumber, formatRelativeSync, getPrimaryResetInfo } from "../lib/usageLabels";
import { getBrandTheme } from "../lib/brandThemes";
import type { CatalogEntry, CreditInfo, Subscription } from "../types";
import { usageApi } from "../api";
import { PlanBadge } from "./PlanBadge";
import { hasBrandIcon, ProviderLogo } from "./ProviderLogo";
import { priorityCardClass, ResetCountdown, UsagePriorityHint } from "./ResetCountdown";
import { DeepSeekUsagePanel } from "./DeepSeekUsagePanel";
import { GlmUsagePanel } from "./GlmUsagePanel";
import { UsageWindowBar } from "./UsageWindowBar";

interface SubscriptionCardProps {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
  onRefresh: (id: string) => Promise<void>;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  /** Switch this subscription to be the active account for its catalog
   *  (Phase 7 multi-account). When omitted, the switch button is hidden. */
  onSetActive?: (id: string) => Promise<void>;
  /** Re-push the active account's credentials to its CLI config (retry path
   *  shown when the previous CLI switch failed). Catalog must support CLI
   *  switching (`supports_cli_switch`). */
  onSwitchToCli?: (catalogId: string) => Promise<void>;
  refreshDisabled?: boolean;
  /** Drag handle pointer-down; passed through to dnd lib. */
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

export function SubscriptionCard({
  subscription: sub,
  catalog,
  onRefresh,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  refreshDisabled = false,
  onDragHandlePointerDown,
}: SubscriptionCardProps) {
  const { t } = useTranslation();
  const [refreshing, setRefreshing] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const usage = sub.usage ?? null;
  const planName = (usage?.plan_name ?? sub.plan_tier ?? null) || null;
  const balance = usage?.balance ?? null;
  const credits = usage?.credits ?? [];
  const apiKeys = usage?.api_keys ?? [];
  const hasCredits = credits.length > 0;
  const hasApiKeys = apiKeys.length > 0;
  const isDeepSeek = sub.catalog_id === "deepseek";
  const deepseekExtraBalances = (usage?.credits ?? []).some((credit) =>
    credit.credit_type.startsWith("deepseek-balance:"),
  );
  const hasAutoUsage = Boolean(
    usage?.hourly || usage?.weekly || usage?.monthly || balance || (hasCredits && !deepseekExtraBalances) || hasApiKeys,
  );
  const showRenewFooter = sub.renew_date > 0;
  const renewDays = daysUntil(sub.renew_date);
  const monthlyCost = monthlyEquivalentPrice(sub);
  const resetInfo = getPrimaryResetInfo(usage);
  const brandColorHex = catalog?.brand_color ?? "6B7280";
  const theme = getBrandTheme(sub.catalog_id, brandColorHex);
  const brandRgb = hexToRgb(theme.glow);
  const brandIcon = hasBrandIcon(sub.catalog_id);
  const usageError = formatUsageErrorForDisplay(usage?.error, t);

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh(sub.id);
    } finally {
      setRefreshing(false);
    }
  };

  const [activating, setActivating] = useState(false);
  const [cliSyncing, setCliSyncing] = useState(false);
  const handleSetActive = async () => {
    if (!onSetActive || sub.is_active) return;
    setActivating(true);
    try {
      await onSetActive(sub.id);
    } finally {
      setActivating(false);
    }
  };
  const handleSwitchToCli = async () => {
    if (!onSwitchToCli) return;
    setCliSyncing(true);
    try {
      await onSwitchToCli(sub.catalog_id);
    } finally {
      setCliSyncing(false);
    }
  };

  return (
    <motion.article
      layout
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.96 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      style={
        {
          "--brand-rgb": brandRgb,
          "--brand-color": theme.bar[0],
          "--brand-color-2": theme.bar[1],
        } as React.CSSProperties
      }
      className={cn(
        "group relative flex flex-col rounded-3xl border bg-white/95 backdrop-blur-xl overflow-hidden",
        "border-zinc-200/80 hover:border-zinc-300 transition-all duration-300",
        "w-full sm:w-[280px] min-h-[320px] shrink-0 shadow-[0_8px_30px_rgba(0,0,0,0.03)]",
        "hover:shadow-[0_10px_34px_rgba(var(--brand-rgb),0.14)]",
        sub.is_active && "border-emerald-400/60 ring-1 ring-emerald-300/40",
        sub.requires_reauth && "border-red-500/40 ring-1 ring-red-500/20",
        resetInfo && priorityCardClass(resetInfo.resetAt, resetInfo.usedPercent, resetInfo.mode),
      )}
      aria-label={sub.display_name}
    >
      {/* ── Brand signature header band ── */}
      <header className="relative z-10">
        <div
          className="relative overflow-hidden px-4 pt-4 pb-3.5"
          style={{ background: `linear-gradient(135deg, ${theme.header[0]}, ${theme.header[1]})`, color: theme.fg }}
        >
          {/* glossy top sheen + soft light bloom */}
          <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-white/30" />
          <div className="pointer-events-none absolute -top-10 -right-8 h-28 w-28 rounded-full bg-white/15 blur-2xl transition-transform duration-500 group-hover:scale-125" />

          <div className="relative flex items-start gap-3" style={{ textShadow: "0 1px 2px rgba(0,0,0,0.18)" }}>
            {brandIcon ? (
              // text-zinc-900 fixes mono logos (Cursor/Grok/Windsurf/…) that render
              // in currentColor — otherwise they inherit the band's white fg and
              // vanish on the white chip.
              <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-white text-zinc-900 shadow-[0_2px_8px_rgba(0,0,0,0.18)] ring-1 ring-black/5 [text-shadow:none]">
                <ProviderLogo
                  catalogId={sub.catalog_id}
                  displayName={sub.display_name}
                  brandColor={brandColorHex}
                  size="md"
                />
              </div>
            ) : (
              <ProviderLogo
                catalogId={sub.catalog_id}
                displayName={sub.display_name}
                brandColor={brandColorHex}
                size="lg"
                className="shrink-0 shadow-[0_2px_8px_rgba(0,0,0,0.2)] ring-1 ring-white/30"
              />
            )}
            <div className="min-w-0 flex-1">
              <h3 className="pr-1 text-sm font-bold leading-snug line-clamp-2" title={sub.display_name}>
                {sub.display_name}
              </h3>
              {catalog?.description && (
                <p
                  className="mt-0.5 text-[10px] leading-snug opacity-90 line-clamp-1 break-words"
                  title={catalog.description}
                >
                  {catalog.description}
                </p>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-1 self-start">
              <PlanBadge plan={planName} variant="onBrand" />
              <button
                type="button"
                onPointerDown={onDragHandlePointerDown}
                className={cn(
                  "cursor-grab text-current/70 hover:text-current active:cursor-grabbing",
                  onDragHandlePointerDown ? "opacity-70 group-hover:opacity-100" : "opacity-0 group-hover:opacity-100",
                )}
                aria-label={t("usage.dragHandle")}
                tabIndex={-1}
              >
                <GripVertical className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </div>

        {/* ── Meta strip (auth / active · reset / synced) on white ── */}
        <div className="space-y-1.5 px-4 pt-2.5">
          <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
            <div className="flex min-w-0 flex-wrap items-center gap-1.5">
              <span className="shrink-0 rounded px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider bg-zinc-100 text-zinc-600 ring-1 ring-zinc-200/60">
                {authModeLabel(sub.auth_mode, t)}
              </span>
              {sub.is_active && (
                <span
                  className="inline-flex shrink-0 items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider bg-emerald-50 text-emerald-700 ring-1 ring-emerald-200/60"
                  title="该 catalog 当前活跃的账号"
                >
                  <BadgeCheck className="h-2.5 w-2.5" />
                  当前
                </span>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              {resetInfo && (
                <ResetCountdown resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
              )}
              <p className="text-[9px] font-mono tabular-nums text-zinc-400">
                {formatRelativeSync(usage?.fetched_at ?? 0, t)}
              </p>
            </div>
          </div>

          {resetInfo && (
            <UsagePriorityHint resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
          )}
        </div>
      </header>

      {/* ── Body: progress bars / balance / fallback ─────────────── */}
      <div className="relative z-10 flex-1 px-4 pt-3 pb-2 space-y-3.5 overflow-hidden">
        {sub.catalog_id === "glm" && usage ? (
          <GlmUsagePanel usage={usage} brandColor={brandColorHex} />
        ) : isDeepSeek && usage ? (
          <DeepSeekUsagePanel usage={usage} brandColor={brandColorHex} hasPlatformToken={sub.has_platform_token} />
        ) : (
          <>
            {usage?.hourly && <UsageWindowBar window={usage.hourly} />}
            {usage?.weekly && <UsageWindowBar window={usage.weekly} />}
            {usage?.monthly && <UsageWindowBar window={usage.monthly} />}
          </>
        )}
        {balance && !isDeepSeek && <BalanceLine balance={balance} brandColor={brandColorHex} />}
        {hasCredits && sub.catalog_id !== "glm" && !isDeepSeek && (
          <CreditsLine credits={credits} brandColor={brandColorHex} />
        )}
        {!hasAutoUsage && <ManualUsage sub={sub} />}
        {usageError && (
          <p
            className="text-[11px] text-amber-500/90 line-clamp-2 rounded-lg bg-amber-500/[0.04] border border-amber-500/10 p-2"
            title={usage?.error ?? usageError}
          >
            ⚠ {usageError}
          </p>
        )}
        {hasApiKeys && <OpenCodeApiKeyCopyBar subscriptionId={sub.id} apiKeys={apiKeys} />}
      </div>

      <footer className="relative z-10 flex flex-col gap-2.5 px-4 py-3 border-t border-zinc-100 bg-zinc-50/50">
        {(monthlyCost !== null || showRenewFooter) && (
          <div
            className={cn(
              "grid gap-2.5 text-[10px]",
              monthlyCost !== null && showRenewFooter ? "grid-cols-2" : "grid-cols-1",
            )}
          >
            {monthlyCost !== null && (
              <div className="rounded-xl bg-zinc-100/60 border border-zinc-200/40 px-2.5 py-2 min-w-0">
                <p className="text-[10px] text-zinc-500 whitespace-nowrap mb-1">{t("usage.subscriptionCost")}</p>
                <p className="font-bold text-[11px] tabular-nums text-zinc-800 whitespace-nowrap">
                  {formatCurrencyAmount(monthlyCost, sub.currency)}
                  <span className="text-[9px] font-normal text-zinc-400 ml-0.5">{t("usage.perMonth")}</span>
                </p>
              </div>
            )}
            {showRenewFooter && (
              <div className="rounded-xl bg-zinc-100/60 border border-zinc-200/40 px-2.5 py-2 min-w-0">
                <p className="text-[10px] text-zinc-500 whitespace-nowrap mb-1">{t("usage.nextRenew")}</p>
                <div className="font-bold text-[11px] whitespace-nowrap">
                  {renewDays !== null ? (
                    renewDays < 0 ? (
                      <span className="text-rose-600">{t("usage.expired", { days: -renewDays })}</span>
                    ) : renewDays === 0 ? (
                      <span className="text-amber-600">{t("usage.expiresToday")}</span>
                    ) : renewDays <= 7 ? (
                      <span className="text-amber-600">{t("usage.renewInDays", { days: renewDays })}</span>
                    ) : (
                      <span className="text-zinc-700">{t("usage.renewInDays", { days: renewDays })}</span>
                    )
                  ) : (
                    <span className="text-zinc-400 font-normal">{t("usage.noExpiry")}</span>
                  )}
                </div>
              </div>
            )}
          </div>
        )}
        <div className="flex items-center justify-end gap-0.5">
          {onSetActive && !sub.is_active && (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.setActive", "切为当前账号")}
              onClick={handleSetActive}
              disabled={activating}
              className="text-zinc-500 hover:text-emerald-600 hover:scale-105 transition-transform"
            >
              <BadgeCheck className={cn("w-3.5 h-3.5", activating && "animate-pulse")} />
            </Button>
          )}
          {onSwitchToCli &&
            sub.is_active &&
            sub.supports_cli_switch &&
            sub.switch_result &&
            !sub.switch_result.success && (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.resyncCli", "重新同步到 CLI")}
                onClick={() => void handleSwitchToCli()}
                disabled={cliSyncing}
                className="text-amber-500 hover:text-amber-600 hover:scale-105 transition-transform"
              >
                <RefreshCw className={cn("w-3.5 h-3.5", cliSyncing && "animate-spin")} />
              </Button>
            )}
          {isTauri() && (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.openInWindow", "在新窗口打开")}
              onClick={() => void usageApi.openUsageCardWindow(sub.id)}
              className="text-zinc-500 hover:text-zinc-800 hover:scale-105 transition-transform"
            >
              <ExternalLink className="w-3.5 h-3.5" />
            </Button>
          )}
          {sub.requires_reauth ? (
            <Button
              size="icon-sm"
              variant="destructive"
              title={t("usage.requiresReauth")}
              onClick={() => onReauth?.(sub.id)}
              className="hover:scale-105 transition-transform"
            >
              <ShieldAlert className="w-3.5 h-3.5" />
            </Button>
          ) : (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.syncUsage")}
              onClick={handleRefresh}
              disabled={refreshing || refreshDisabled || sub.auth_mode === "manual"}
              className="text-zinc-500 hover:text-zinc-800 hover:scale-105 transition-transform"
            >
              <RefreshCw className={cn("w-3.5 h-3.5", refreshing && "animate-spin")} />
            </Button>
          )}
          <Button
            size="icon-sm"
            variant="ghost"
            title={t("common.edit")}
            onClick={() => onEdit(sub.id)}
            className="text-zinc-500 hover:text-zinc-800 hover:scale-105 transition-transform"
          >
            <Pencil className="w-3.5 h-3.5" />
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            title={t("common.delete")}
            onClick={() => setDeletePending(true)}
            className="text-zinc-400 hover:text-red-500 hover:scale-105 transition-transform"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </Button>
          {catalog?.subscription_url && (
            <ExternalAnchor
              href={catalog.subscription_url}
              className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-white/5 hover:text-foreground hover:scale-105 transition-all"
              title={t("usage.renewConsole")}
            >
              <ExternalLink className="w-3.5 h-3.5" />
            </ExternalAnchor>
          )}
        </div>
      </footer>

      {deletePending && (
        <div className="absolute inset-0 z-20 flex items-center justify-center rounded-3xl bg-white/90 backdrop-blur-sm">
          <div className="mx-4 rounded-2xl border border-red-200 bg-white p-5 shadow-xl">
            <p className="mb-1 text-sm font-semibold text-zinc-900">{t("usage.confirmDeleteTitle", "确认删除")}</p>
            <p className="mb-4 text-xs text-zinc-500">{t("usage.confirmDeleteMsg", { name: sub.display_name })}</p>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setDeletePending(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                onClick={() => {
                  setDeletePending(false);
                  onDelete(sub.id);
                }}
              >
                {t("common.delete")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </motion.article>
  );
}

function daysUntil(epoch: number): number | null {
  if (!epoch || epoch <= 0) return null;
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  return Math.floor(diff / 86_400);
}

function hexToRgb(hex: string): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b) ? "107, 114, 128" : `${r}, ${g}, ${b}`;
}

function BalanceLine({
  balance,
  brandColor = "10B981",
}: {
  balance: NonNullable<Subscription["usage"]>["balance"];
  brandColor?: string;
}) {
  const { t } = useTranslation();
  if (!balance) return null;
  const fmt = (n: number) => formatNumber(n, t("usage.numberUnit10k"));
  const c = `#${brandColor}`;
  return (
    <div
      className="rounded-xl border p-3 flex flex-col relative overflow-hidden"
      style={{ backgroundColor: `${c}08`, borderColor: `${c}1A` }}
    >
      <div
        className="absolute top-0 right-0 w-12 h-12 rounded-full filter blur-md pointer-events-none"
        style={{ backgroundColor: `${c}0D` }}
      />
      <div className="text-[10px] font-medium uppercase tracking-wider mb-0.5" style={{ color: c }}>
        {t("usage.balanceLabel")}
      </div>
      <div className="text-xl font-bold font-mono tabular-nums drop-shadow-sm" style={{ color: c }}>
        {balance.currency === "CNY" ? "¥" : balance.currency === "USD" ? "$" : ""}
        {fmt(balance.total)}
      </div>
      {(balance.granted > 0 || balance.topped_up > 0) && (
        <div
          className="text-[9px] text-muted-foreground/75 mt-1.5 pt-1.5 flex items-center justify-between"
          style={{ borderTopColor: `${c}0D`, borderTopWidth: 1 }}
        >
          <span>{t("usage.balanceGranted", { granted: fmt(balance.granted), topup: fmt(balance.topped_up) })}</span>
        </div>
      )}
    </div>
  );
}

function CreditsLine({ credits, brandColor = "10B981" }: { credits: CreditInfo[]; brandColor?: string }) {
  const { t } = useTranslation();
  if (!credits || credits.length === 0) return null;
  const c = `#${brandColor}`;
  return (
    <div
      className="rounded-2xl border p-3 flex flex-col gap-2.5 relative overflow-hidden"
      style={{ backgroundColor: `${c}06`, borderColor: `${c}14` }}
    >
      <div className="text-[10px] font-semibold uppercase tracking-wider" style={{ color: c }}>
        {t("usage.creditsLabel", "AI 积分")}
      </div>
      {credits.map((credit, i) => (
        <CreditProgressItem key={`${credit.credit_type}-${i}`} credit={credit} brandColor={brandColor} />
      ))}
    </div>
  );
}

function CreditProgressItem({ credit, brandColor }: { credit: CreditInfo; brandColor: string }) {
  const { t } = useTranslation();
  const parsed = parseCreditProgress(credit.credit_amount);
  const c = `#${brandColor}`;
  const label = formatCreditType(credit.credit_type, t);

  if (!parsed) {
    return (
      <div className="flex items-center justify-between gap-2 rounded-xl bg-white/45 px-2 py-1.5 text-[10px]">
        <span className="text-zinc-500 capitalize">{label}</span>
        <span className="font-mono font-semibold tabular-nums text-zinc-800">{credit.credit_amount ?? "—"}</span>
      </div>
    );
  }

  const remaining = Math.max(0, parsed.total - parsed.used);
  const percent = clampPercent(parsed.percent ?? (parsed.used / parsed.total) * 100);

  return (
    <div className="space-y-2 rounded-xl bg-white/45 px-2.5 py-2 ring-1 ring-zinc-200/45">
      <div className="flex items-center justify-between gap-2">
        <span className="min-w-0 truncate text-[10px] font-semibold text-zinc-700">{label}</span>
        <span
          className="shrink-0 rounded-md px-1.5 py-0.5 font-mono text-[9px] font-bold tabular-nums"
          style={{ backgroundColor: `${c}12`, color: c }}
        >
          {percent}%
        </span>
      </div>
      <div className="flex items-baseline gap-1.5">
        <span className="font-mono text-sm font-bold leading-none tabular-nums text-zinc-900">
          {formatQuotaNumber(parsed.used)}
        </span>
        <span className="text-[10px] text-zinc-300">/</span>
        <span className="font-mono text-[11px] font-semibold tabular-nums text-zinc-500">
          {formatQuotaNumber(parsed.total)}
        </span>
        <span className="ml-auto text-[10px] font-medium text-zinc-400">{t("usage.used")}</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/30">
        <div
          className="h-full rounded-full transition-[width] duration-500 ease-out"
          style={{
            width: `${Math.max(2, percent)}%`,
            background: `linear-gradient(90deg, ${c}, ${c}cc)`,
            boxShadow: `0 0 10px ${c}45`,
          }}
        />
      </div>
      <p className="text-[9px] leading-none text-zinc-400">
        {t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })}
      </p>
    </div>
  );
}

function parseCreditProgress(value?: string | null): { used: number; total: number; percent: number | null } | null {
  if (!value) return null;
  const match = value.match(/([\d,.\s]+)\s*\/\s*([\d,.\s]+)(?:\s*\((\d+(?:\.\d+)?)%\))?/);
  if (!match) return null;
  const used = parseCreditNumber(match[1]);
  const total = parseCreditNumber(match[2]);
  if (used === null || total === null || total <= 0) return null;
  const percent = match[3] ? Number(match[3]) : null;
  return { used, total, percent: Number.isFinite(percent) ? percent : null };
}

function parseCreditNumber(value: string): number | null {
  const parsed = Number(value.replace(/,/g, "").trim());
  return Number.isFinite(parsed) ? parsed : null;
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

function formatCreditType(value: string, t: ReturnType<typeof useTranslation>["t"]): string {
  if (value === "Compensation Credits" || value === "compensation_total_token") return t("usage.creditCompensation");
  if (value === "Token Plan Credits" || value === "total_token") return "Token Plan";
  return value.replace(/_/g, " ");
}

function ManualUsage({ sub }: { sub: Subscription }) {
  const { t } = useTranslation();
  const q = sub.manual_quota;
  if (!q || (!q.total_tokens && !q.used_tokens)) {
    return <p className="text-[11px] text-zinc-400 italic py-2">{t("usage.noUsageData")}</p>;
  }
  const total = q.total_tokens ?? 0;
  const used = q.used_tokens ?? 0;
  const percent = total > 0 ? Math.round((used / total) * 100) : 0;
  return (
    <UsageWindowBar
      window={{
        label: q.period_label ?? t("usage.defaultPeriod"),
        used,
        total: q.total_tokens,
        percent,
        reset_at: null,
      }}
    />
  );
}

function formatNumber(n: number, unit10k: string): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 10_000) {
    return `${(n / 10_000).toFixed(2)}${unit10k}`;
  }
  return n.toFixed(2);
}

function OpenCodeApiKeyCopyBar({
  subscriptionId,
  apiKeys,
}: {
  subscriptionId: string;
  apiKeys: { id: string; name: string; display: string; email: string | null }[];
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [copying, setCopying] = useState(false);

  const handleCopy = async () => {
    if (copying) return;
    setCopying(true);
    try {
      const key = await usageApi.getSubscriptionApiKey(subscriptionId);
      if (!key) {
        toast.error(t("usage.copyApiKeyEmpty"));
        return;
      }
      await navigator.clipboard.writeText(key);
      setCopied(true);
      toast.success(t("usage.copyApiKeySuccess"));
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error(t("usage.copyApiKeyFailed"));
    } finally {
      setCopying(false);
    }
  };

  return (
    <div className="flex items-center gap-2 rounded-lg border border-zinc-200/60 bg-zinc-50/80 px-2.5 py-1.5">
      <div className="flex-1 min-w-0">
        <p className="text-[9px] font-medium uppercase tracking-wider text-zinc-500">{t("usage.apiKeyLabel")}</p>
        <p className="text-[10px] font-mono text-zinc-600 truncate">{apiKeys[0]?.display ?? "—"}</p>
      </div>
      <Button
        size="icon-sm"
        variant="ghost"
        onClick={handleCopy}
        disabled={copying}
        title={t("usage.copyApiKey")}
        className={cn("shrink-0 transition-all", copied ? "text-emerald-500" : "text-zinc-400 hover:text-zinc-700")}
      >
        {copied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
      </Button>
    </div>
  );
}
