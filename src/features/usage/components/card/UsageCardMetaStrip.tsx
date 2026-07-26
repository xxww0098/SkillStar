import { ShieldAlert, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { authModeLabel, formatRelativeSync, type PrimaryResetInfo } from "../../lib/usageLabels";
import type { AuthMode } from "../../types";
import { ResetCountdown, UsagePriorityHint } from "../ResetCountdown";

export interface UsageCardMetaStripProps {
  authMode: AuthMode;
  /** Active state is shown on the header band — kept out of this strip so it never wraps layout. */
  requiresReauth?: boolean;
  supportsCliSwitch?: boolean;
  hasCredential?: boolean;
  note?: string | null;
  fetchedAt: number;
  resetInfo: PrimaryResetInfo | null;
  bodyOwnsPrimaryReset: boolean;
}

/** Auth / reauth / CLI badges · primary reset · last-sync · note · priority hint.
 *  Billing type + active ("当前") live on the brand header band so this strip stays one row. */
export function UsageCardMetaStrip({
  authMode,
  requiresReauth,
  supportsCliSwitch,
  hasCredential,
  note,
  fetchedAt,
  resetInfo,
  bodyOwnsPrimaryReset,
}: UsageCardMetaStripProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-1.5 px-4 pt-2.5">
      {/* Fixed single-row height: never wrap badges onto a second line (that pushed body down). */}
      <div className="flex h-5 items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5 overflow-hidden">
          <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-zinc-600 uppercase ring-1 ring-zinc-200/60">
            {authModeLabel(authMode, t)}
          </span>
          {requiresReauth && (
            <span
              className="inline-flex shrink-0 items-center gap-0.5 rounded bg-amber-50 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-amber-700 uppercase ring-1 ring-amber-200/70"
              title={t("usage.reauthRequiredHint")}
            >
              <ShieldAlert className="h-2.5 w-2.5" />
              {t("usage.reauthRequired")}
            </span>
          )}
          {supportsCliSwitch && (
            <span
              className="inline-flex shrink-0 items-center gap-0.5 rounded bg-sky-50 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-sky-700 uppercase ring-1 ring-sky-200/60"
              title={t("usage.cliSwitchSupportedHint")}
            >
              <Terminal className="h-2.5 w-2.5" />
              CLI
            </span>
          )}
          {hasCredential === false && (
            <span className="shrink-0 rounded bg-rose-50 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-rose-600 uppercase ring-1 ring-rose-200/60">
              {t("usage.noCredential")}
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

      {note?.trim() ? (
        <p className="line-clamp-2 text-[10px] leading-snug text-zinc-500" title={note}>
          {note.trim()}
        </p>
      ) : null}

      {resetInfo && (
        <UsagePriorityHint resetAt={resetInfo.resetAt} usedPercent={resetInfo.usedPercent} mode={resetInfo.mode} />
      )}
    </div>
  );
}
