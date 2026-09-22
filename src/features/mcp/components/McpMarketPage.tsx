import { Boxes, ChevronLeft, ChevronRight, Database, PackageSearch, RefreshCw, SlidersHorizontal } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageToolbar } from "../../../components/layout/PageToolbar";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { toast } from "../../../lib/toast";
import { cn } from "../../../lib/utils";
import type { McpInstallOutcome } from "../../../types";
import { useMcpMarketPage } from "../hooks/useMcpMarketPage";
import { type McpMarketInstallSubmission, useMcpServers } from "../hooks/useMcpServers";
import { useMcpSources } from "../hooks/useMcpSources";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import { mcpEnabledMapFromProfiles, selectMcpAgentTargets } from "../lib/agentTargets";
import { groupMcpMarketShelves } from "../lib/curatedShelves";
import { buildInstalledIndex } from "../lib/installState";
import { DEFAULT_MCP_MARKET_FILTERS, hasActiveMcpNarrowing } from "../lib/marketQuery";
import { failedMcpSyncCount } from "../lib/syncResults";
import { McpCatalogHealthBanner } from "./McpCatalogHealthBanner";
import { McpInstallWizard } from "./McpInstallWizard";
import { McpMarketBrowser } from "./McpMarketBrowser";
import { McpMarketFilters } from "./McpMarketFilters";

/**
 * MCP store — what you can get, and how to get it.
 *
 * The store used to open on a publisher grid that existed only to drill into
 * one publisher's bucket, which made a two-click trip out of a one-click
 * question and put a whole extra navigation layer in front of the servers.
 * Publishers are now a scope, not a page: *Curated* is the recommended
 * shortlist we seed — rendered as labeled shelves (`core` / `context` /
 * `browser`) — and *Full catalog* is the ~21k-row remote registry (`github`),
 * the flat filterable grid.
 *
 * The remote registry is never held in memory: search, every filter, the sort
 * order and the page window all compile into one backend query, and the row
 * count shown ("1–60 of 21363") is the backend's pre-pagination total.
 *
 * This page owns every page-level state — loading, empty, remote-error with its
 * retry, pagination — so `McpMarketBrowser` stays a pure card renderer.
 */

type StoreScope = "curated" | "registry";

const SCOPES: Array<{ id: StoreScope; label: string }> = [
  { id: "curated", label: "mcp.scopeCurated" },
  { id: "registry", label: "mcp.scopeRegistry" },
];

interface McpMarketPageProps {
  /** View switch when this page is the MCP store surface. */
  title?: ReactNode;
  /** Open the catalog-source inspector. */
  onOpenSources?: () => void;
}

export function McpMarketPage({ title, onOpenSources }: McpMarketPageProps) {
  const { t } = useTranslation();
  const [scope, setScope] = useState<StoreScope>("curated");
  const [showFilters, setShowFilters] = useState(false);
  const [installId, setInstallId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const browsingRegistry = scope === "registry";
  const market = useMcpMarketPage({
    curatedOnly: !browsingRegistry,
    publisherId: browsingRegistry ? "github" : null,
  });
  const { servers, installFromMarket } = useMcpServers();
  const { profiles } = useAgentProfiles();
  const { health } = useMcpSources();
  const { noteForTool } = useMcpToolStatuses();

  const installedIndex = useMemo(() => buildInstalledIndex(servers), [servers]);
  const agentTargets = useMemo(() => selectMcpAgentTargets(profiles), [profiles]);

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
  const remoteError = market.snapshotStatus === "remote_error";
  const searchPlaceholder = t(browsingRegistry ? "mcp.marketSearchPlaceholder" : "mcp.officialSearchPlaceholder");

  const closeInstall = () => {
    if (!saving) setInstallId(null);
  };

  /**
   * Curated is an eight-row shortlist — the filter panel (kind / license /
   * stars) exists for the ~21k registry, so it hides there. Switching scope
   * also drops any narrowing but the search text: a filter set in one scope
   * that silently follows into the other would read as missing rows.
   */
  const handleScopeChange = (next: StoreScope) => {
    if (next === scope) return;
    setScope(next);
    setShowFilters(false);
    market.setFilters((prev) => ({ ...DEFAULT_MCP_MARKET_FILTERS, search: prev.search }));
  };

  const sections = useMemo(
    () => (browsingRegistry ? undefined : groupMcpMarketShelves(market.items)),
    [browsingRegistry, market.items],
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <PageToolbar
        title={title}
        search={
          <SearchInput
            containerClassName="w-72"
            value={market.filters.search}
            onChange={(event) => market.setFilters((prev) => ({ ...prev, search: event.target.value }))}
            placeholder={searchPlaceholder}
            className="h-8 bg-sidebar/50 text-xs focus-visible:bg-background"
            iconClassName="left-2.5"
          />
        }
        filters={
          <>
            <div
              role="group"
              aria-label={t("mcp.storeScope")}
              className="flex h-8 items-center rounded-lg border border-border/70 bg-sidebar/30 p-0.5"
            >
              {SCOPES.map(({ id, label }) => (
                <button
                  key={id}
                  type="button"
                  aria-pressed={scope === id}
                  onClick={() => handleScopeChange(id)}
                  className={cn(
                    "inline-flex h-full cursor-pointer items-center rounded-md px-2.5 text-xs transition-colors duration-150 focus-ring select-none",
                    scope === id
                      ? "bg-accent font-semibold text-accent-foreground"
                      : "font-medium text-muted-foreground hover:bg-sidebar-hover hover:text-foreground",
                  )}
                >
                  {t(label)}
                </button>
              ))}
            </div>
            {browsingRegistry ? (
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
            ) : null}
          </>
        }
        actions={
          <>
            {onOpenSources ? (
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                onClick={onOpenSources}
                title={t("mcp.sourcesTitle")}
                aria-label={t("mcp.sourcesTitle")}
              >
                <Database className="h-3.5 w-3.5" />
              </Button>
            ) : null}
            <span className="text-xs tabular-nums text-muted-foreground">
              {pageWindow.total > 0
                ? t("mcp.showingRange", { from: pageWindow.from, to: pageWindow.to, total: pageWindow.total })
                : t("mcp.showingNone")}
            </span>
          </>
        }
      />

      <main className="ss-page-scroll">
        <div className="ss-page-stack">
          {browsingRegistry ? (
            <McpCatalogHealthBanner
              health={health}
              onRefresh={() => void market.refresh()}
              refreshing={market.refreshing}
            />
          ) : null}

          {showFilters ? (
            <McpMarketFilters
              filters={market.filters}
              onChange={(next) => market.setFilters(next)}
              onReset={market.resetFilters}
            />
          ) : null}

          {market.isLoading || (market.snapshotStatus === "seeding" && market.items.length === 0) ? (
            <div className="flex items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("mcp.marketLoading")} />
            </div>
          ) : market.items.length === 0 ? (
            <EmptyState
              icon={<Boxes className="h-6 w-6 text-muted-foreground" />}
              title={narrowed ? t("mcp.marketNoMatches") : t("mcp.marketEmptyTitle")}
              description={
                remoteError
                  ? t("mcp.marketRemoteErrorDescription")
                  : narrowed
                    ? t("mcp.marketNoMatchesDescription")
                    : t(browsingRegistry ? "mcp.marketEmptyDescription" : "mcp.officialEmptyDescription")
              }
              action={
                remoteError ? (
                  <Button
                    variant="outline"
                    onClick={() => void market.refresh()}
                    disabled={market.refreshing}
                    className="gap-1.5"
                  >
                    <RefreshCw className={market.refreshing ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
                    {t("common.retry")}
                  </Button>
                ) : narrowed ? (
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
                sections={sections}
                onInstall={setInstallId}
              />

              {pageWindow.pageCount > 1 ? (
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
              ) : null}
            </>
          )}
        </div>
      </main>

      <ModalShell
        open={installId != null}
        onClose={closeInstall}
        ariaLabel={t("mcp.installWizardTitle")}
        dismissable={!saving}
        panelClassName="max-w-[760px]"
        surfaceClassName="flex max-h-[min(780px,calc(100vh-2rem))] flex-col overflow-hidden"
        contentClassName="flex min-h-0 flex-col"
      >
        <ModalHeader
          icon={<PackageSearch className="h-4 w-4 text-primary" />}
          title={t("mcp.installWizardTitle")}
          onClose={closeInstall}
          closeDisabled={saving}
        />
        <p className="shrink-0 px-6 pb-2 text-[11px] leading-relaxed text-muted-foreground">
          {t("mcp.installWizardSubtitle")}
        </p>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
          {installId ? (
            <McpInstallWizard
              key={installId}
              serverId={installId}
              submitting={saving}
              onSubmit={handleInstall}
              onCancel={closeInstall}
              noteForTool={noteForTool}
              defaultEnabled={mcpEnabledMapFromProfiles(profiles)}
              targets={agentTargets}
            />
          ) : null}
        </div>
      </ModalShell>
    </div>
  );
}
