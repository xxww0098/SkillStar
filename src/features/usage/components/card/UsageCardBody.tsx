import { Check, Copy } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { usageApi } from "../../api";
import { computeHasAutoUsage } from "../../lib/hasAutoUsage";
import { formatUsageErrorForDisplay } from "../../lib/usageErrors";
import { formatQuotaNumber } from "../../lib/usageLabels";
import type { CreditInfo, Subscription } from "../../types";
import { UsageWindowBar } from "../UsageWindowBar";
import { resolveUsageBodyRegistration } from "./bodyRegistry";
import { ProgressTrack } from "./primitives";
import { type AttachmentSurface, surfaceAllows } from "./surfaceAttachments";

export interface UsageCardBodyProps {
  subscription: Subscription;
  brandColorHex: string;
  density: "comfortable" | "compact";
  surface: AttachmentSurface;
}

/**
 * Shared usage body: registry panel + attachment decision tree.
 * Shell / header / footer / window chrome stay outside.
 *
 * @see docs/design-usage-card.md §2 Body
 */
export function UsageCardBody({ subscription: sub, brandColorHex, density, surface }: UsageCardBodyProps) {
  const { t } = useTranslation();
  const usage = sub.usage ?? null;
  const reg = resolveUsageBodyRegistration(sub.catalog_id);
  const hasAuto = computeHasAutoUsage(sub);
  const usageError = formatUsageErrorForDisplay(usage?.error, t);
  const credits = usage?.credits ?? [];
  const apiKeys = usage?.api_keys ?? [];
  const balance = usage?.balance ?? null;

  let panelNode: ReactNode = null;
  if (usage) {
    const Panel = reg.component;
    panelNode = (
      <Panel
        usage={usage}
        catalogId={sub.catalog_id}
        brandColorHex={brandColorHex}
        density={density}
        context={{ hasPlatformToken: sub.has_platform_token }}
      />
    );
  }

  const showBalance = Boolean(balance && !reg.ownsBalance && surfaceAllows("balance", surface));
  const showCredits = credits.length > 0 && !reg.ownsCredits && surfaceAllows("credits", surface);
  const showManual = !hasAuto && surfaceAllows("manual", surface);
  const showError = Boolean(usageError && surfaceAllows("usageError", surface));
  const showApiKeys = apiKeys.length > 0 && surfaceAllows("apiKeys", surface);

  return (
    <>
      {panelNode}
      {showBalance && balance && <BalanceLine balance={balance} brandColor={brandColorHex} />}
      {showCredits && <CreditsLine credits={credits} brandColor={brandColorHex} />}
      {showManual && <ManualUsage sub={sub} density={density} />}
      {showError && (
        <p
          className="line-clamp-2 rounded-lg border border-amber-500/10 bg-amber-500/[0.04] p-2 text-[11px] text-amber-500/90"
          title={usage?.error ?? usageError ?? undefined}
        >
          ⚠ {usageError}
        </p>
      )}
      {showApiKeys && <OpenCodeApiKeyCopyBar subscriptionId={sub.id} apiKeys={apiKeys} />}
    </>
  );
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
  const c = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;
  return (
    <div
      className="relative flex flex-col overflow-hidden rounded-xl border p-3"
      style={{ backgroundColor: `${c}08`, borderColor: `${c}1A` }}
    >
      <div
        className="pointer-events-none absolute top-0 right-0 h-12 w-12 rounded-full filter blur-md"
        style={{ backgroundColor: `${c}0D` }}
      />
      <div className="mb-0.5 text-[10px] font-medium tracking-wider uppercase" style={{ color: c }}>
        {t("usage.balanceLabel")}
      </div>
      <div className="font-mono text-xl font-bold tabular-nums drop-shadow-sm" style={{ color: c }}>
        {balance.currency === "CNY" ? "¥" : balance.currency === "USD" ? "$" : ""}
        {fmt(balance.total)}
      </div>
      {(balance.granted > 0 || balance.topped_up > 0) && (
        <div
          className="mt-1.5 flex items-center justify-between pt-1.5 text-[9px] text-muted-foreground/75"
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
  const c = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;
  return (
    <div
      className="relative flex flex-col gap-2.5 overflow-hidden rounded-2xl border p-3"
      style={{ backgroundColor: `${c}06`, borderColor: `${c}14` }}
    >
      <div className="text-[10px] font-semibold tracking-wider uppercase" style={{ color: c }}>
        {t("usage.creditsLabel")}
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
  const c = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;
  const label = formatCreditType(credit.credit_type, t);

  if (!parsed) {
    return (
      <div className="flex items-center justify-between gap-2 rounded-xl bg-white/45 px-2 py-1.5 text-[10px]">
        <span className="capitalize text-zinc-500">{label}</span>
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
      <ProgressTrack usedPercent={percent} size="compact" tone="accent-static" accent={c} />
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

function ManualUsage({ sub, density }: { sub: Subscription; density: "comfortable" | "compact" }) {
  const { t } = useTranslation();
  const q = sub.manual_quota;
  if (!q || (!q.total_tokens && !q.used_tokens)) {
    return <p className="py-2 text-[11px] text-zinc-400 italic">{t("usage.noUsageData")}</p>;
  }
  const total = q.total_tokens ?? 0;
  const used = q.used_tokens ?? 0;
  const percent = total > 0 ? Math.round((used / total) * 100) : 0;
  return (
    <UsageWindowBar
      compact={density === "compact"}
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
      // Clipboard only after invoke — never render full key in React state.
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
      <div className="min-w-0 flex-1">
        <p className="text-[9px] font-medium tracking-wider text-zinc-500 uppercase">{t("usage.apiKeyLabel")}</p>
        <p className="truncate font-mono text-[10px] text-zinc-600">{apiKeys[0]?.display ?? "—"}</p>
      </div>
      <Button
        size="icon-sm"
        variant="ghost"
        onClick={handleCopy}
        disabled={copying}
        title={t("usage.copyApiKey")}
        className={cn("shrink-0 transition-all", copied ? "text-emerald-500" : "text-zinc-400 hover:text-zinc-700")}
      >
        {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
      </Button>
    </div>
  );
}
