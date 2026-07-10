import { BadgeCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { authModeLabel, formatRelativeSync, type PrimaryResetInfo } from "../../lib/usageLabels";
import type { AuthMode } from "../../types";
import { ResetCountdown, UsagePriorityHint } from "../ResetCountdown";

export interface UsageCardMetaStripProps {
  authMode: AuthMode;
  isActive: boolean;
  fetchedAt: number;
  resetInfo: PrimaryResetInfo | null;
  bodyOwnsPrimaryReset: boolean;
}

/** Auth / active badge · primary reset · last-sync · priority hint (white strip under band). */
export function UsageCardMetaStrip({
  authMode,
  isActive,
  fetchedAt,
  resetInfo,
  bodyOwnsPrimaryReset,
}: UsageCardMetaStripProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-1.5 px-4 pt-2.5">
      <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-zinc-600 uppercase ring-1 ring-zinc-200/60">
            {authModeLabel(authMode, t)}
          </span>
          {isActive && (
            <span
              className="inline-flex shrink-0 items-center gap-0.5 rounded bg-emerald-50 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-emerald-700 uppercase ring-1 ring-emerald-200/60"
              title={t("usage.cardActiveTitle")}
            >
              <BadgeCheck className="h-2.5 w-2.5" />
              {t("usage.cardActive")}
            </span>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {resetInfo && !bodyOwnsPrimaryReset && (
            <ResetCountdown resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
          )}
          <p className="font-mono text-[9px] text-zinc-400 tabular-nums">{formatRelativeSync(fetchedAt, t)}</p>
        </div>
      </div>

      {resetInfo && (
        <UsagePriorityHint resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
      )}
    </div>
  );
}
