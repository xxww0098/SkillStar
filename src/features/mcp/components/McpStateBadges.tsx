import { AlertTriangle, ArrowUpCircle, Check, History } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import type { McpEntryStatus } from "../lib/installState";

/**
 * The catalog's three-state marking: already installed, installed but behind,
 * and deprecated by the registry.
 *
 * Deliberately three separate signals rather than one: a deprecated server can
 * also be installed, and an installed one can be both behind *and* deprecated —
 * in which case the update is not the interesting news.
 */

const chip = "inline-flex h-5 items-center gap-1 rounded px-1.5 text-micro font-medium ring-1 ring-inset";

export function McpInstalledBadge({ state }: { state: McpEntryStatus["state"] }) {
  const { t } = useTranslation();
  if (state === "notInstalled") return null;
  if (state === "updateAvailable") {
    return (
      <span className={cn(chip, "bg-sky-500/12 text-sky-600 ring-sky-500/25 dark:text-sky-400")}>
        <ArrowUpCircle className="h-3 w-3" />
        {t("mcp.badgeUpdateAvailable")}
      </span>
    );
  }
  return (
    <span className={cn(chip, "bg-emerald-500/12 text-emerald-600 ring-emerald-500/25 dark:text-emerald-400")}>
      <Check className="h-3 w-3" />
      {t("mcp.badgeInstalled")}
    </span>
  );
}

export function McpDeprecatedBadge({ deprecated }: { deprecated: boolean }) {
  const { t } = useTranslation();
  if (!deprecated) return null;
  return (
    <span
      className={cn(chip, "bg-destructive/12 text-destructive ring-destructive/25")}
      title={t("mcp.badgeDeprecatedHint")}
    >
      <AlertTriangle className="h-3 w-3" />
      {t("mcp.badgeDeprecated")}
    </span>
  );
}

export function McpSupersededBadge({ superseded }: { superseded: boolean }) {
  const { t } = useTranslation();
  if (!superseded) return null;
  return (
    <span
      className={cn(chip, "bg-amber-500/12 text-amber-600 ring-amber-500/25 dark:text-amber-400")}
      title={t("mcp.badgeSupersededHint")}
    >
      <History className="h-3 w-3" />
      {t("mcp.badgeSuperseded")}
    </span>
  );
}

/** All three, in the order a card should read them. */
export function McpEntryBadges({ status }: { status: McpEntryStatus }) {
  return (
    <>
      <McpDeprecatedBadge deprecated={status.deprecated} />
      <McpSupersededBadge superseded={status.superseded} />
      <McpInstalledBadge state={status.state} />
    </>
  );
}
