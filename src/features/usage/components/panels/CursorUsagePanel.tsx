import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatUsdCents } from "../../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage } from "../../types";
import { MetaRow, MeterFigure, SecondaryPanel, UsageMeter } from "../card/primitives";
import { UsageWindowBar } from "../UsageWindowBar";

/** Backend credit types from `skillstar-usage` cursor fetcher. */
export const CURSOR_ON_DEMAND = "cursor-on-demand";
export const CURSOR_TEAM_ON_DEMAND = "cursor-team-on-demand";
export const CURSOR_BONUS = "cursor-bonus";

interface CursorUsagePanelProps {
  usage: SubscriptionUsage;
  /** Preferred brand hex (registry). */
  brandColorHex?: string;
  /** Legacy alias used by tests. */
  brandColor?: string;
  density?: "comfortable" | "compact";
  catalogId?: string;
  context?: { hasPlatformToken?: boolean };
}

/**
 * Cursor-specific usage body:
 * - Main plan window via shared `UsageWindowBar` (monetary Total + Auto/API,
 *   or percent-only breakdown) with remaining-oriented bars.
 * - Secondary lines for on-demand / team on-demand / bonus credits that the
 *   generic "AI 积分" block would mislabel.
 */
export function CursorUsagePanel({
  usage,
  brandColorHex,
  brandColor = "00E5BC",
  density = "comfortable",
}: CursorUsagePanelProps) {
  const { t } = useTranslation();
  const hex = brandColorHex ?? brandColor;
  const accent = hex.startsWith("#") ? hex : `#${hex}`;
  const compact = density === "compact";
  const monthly = usage.monthly;
  const weekly = usage.weekly;
  const onDemand = findCredit(usage.credits, CURSOR_ON_DEMAND);
  const teamOnDemand = findCredit(usage.credits, CURSOR_TEAM_ON_DEMAND);
  const bonus = findCredit(usage.credits, CURSOR_BONUS);
  const hasSecondary = Boolean(onDemand || teamOnDemand || bonus);

  if (!monthly && !weekly && !hasSecondary) {
    return null;
  }

  return (
    <div className={cn("space-y-3", compact && "space-y-2")}>
      {monthly && <UsageWindowBar window={monthly} compact={compact} />}
      {weekly && <UsageWindowBar window={weekly} compact={compact} />}
      {usage.hourly && <UsageWindowBar window={usage.hourly} compact={compact} />}
      {hasSecondary && (
        <SecondaryPanel accent={accent} className={compact ? "p-2" : undefined}>
          {bonus?.credit_amount && (
            <MetaRow label={t("usage.cursorBonus")} value={bonus.credit_amount} accent={accent} />
          )}
          {onDemand && (
            <OnDemandRow
              label={t("usage.cursorOnDemand")}
              credit={onDemand}
              accent={accent}
              showDivider={Boolean(bonus)}
            />
          )}
          {teamOnDemand && (
            <OnDemandRow
              label={t("usage.cursorTeamOnDemand")}
              credit={teamOnDemand}
              accent={accent}
              showDivider={Boolean(bonus || onDemand)}
            />
          )}
        </SecondaryPanel>
      )}
    </div>
  );
}

function OnDemandRow({
  label,
  credit,
  accent,
  showDivider,
}: {
  label: string;
  credit: CreditInfo;
  accent: string;
  showDivider: boolean;
}) {
  const { t } = useTranslation();
  const parsed = parseCentsProgress(credit.credit_amount);

  if (!parsed) {
    return <MetaRow label={label} value={credit.credit_amount ?? "—"} accent={accent} showDivider={showDivider} />;
  }

  const percent = parsed.total > 0 ? Math.max(0, Math.min(100, Math.round((parsed.used / parsed.total) * 100))) : 0;
  const remainingPct = Math.max(0, 100 - percent);

  // `used / total` cents is a quota, so it reads through the shared meter
  // grammar (docs/features/usage §"所有额度条共享 UsageMeter primitive").
  return (
    <div className={cn(showDivider && "border-t border-zinc-200/50 pt-2")}>
      <UsageMeter
        label={label}
        usedPercent={percent}
        compact
        figure={<MeterFigure value={formatUsdCents(parsed.used)} unit={`/ ${formatUsdCents(parsed.total)}`} />}
        caption={t("usage.used")}
        showReset={false}
        footNote={`${t("usage.remaining")} ${remainingPct}%`}
      />
    </div>
  );
}

function findCredit(credits: CreditInfo[] | undefined, type: string): CreditInfo | undefined {
  return (credits ?? []).find((c) => c.credit_type === type);
}

/** Parse `used/total` USD cents, or reject plain `$X` (handled as MetaRow). */
export function parseCentsProgress(amount?: string | null): { used: number; total: number } | null {
  if (!amount) return null;
  const match = amount.trim().match(/^(\d+)\s*\/\s*(\d+)$/);
  if (!match) return null;
  const used = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isFinite(used) || !Number.isFinite(total) || total <= 0) return null;
  return { used, total };
}
