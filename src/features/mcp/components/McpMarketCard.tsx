import { ArrowUpCircle, Boxes, Check, Download, Globe, Sparkles, Terminal } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn, formatInstalls } from "../../../lib/utils";
import type { McpMarketEntry, McpServerKind } from "../../../types";
import { CURATED_MCP_PRESENTATION } from "../lib/curatedShelves";
import type { McpEntryStatus } from "../lib/installState";
import { McpDeprecatedBadge, McpSupersededBadge } from "./McpStateBadges";

function kindIcon(kind: McpServerKind) {
  switch (kind) {
    case "stdio":
      return Terminal;
    case "remote":
      return Globe;
    case "both":
      return Boxes;
    default:
      return Boxes;
  }
}

/** The identity fields the icon needs — `McpMarketEntry` and `McpMarketServerDetail` both satisfy it. */
export type McpIconEntry = Pick<McpMarketEntry, "id" | "kind" | "iconUrl">;

/**
 * The card's identity glyph.
 *
 * Curated rows get the service-shaped icon from `CURATED_MCP_PRESENTATION`
 * (the shortlist is a fixed code registry, so the map always resolves).
 * Registry rows prefer their publisher-declared `iconUrl`, falling back to
 * the kind glyph when absent or when the image fails to load.
 */
export function McpEntryIcon({ entry }: { entry: McpIconEntry }) {
  const [imgFailed, setImgFailed] = useState(false);
  const curated = CURATED_MCP_PRESENTATION[entry.id];

  if (curated) {
    const Icon = curated.icon;
    return (
      <div className={cn("flex h-9 w-9 shrink-0 items-center justify-center rounded-xl", curated.wellClass)}>
        <Icon className={cn("h-4 w-4", curated.iconClass)} />
      </div>
    );
  }

  if (entry.iconUrl && !imgFailed) {
    return (
      <div className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-xl bg-muted/50">
        <img
          src={entry.iconUrl}
          alt=""
          loading="lazy"
          onError={() => setImgFailed(true)}
          className="h-5 w-5 object-contain"
        />
      </div>
    );
  }

  const KindIcon = kindIcon(entry.kind);
  const isRemote = entry.kind === "remote";
  return (
    <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary/10">
      <KindIcon
        className={cn(
          "h-4 w-4",
          isRemote ? "text-sky-400 paper:text-sky-600" : "text-emerald-400 paper:text-emerald-600",
        )}
      />
    </div>
  );
}

interface McpMarketCardProps {
  entry: McpMarketEntry;
  /**
   * Installed / behind / deprecated, resolved by fingerprint rather than by the
   * old `installedNames.has(entry.name)` string guess.
   */
  status: McpEntryStatus;
  onInstall: () => void;
  onOpenDetail: () => void;
}

export function McpMarketCard({ entry, status, onInstall, onOpenDetail }: McpMarketCardProps) {
  const { t } = useTranslation();

  const statusAction = (
    <Button
      size="sm"
      variant={status.state === "installed" ? "ghost" : "default"}
      disabled={status.state === "installed"}
      onClick={(e) => {
        e.stopPropagation();
        onInstall();
      }}
      className="h-7 px-2.5 text-xs font-medium"
    >
      {status.state === "installed" ? (
        <>
          <Check className="h-3.5 w-3.5" />
          {t("mcp.presetAdded")}
        </>
      ) : status.state === "updateAvailable" ? (
        <>
          <ArrowUpCircle className="h-3.5 w-3.5" />
          {t("mcp.updateAction")}
        </>
      ) : (
        <>
          <Download className="h-3.5 w-3.5" />
          {t("mcp.install")}
        </>
      )}
    </Button>
  );

  const stars =
    entry.stars > 0 ? (
      <span className="text-xs font-medium text-foreground/60 tabular-nums">{formatInstalls(entry.stars)}</span>
    ) : null;
  const hasFooter = Boolean(status.deprecated || status.superseded || stars);

  return (
    <CardTemplate
      className={cn("group cursor-pointer", status.deprecated && "opacity-80")}
      onClick={onOpenDetail}
      topRightSlot={statusAction}
      headerClassName="pr-24"
      header={
        <div className="flex items-center gap-2.5">
          <McpEntryIcon entry={entry} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5 min-w-0">
              <CardTitle className="truncate ss-card-title">{entry.name}</CardTitle>
              {entry.recommended ? (
                <span title={t("mcp.recommendedBadge")} className="shrink-0 flex items-center">
                  <Sparkles className="h-3.5 w-3.5 text-primary" />
                </span>
              ) : null}
            </div>
            <span className="block truncate ss-card-meta" title={entry.namespace}>
              {entry.namespace}
            </span>
          </div>
        </div>
      }
      bodyClassName="flex-1"
      body={
        <CardDescription className="ss-card-desc">
          {entry.description || t("detailPanel.noDescription")}
        </CardDescription>
      }
      footerClassName={hasFooter ? "ss-card-footer flex items-center mt-auto rounded-b-xl" : undefined}
      footer={
        hasFooter ? (
          <div className="flex min-w-0 items-center gap-2">
            <McpDeprecatedBadge deprecated={status.deprecated} />
            <McpSupersededBadge superseded={status.superseded} />
            {stars}
          </div>
        ) : undefined
      }
    />
  );
}
