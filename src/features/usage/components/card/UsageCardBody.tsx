import { Check, Copy } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { cn, copyToClipboard } from "@/lib/utils";
import { usageApi } from "../../api";
import { computeHasAutoUsage } from "../../lib/hasAutoUsage";
import { formatUsageErrorForDisplay } from "../../lib/usageErrors";
import { formatQuotaNumber } from "../../lib/usageLabels";
import type { CreditInfo, Subscription } from "../../types";
import { UsageWindowBar } from "../UsageWindowBar";
import { resolveUsageBodyRegistration } from "./bodyRegistry";
import { MeterFigure, UsageMeter } from "./primitives";
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
  const showEmptyHint = !usage && sub.has_credential && surfaceAllows("manual", surface) && !showManual;

  return (
    <>
      {panelNode}
      {showBalance && balance && <BalanceLine balance={balance} brandColor={brandColorHex} />}
      {showCredits && <CreditsLine credits={credits} brandColor={brandColorHex} />}
      {showManual && <ManualUsage sub={sub} density={density} />}
      {showEmptyHint && <p className="py-1 text-[11px] text-zinc-500">{t("usage.awaitingUsageRefresh")}</p>}
      {showError && (
        <p className="line-clamp-3 text-[11px] text-amber-800" title={usage?.error ?? usageError ?? undefined}>
          {usageError}
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
  const available = balance.is_available;
  const symbol = balance.currency === "CNY" ? "¥" : balance.currency === "USD" ? "$" : `${balance.currency} `;
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-semibold" style={{ color: c }}>
          {t("usage.balanceLabel")}
        </span>
        {available != null && (
          <span
            className={
              available ? "text-[10px] font-semibold text-emerald-800" : "text-[10px] font-semibold text-amber-800"
            }
          >
            {available ? t("usage.deepseekAvailable") : t("usage.deepseekUnavailable")}
          </span>
        )}
      </div>
      <div className="font-mono text-xl font-bold tabular-nums" style={{ color: c }}>
        {symbol}
        {fmt(balance.total)}
      </div>
      {(balance.granted > 0 || balance.topped_up > 0) && (
        <div className="flex justify-between gap-2 text-[10px] text-zinc-600">
          <span>{t("usage.balanceGrantedOnly", { granted: fmt(balance.granted) })}</span>
          <span>{t("usage.balanceTopupOnly", { topup: fmt(balance.topped_up) })}</span>
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
    <div className="space-y-2">
      <div className="text-[11px] font-semibold" style={{ color: c }}>
        {t("usage.creditsLabel")}
      </div>
      {credits.map((credit, i) => (
        <CreditProgressItem key={`${credit.credit_type}-${i}`} credit={credit} />
      ))}
    </div>
  );
}

/**
 * One credit line. When the backend hands us a `used / total`, it is a quota —
 * so it renders through the shared `UsageMeter` grammar rather than a private
 * copy of it (docs/features/usage §"所有额度条共享 UsageMeter primitive").
 */
function CreditProgressItem({ credit }: { credit: CreditInfo }) {
  const { t } = useTranslation();
  const parsed = parseCreditProgress(credit.credit_amount);
  const label = formatCreditType(credit.credit_type, t);

  if (!parsed) {
    return (
      <div className="flex flex-col gap-0.5 text-[10px]">
        <div className="flex items-center justify-between gap-2">
          <span className="capitalize text-zinc-600">{label}</span>
          <span className="font-mono font-semibold tabular-nums text-zinc-800">{credit.credit_amount ?? "—"}</span>
        </div>
        {credit.minimum_credit_amount_for_usage ? (
          <p className="text-[10px] text-zinc-500">
            {t("usage.creditMinimum", { min: credit.minimum_credit_amount_for_usage })}
          </p>
        ) : null}
      </div>
    );
  }

  const remaining = Math.max(0, parsed.total - parsed.used);
  const percent = clampPercent(parsed.percent ?? (parsed.used / parsed.total) * 100);

  return (
    <UsageMeter
      label={label}
      usedPercent={percent}
      compact
      figure={<MeterFigure value={formatQuotaNumber(parsed.used)} unit={`/ ${formatQuotaNumber(parsed.total)}`} />}
      caption={t("usage.used")}
      showReset={false}
      footNote={t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })}
    >
      {credit.minimum_credit_amount_for_usage ? (
        <p className="text-right text-[10px] text-zinc-500">
          {t("usage.creditMinimum", { min: credit.minimum_credit_amount_for_usage })}
        </p>
      ) : null}
    </UsageMeter>
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
    return <p className="py-2 text-[11px] text-zinc-500">{t("usage.noUsageData")}</p>;
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
        // Manual quotas have no reset clock; `UsageWindow.reset_at` is an
        // omitted key on the wire, so `undefined` — not `null` — is the miss.
        reset_at: undefined,
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
      if (!(await copyToClipboard(key))) {
        toast.error(t("usage.copyApiKeyFailed"));
        return;
      }
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
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-[10px] font-medium tracking-wider text-zinc-600 uppercase">
            {t("usage.apiKeyLabel")}
            {apiKeys.length > 1 ? ` · ${apiKeys.length}` : ""}
          </p>
          <p className="truncate font-mono text-[10px] text-zinc-700" title={apiKeys[0]?.display}>
            {apiKeys[0]?.display ?? "—"}
          </p>
          {(apiKeys[0]?.name || apiKeys[0]?.email) && (
            <p className="truncate text-[10px] text-zinc-500">
              {[apiKeys[0]?.name, apiKeys[0]?.email].filter(Boolean).join(" · ")}
            </p>
          )}
        </div>
        <Button
          size="icon-sm"
          variant="ghost"
          onClick={handleCopy}
          disabled={copying}
          aria-busy={copying}
          title={t("usage.copyApiKey")}
          aria-label={t("usage.copyApiKey")}
          className={cn("shrink-0 transition-all", copied ? "text-emerald-600" : "text-zinc-500 hover:text-zinc-800")}
        >
          {copied ? <Check className="h-3.5 w-3.5" aria-hidden /> : <Copy className="h-3.5 w-3.5" aria-hidden />}
        </Button>
      </div>
      {apiKeys.length > 1 && (
        <ul className="space-y-0.5 border-t border-zinc-200/50 pt-1">
          {apiKeys.slice(1).map((k) => (
            <li key={k.id} className="truncate font-mono text-[10px] text-zinc-600" title={k.display}>
              {k.display}
              {k.name || k.email ? (
                <span className="ml-1 font-sans text-zinc-500">({[k.name, k.email].filter(Boolean).join(" · ")})</span>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
