import { ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { PrimaryResetInfo } from "../../lib/usageLabels";
import { ResetCountdown } from "../ResetCountdown";
import { usageCardSlotClassName } from "./usageCardShell";

export interface UsageCardMetaStripProps {
  requiresReauth?: boolean;
  hasCredential?: boolean;
  note?: string | null;
  resetInfo: PrimaryResetInfo | null;
  bodyOwnsPrimaryReset: boolean;
}

/** Exception flags, primary reset, user note. Absent when there is nothing to say. */
export function UsageCardMetaStrip({
  requiresReauth,
  hasCredential,
  note,
  resetInfo,
  bodyOwnsPrimaryReset,
}: UsageCardMetaStripProps) {
  const { t } = useTranslation();
  const noteText = note?.trim() ?? "";
  const showReset = Boolean(resetInfo && !bodyOwnsPrimaryReset);
  const showReauth = Boolean(requiresReauth);
  const showNoCredential = hasCredential === false;

  if (!noteText && !showReset && !showReauth && !showNoCredential) return null;

  return (
    <div className={usageCardSlotClassName.meta}>
      {(showReauth || showNoCredential || showReset) && (
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-1.5 overflow-hidden">
            {showReauth && (
              <span
                className="inline-flex shrink-0 items-center gap-0.5 rounded bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold text-amber-700 ring-1 ring-amber-200/70"
                title={t("usage.reauthRequiredHint")}
              >
                <ShieldAlert className="h-2.5 w-2.5" />
                {t("usage.reauthRequired")}
              </span>
            )}
            {showNoCredential && (
              <span className="shrink-0 rounded bg-rose-50 px-1.5 py-0.5 text-[10px] font-semibold text-rose-600 ring-1 ring-rose-200/60">
                {t("usage.noCredential")}
              </span>
            )}
          </div>
          {showReset && resetInfo ? (
            <ResetCountdown resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
          ) : null}
        </div>
      )}

      {noteText ? (
        <p className="line-clamp-2 text-[11px] leading-snug text-zinc-500" title={noteText}>
          {noteText}
        </p>
      ) : null}
    </div>
  );
}
