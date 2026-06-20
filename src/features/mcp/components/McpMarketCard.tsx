import ZhipuColor from "@lobehub/icons/es/Zhipu/components/Color";
import { Boxes, Check, Download, ExternalLink, Globe, Info, Sparkles, Star, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { cn, formatInstalls } from "../../../lib/utils";
import type { McpMarketEntry, McpServerKind } from "../../../types";

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
  installed: boolean;
  onInstall: () => void;
  onOpenDetail: () => void;
  compact?: boolean;
}

export function McpMarketCard({ entry, installed, onInstall, onOpenDetail, compact }: McpMarketCardProps) {
  const { t } = useTranslation();
  const badge = kindBadge(entry.kind);

  const statusAction = (
    <Button
      size="sm"
      variant={installed ? "outline" : "default"}
      disabled={installed}
      onClick={(e) => {
        e.stopPropagation();
        onInstall();
      }}
      className="h-7 px-2.5 text-xs font-medium"
    >
      {installed ? (
        <>
          <Check className="h-3.5 w-3.5" />
          {t("mcp.presetAdded")}
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
      className={cn("group cursor-pointer", compact && "p-2")}
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
            {entry.source === "bigmodel" ? <ZhipuColor size={18} /> : <Boxes className="h-4 w-4 text-primary" />}
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
