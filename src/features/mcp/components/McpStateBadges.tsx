import { AlertTriangle, ArrowUpCircle, Check, History } from "lucide-react";
import { useTranslation } from "react-i18next";
import { StatusChip } from "../../../components/ui/StatusChip";
import type { McpEntryStatus } from "../lib/installState";

/**
 * The catalog's three-state marking: already installed, installed but behind,
 * and deprecated by the registry.
 *
 * Deliberately three separate signals rather than one: a deprecated server can
 * also be installed, and an installed one can be both behind *and* deprecated —
 * in which case the update is not the interesting news.
 */

export function McpInstalledBadge({ state }: { state: McpEntryStatus["state"] }) {
  const { t } = useTranslation();
  if (state === "notInstalled") return null;
  if (state === "updateAvailable") {
    return (
      <StatusChip tone="info">
        <ArrowUpCircle className="h-3 w-3" />
        {t("mcp.badgeUpdateAvailable")}
      </StatusChip>
    );
  }
  return (
    <StatusChip tone="success">
      <Check className="h-3 w-3" />
      {t("mcp.badgeInstalled")}
    </StatusChip>
  );
}

export function McpDeprecatedBadge({ deprecated }: { deprecated: boolean }) {
  const { t } = useTranslation();
  if (!deprecated) return null;
  return (
    <StatusChip tone="danger" title={t("mcp.badgeDeprecatedHint")}>
      <AlertTriangle className="h-3 w-3" />
      {t("mcp.badgeDeprecated")}
    </StatusChip>
  );
}

export function McpSupersededBadge({ superseded }: { superseded: boolean }) {
  const { t } = useTranslation();
  if (!superseded) return null;
  return (
    <StatusChip tone="warning" title={t("mcp.badgeSupersededHint")}>
      <History className="h-3 w-3" />
      {t("mcp.badgeSuperseded")}
    </StatusChip>
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
