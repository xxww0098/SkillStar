import { Boxes, ChevronLeft, ChevronRight, PackageSearch, SlidersHorizontal } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DrawerShell } from "../../../components/shared/DrawerShell";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { cn } from "../../../lib/utils";
import { toast } from "../../../lib/toast";
import type { McpInstallOutcome, ViewMode } from "../../../types";
import { useMcpMarketPage } from "../hooks/useMcpMarketPage";
import { type McpMarketInstallSubmission, useMcpServers } from "../hooks/useMcpServers";
import { useMcpSources } from "../hooks/useMcpSources";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import { buildInstalledIndex } from "../lib/installState";
import { hasActiveMcpNarrowing } from "../lib/marketQuery";
import { failedMcpSyncCount } from "../lib/syncResults";
import { McpCatalogHealthBanner } from "./McpCatalogHealthBanner";
import { McpInstallWizard } from "./McpInstallWizard";
import { McpMarketBrowser } from "./McpMarketBrowser";
import { McpMarketFilters } from "./McpMarketFilters";

/**
 * Global MCP catalog browse.
 *
 * The catalog is the merge of every enabled source — currently ~21k servers —
 * so this page never holds it in memory: search, every filter, the sort order
 * and the page window all compile into one backend query, and the row count
 * shown ("1–60 of 21363") is the backend's pre-pagination total. Previously the
 * only way in was per publisher, with a three-field substring match over an
 * already-fetched array while the snapshot's FTS index went unused
 * (audit D.3-1/2/3).
 */

interface McpMarketPageProps {
  /** Scope to one publisher bucket; omit for the whole merged catalog. */
  publisherId?: string | null;
  className?: string;
}

export function McpMarketPage({ publisherId = null, className }: McpMarketPageProps) {
  const { t } = useTranslation();
  const [showFilters, setShowFilters] = useState(false);
  const [installId, setInstallId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [viewMode] = useState<ViewMode>("grid");

  const market = useMcpMarketPage({ publisherId });
  const { servers, installFromMarket } = useMcpServers();
  const { health } = useMcpSources();
  const { noteForTool } = useMcpToolStatuses();

  const installedIndex = useMemo(() => buildInstalledIndex(servers), [servers]);

  /**
   * A refused install is not an error: the wizard keeps the drawer open and
   * says which of the two refusals it was, so the verdict is handed back rather
   * than toasted. Only a genuine failure gets a toast.
   */
  const handleInstall = async (submission: McpMarketInstallSubmission): Promise<McpInstallOutcome> => {
    setSaving(true);
    try {
      const outcome = await installFromMarket(submission);
      if (outcome.status === "installed") {
        const failed = failedMcpSyncCount(outcome.installed.syncResults);
        if (failed > 0) toast.warning(t("mcp.syncPartial", { count: failed }));
        else toast.success(t("mcp.added"));
        setInstallId(null);
      }
      return outcome;
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSaving(false);
    }
  };

  const { window: pageWindow } = market;
  const narrowed = hasActiveMcpNarrowing(market.filters);

  return (
    <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden", className)}>
      <div className="flex items-center gap-2 border-b border-border/60 px-6 py-2.5">
        <SearchInput
          containerClassName="w-72"
          value={market.filters.search}
          onChange={(event) => market.setFilters((prev) => ({ ...prev, search: event.target.value }))}
          placeholder={t("mcp.marketSearchPlaceholder")}
          className="h-8 bg-sidebar/50 text-xs focus-visible:bg-background"
          iconClassName="left-2.5"
        />
        <Button
          type="button"
          variant={showFilters ? "default" : "outline"}
          size="sm"
          className="h-8 gap-1.5"
          onClick={() => setShowFilters((prev) => !prev)}
        >
          <SlidersHorizontal className="h-3.5 w-3.5" />
          {t("mcp.filtersTitle")}
        </Button>
        <span className="ml-auto text-xs tabular-nums text-muted-foreground">
          {pageWindow.total > 0
            ? t("mcp.showingRange", { from: pageWindow.from, to: pageWindow.to, total: pageWindow.total })
            : t("mcp.showingNone")}
        </span>
      </div>

      <main className="ss-page-scroll">
        <div className="ss-page-stack">
          <McpCatalogHealthBanner
            health={health}
            onRefresh={() => void market.refresh()}
            refreshing={market.refreshing}
          />

          {showFilters ? (
            <McpMarketFilters
              filters={market.filters}
              onChange={(next) => market.setFilters(next)}
              onReset={market.resetFilters}
            />
          ) : null}

          {market.isLoading ? (
            <div className="flex items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("mcp.marketLoading")} />
            </div>
          ) : market.items.length === 0 ? (
            <EmptyState
              icon={<Boxes className="h-6 w-6 text-muted-foreground" />}
              title={narrowed ? t("mcp.marketNoMatches") : t("mcp.marketEmptyTitle")}
              description={narrowed ? t("mcp.marketNoMatchesDescription") : t("mcp.marketEmptyDescription")}
              action={
                narrowed ? (
                  <Button variant="outline" onClick={market.resetFilters}>
                    {t("mcp.filtersClearAll")}
                  </Button>
                ) : null
              }
              size="lg"
            />
          ) : (
            <>
              <McpMarketBrowser
                installedIndex={installedIndex}
                entries={market.items}
                status={market.snapshotStatus}
                isLoading={false}
                query={market.filters.search}
                refreshing={market.refreshing}
                viewMode={viewMode}
                onRefresh={() => void market.refresh()}
                onInstall={setInstallId}
              />

              <div className="flex items-center justify-center gap-3 pb-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-8 gap-1"
                  onClick={market.prevPage}
                  disabled={!pageWindow.hasPrev || market.isFetching}
                >
                  <ChevronLeft className="h-3.5 w-3.5" />
                  {t("mcp.pagePrev")}
                </Button>
                <span className="text-xs tabular-nums text-muted-foreground">
                  {t("mcp.pageOf", { page: pageWindow.pageIndex + 1, pages: pageWindow.pageCount })}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-8 gap-1"
                  onClick={market.nextPage}
                  disabled={!pageWindow.hasNext || market.isFetching}
                >
                  {t("mcp.pageNext")}
                  <ChevronRight className="h-3.5 w-3.5" />
                </Button>
              </div>
            </>
          )}
        </div>
      </main>

      <DrawerShell
        open={installId != null}
        onOpenChange={(open) => {
          if (!open) setInstallId(null);
        }}
        title={
          <span className="flex items-center gap-2 text-foreground">
            <PackageSearch className="h-4 w-4 text-primary" />
            {t("mcp.installWizardTitle")}
          </span>
        }
        subtitle={t("mcp.installWizardSubtitle")}
      >
        {installId ? (
          <McpInstallWizard
            key={installId}
            serverId={installId}
            submitting={saving}
            onSubmit={handleInstall}
            onCancel={() => setInstallId(null)}
            noteForTool={noteForTool}
          />
        ) : null}
      </DrawerShell>
    </div>
  );
}
