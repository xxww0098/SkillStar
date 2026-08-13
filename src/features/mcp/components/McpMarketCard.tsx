import {
  ArrowUpCircle,
  Boxes,
  Check,
  Download,
  ExternalLink,
  Globe,
  Info,
  Sparkles,
  Star,
  Terminal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { ZhipuColor } from "../../../components/ui/icons/lobe";
import { LobeIcon } from "../../../components/ui/icons/LobeIcon";
import { cn, formatInstalls } from "../../../lib/utils";
import type { McpMarketEntry, McpServerKind } from "../../../types";
import type { McpEntryStatus } from "../lib/installState";
import { McpDeprecatedBadge, McpSupersededBadge } from "./McpStateBadges";

function kindBadge(kind: McpServerKind): { icon: typeof Terminal; label: string } | null {
  switch (kind) {
    case "stdio":
      return { icon: Terminal, label: "STDIO" };
    case "remote":
      return { icon: Globe, label: "REMOTE" };
    case "both":
      return { icon: Boxes, label: "STDIO / REMOTE" };
    default:
      return null;
  }
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
  compact?: boolean;
}

export function McpMarketCard({ entry, status, onInstall, onOpenDetail, compact }: McpMarketCardProps) {
  const { t } = useTranslation();
  const badge = kindBadge(entry.kind);

  // An update is an action, so the button stays live for it. Plain "installed"
  // is not, and a deprecated row keeps its button — installing one is a
  // legitimate choice, just one that has to pass the wizard's warnings first.
  const statusAction = (
    <Button
      size="sm"
      variant={status.state === "installed" ? "outline" : "default"}
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

  return (
    <CardTemplate
      className={cn("group cursor-pointer", compact && "p-2", status.deprecated && "opacity-80")}
      onClick={onOpenDetail}
      topRightSlot={statusAction}
      headerClassName="pr-24"
      header={
        <div className="flex items-center gap-2.5">
          <div
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary/10",
              entry.recommended && "ring-1 ring-primary/25",
            )}
          >
            {entry.source === "bigmodel" ? (
              <LobeIcon icon={ZhipuColor} size={18} />
            ) : (
              <Boxes className="h-4 w-4 text-primary" />
            )}
          </div>
          <div className="min-w-0">
            <CardTitle className="truncate ss-card-title">{entry.name}</CardTitle>
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
      footerClassName="ss-card-footer flex items-center justify-between mt-auto rounded-b-xl"
      footer={
        <>
          <div className="flex min-w-0 items-center gap-2">
            <McpDeprecatedBadge deprecated={status.deprecated} />
            <McpSupersededBadge superseded={status.superseded} />
            {entry.recommended ? (
              <span className="inline-flex h-4 items-center gap-1 rounded bg-primary/12 px-1.5 text-micro font-medium text-primary ring-1 ring-inset ring-primary/20">
                <Sparkles className="h-3 w-3" />
                {t("mcp.recommendedBadge")}
              </span>
            ) : null}
            {entry.stars > 0 ? (
              <span className="inline-flex items-center gap-1 text-xs font-medium text-muted-foreground tabular-nums">
                <Star className="h-3.5 w-3.5 text-primary/60" />
                {formatInstalls(entry.stars)}
              </span>
            ) : null}
            {badge ? (
              <span className="inline-flex h-4 items-center gap-1 rounded bg-muted/70 px-1.5 text-micro text-muted-foreground">
                <badge.icon className="h-3 w-3" />
                {badge.label}
              </span>
            ) : null}
            {entry.runtimes.slice(0, 2).map((rt) => (
              <span key={rt} className="rounded bg-muted/70 px-1.5 py-0.5 font-mono text-micro text-muted-foreground">
                {rt}
              </span>
            ))}
            {entry.version ? <span className="text-micro text-muted-foreground/70">v{entry.version}</span> : null}
            {status.state === "updateAvailable" && status.installedVersion ? (
              <span className="text-micro text-sky-600 dark:text-sky-400">
                {t("mcp.updateFromVersion", { version: status.installedVersion })}
              </span>
            ) : null}
          </div>

          <div className="relative z-10 flex shrink-0 items-center gap-2">
            {entry.repoUrl ? (
              <ExternalAnchor
                href={entry.repoUrl}
                onClick={(e) => e.stopPropagation()}
                className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
              >
                <ExternalLink className="h-3 w-3" />
                {t("mcp.repo")}
              </ExternalAnchor>
            ) : null}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onOpenDetail();
              }}
              className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
            >
              <Info className="h-3 w-3" />
              {t("common.details")}
            </button>
          </div>
        </>
      }
    />
  );
}
