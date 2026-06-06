import { Boxes, Check, Download, ExternalLink, Globe, Info, Star, Terminal } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import type { McpMarketEntry, McpServerKind } from "../../../types";

function kindBadge(kind: McpServerKind): { icon: typeof Terminal; label: string } | null {
  switch (kind) {
    case "stdio":
      return { icon: Terminal, label: "本地" };
    case "remote":
      return { icon: Globe, label: "远程" };
    case "both":
      return { icon: Boxes, label: "本地 / 远程" };
    default:
      return null;
  }
}

interface McpMarketCardProps {
  entry: McpMarketEntry;
  installed: boolean;
  onInstall: () => void;
  onOpenDetail: () => void;
}

export function McpMarketCard({ entry, installed, onInstall, onOpenDetail }: McpMarketCardProps) {
  const badge = kindBadge(entry.kind);

  return (
    <div className="group flex flex-col gap-2 rounded-xl border border-border/60 bg-card/40 p-4 transition hover:border-primary/30">
      <div className="flex items-center gap-2">
        <Boxes className="h-4 w-4 shrink-0 text-primary" />
        <button
          type="button"
          onClick={onOpenDetail}
          className="truncate text-left text-sm font-semibold text-foreground hover:text-primary"
          title={entry.namespace}
        >
          {entry.name}
        </button>
        {badge ? (
          <span className="ml-auto inline-flex shrink-0 items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
            <badge.icon className="h-3 w-3" />
            {badge.label}
          </span>
        ) : null}
      </div>

      <p className="truncate text-[11px] text-muted-foreground/70" title={entry.namespace}>
        {entry.namespace}
      </p>

      {entry.description ? (
        <p className="line-clamp-2 text-[11px] text-muted-foreground/80">{entry.description}</p>
      ) : null}

      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground/70">
        {entry.stars > 0 ? (
          <span className="inline-flex items-center gap-1">
            <Star className="h-3 w-3" />
            {entry.stars.toLocaleString()}
          </span>
        ) : null}
        {entry.runtimes.map((rt) => (
          <span key={rt} className="rounded bg-muted/70 px-1.5 py-0.5 font-mono">
            {rt}
          </span>
        ))}
        {entry.version ? <span>v{entry.version}</span> : null}
      </div>

      <div className="mt-auto flex items-center justify-between gap-2 pt-1">
        <div className="flex items-center gap-2">
          {entry.repoUrl ? (
            <ExternalAnchor
              href={entry.repoUrl}
              className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
            >
              <ExternalLink className="h-3 w-3" />
              仓库
            </ExternalAnchor>
          ) : null}
          <button
            type="button"
            onClick={onOpenDetail}
            className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
          >
            <Info className="h-3 w-3" />
            详情
          </button>
        </div>
        <Button
          size="sm"
          variant={installed ? "outline" : "default"}
          disabled={installed}
          onClick={onInstall}
          className="gap-1.5"
        >
          {installed ? (
            <>
              <Check className="h-3.5 w-3.5" />
              已安装
            </>
          ) : (
            <>
              <Download className="h-3.5 w-3.5" />
              安装
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
