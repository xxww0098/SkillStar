import { useQuery } from "@tanstack/react-query";
import { Download, ExternalLink, Globe, RefreshCw, Star, Terminal } from "lucide-react";
import { type CSSProperties, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { Markdown } from "../../../components/ui/Markdown";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { LocalFirstResult, McpMarketEntry, McpMarketServerDetail, SnapshotStatus, ViewMode } from "../../../types";
import { DrawerShell } from "../../models";
import { McpMarketCard } from "./McpMarketCard";

const GRID_GAP_PX = 16;
const MCP_MIN_COLUMN_WIDTH = 320;

interface McpMarketBrowserProps {
  /** Names of already-installed MCP servers, for the "已安装" badge. */
  installedNames: Set<string>;
  /** Open the create form prefilled from this marketplace entry id. */
  onInstall: (id: string) => void;
  entries: McpMarketEntry[];
  status?: SnapshotStatus;
  isLoading: boolean;
  query: string;
  refreshing: boolean;
  onRefresh: () => void;
  viewMode: ViewMode;
}

export function McpMarketBrowser({
  installedNames,
  onInstall,
  entries,
  status,
  isLoading,
  query,
  refreshing,
  onRefresh,
  viewMode,
}: McpMarketBrowserProps) {
  const { t } = useTranslation();
  const [detailId, setDetailId] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  const prevColCountRef = useRef(0);

  const gridColumnCount = useMemo(() => {
    if (viewMode !== "grid") return 1;
    if (containerWidth === 0) return prevColCountRef.current || 1;

    const safeMinWidth = Math.max(220, MCP_MIN_COLUMN_WIDTH);
    let cols = Math.max(1, Math.floor((containerWidth + GRID_GAP_PX) / (safeMinWidth + GRID_GAP_PX)));
    if (prevColCountRef.current > 0 && cols < prevColCountRef.current) {
      const thresholdForPrev = prevColCountRef.current * (safeMinWidth + GRID_GAP_PX) - GRID_GAP_PX;
      if (containerWidth >= thresholdForPrev - 8) {
        cols = prevColCountRef.current;
      }
    }
    prevColCountRef.current = cols;
    return cols;
  }, [containerWidth, viewMode]);

  const hasEntries = entries.length > 0;
  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const updateWidth = () => setContainerWidth(element.clientWidth);
    updateWidth();

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, [hasEntries]);

  const gridStyle: CSSProperties | undefined =
    viewMode === "grid" && gridColumnCount > 0
      ? { gridTemplateColumns: `repeat(${gridColumnCount}, minmax(0, 1fr))` }
      : undefined;

  const detailQuery = useQuery<LocalFirstResult<McpMarketServerDetail | null>>({
    queryKey: ["mcp-market", "detail", detailId],
    queryFn: () => tauriInvoke("get_mcp_market_server_detail_local", { id: detailId as string }),
    enabled: detailId != null,
  });
  const detail = detailQuery.data?.data ?? null;

  return (
    <section className="ss-page-stack">
      {isLoading || status === "seeding" ? (
        <div className="flex items-center justify-center py-20">
          <LoadingLogo size="lg" label={t("mcp.marketLoading")} />
        </div>
      ) : entries.length === 0 ? (
        <EmptyState
          title={query ? t("mcp.marketNoMatches") : t("mcp.marketEmptyTitle")}
          description={
            status === "remote_error"
              ? t("mcp.marketRemoteErrorDescription")
              : query
                ? t("mcp.marketNoMatchesDescription")
                : t("mcp.marketEmptyDescription")
          }
          action={
            status === "remote_error" ? (
              <Button variant="outline" onClick={onRefresh} disabled={refreshing} className="gap-1.5">
                <RefreshCw className={refreshing ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
                {t("common.retry")}
              </Button>
            ) : null
          }
          size="lg"
        />
      ) : (
        <div
          ref={containerRef}
          className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")}
          style={gridStyle}
        >
          {entries.map((entry) => (
            <div key={entry.id} className="h-full">
              <McpMarketCard
                entry={entry}
                installed={installedNames.has(entry.name)}
                compact={viewMode === "list"}
                onInstall={() => onInstall(entry.id)}
                onOpenDetail={() => setDetailId(entry.id)}
              />
            </div>
          ))}
        </div>
      )}

      <DrawerShell
        open={detailId != null}
        onOpenChange={(open) => {
          if (!open) setDetailId(null);
        }}
        title={<span className="text-foreground">{detail?.name ?? t("mcp.title")}</span>}
        subtitle={detail?.namespace}
      >
        {detailQuery.isLoading ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
            {t("common.loading")}
          </div>
        ) : detail ? (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
              {detail.stars > 0 ? (
                <span className="inline-flex items-center gap-1">
                  <Star className="h-3.5 w-3.5" />
                  {detail.stars.toLocaleString()}
                </span>
              ) : null}
              {detail.license ? <span>{detail.license}</span> : null}
              {detail.version ? <span>v{detail.version}</span> : null}
              {detail.repoUrl ? (
                <ExternalAnchor href={detail.repoUrl} className="inline-flex items-center gap-1 hover:text-foreground">
                  <ExternalLink className="h-3.5 w-3.5" />
                  {t("mcp.repo")}
                </ExternalAnchor>
              ) : null}
            </div>

            {detail.description ? <p className="text-sm text-muted-foreground">{detail.description}</p> : null}

            <Button
              onClick={() => {
                const id = detail.id;
                setDetailId(null);
                onInstall(id);
              }}
              disabled={installedNames.has(detail.name)}
              className="w-full gap-1.5"
            >
              <Download className="h-4 w-4" />
              {installedNames.has(detail.name) ? t("mcp.presetAdded") : t("mcp.installToTools")}
            </Button>

            {detail.packages.length > 0 ? (
              <div className="space-y-2">
                <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Terminal className="h-3.5 w-3.5" />
                  {t("mcp.localRun")}
                </h4>
                {detail.packages.map((pkg) => (
                  <div
                    key={`${pkg.runtime}-${pkg.identifier}`}
                    className="rounded-lg border border-border/50 bg-card/40 p-3 text-xs"
                  >
                    <code className="font-mono text-foreground">
                      {pkg.runtime} {pkg.identifier}
                      {pkg.version ? `@${pkg.version}` : ""}
                    </code>
                    {pkg.requiredEnv.length > 0 ? (
                      <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                        {t("mcp.requiredEnv", { keys: pkg.requiredEnv.join(", ") })}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}

            {detail.remotes.length > 0 ? (
              <div className="space-y-2">
                <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Globe className="h-3.5 w-3.5" />
                  {t("mcp.remoteEndpoints")}
                </h4>
                {detail.remotes.map((remote) => (
                  <div key={remote.url} className="rounded-lg border border-border/50 bg-card/40 p-3 text-xs">
                    <code className="break-all font-mono text-foreground">
                      [{remote.transport}] {remote.url}
                    </code>
                    {remote.requiredHeaders.length > 0 ? (
                      <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                        {t("mcp.requiredHeaders", { keys: remote.requiredHeaders.join(", ") })}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}

            {detail.readme ? (
              <div className="space-y-2">
                <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">README</h4>
                <div className="max-h-[40vh] overflow-y-auto rounded-lg border border-border/50 bg-card/30 p-3">
                  <Markdown>{detail.readme}</Markdown>
                </div>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">{t("mcp.notFound")}</div>
        )}
      </DrawerShell>
    </section>
  );
}
