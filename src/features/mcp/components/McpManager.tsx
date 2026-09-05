import { Boxes, Download, PackageSearch, Plug, RefreshCw, Search } from "lucide-react";
import { type CSSProperties, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageToolbar } from "../../../components/layout/PageToolbar";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { AgentFilterPill } from "../../../components/ui/AgentFilterPill";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { toast } from "../../../lib/toast";
import { tauriInvoke } from "../../../lib/ipc";
import { mcpImportPasteText, type McpImportRequest } from "../../../lib/deepLink";
import type {
  McpInstallOutcome,
  McpPasteParse,
  McpPreset,
  McpServerEntry,
  McpServerWithSync,
  McpSyncResult,
  McpToolId,
} from "../../../types";
import { useMcpCatalogUpdates } from "../hooks/useMcpCatalogUpdates";
import { useMcpFleetProbe, useMcpProbe } from "../hooks/useMcpProbe";
import { type McpMarketInstallSubmission, useMcpServers } from "../hooks/useMcpServers";
import { useMcpPresets } from "../hooks/useMcpPresets";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import { mcpEnabledMapFromProfiles, resolveMcpToolFilter, selectMcpAgentTargets } from "../lib/agentTargets";
import { mcpDraftToFormValue, mcpServerCommandLine } from "../lib/pasteDraft";
import { type McpFleetHealthFilter, mcpFleetStatus, mcpFleetStatusMatches } from "../lib/fleetStatus";
import { failedMcpSyncCount, mergeMcpSyncResults, summarizeMcpSyncResults } from "../lib/syncResults";
import { McpFleetCard } from "./McpFleetCard";
import { McpFleetStrip } from "./McpFleetStrip";
import { McpImportBar } from "./McpImportBar";
import { McpInstallWizard } from "./McpInstallWizard";
import { McpProbePanel } from "./McpProbePanel";
import { McpRecommendedPresets } from "./McpRecommendedPresets";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";
import { McpSyncResultsPanel } from "./McpSyncResultsPanel";

/**
 * Map a *built-in* preset into create-form seed values.
 *
 * Only built-ins take this path. A curated preset carries `catalogId` and opens
 * the install wizard instead, so it gets the runtime-shape picker, masked
 * secret fields and the command confirmation that the store tab already gives
 * the same catalog row — seeding this form would drop all three and write the
 * server's API key as a plaintext line in a multi-line textarea.
 */
function presetToDefaults(preset: McpPreset, enabled: Record<string, boolean>): Partial<McpServerFormValue> {
  return {
    name: preset.name,
    transport: preset.transport,
    command: preset.command ?? undefined,
    args: preset.args,
    env: preset.env,
    url: preset.url ?? undefined,
    headers: preset.headers,
    description: preset.description,
    homepage: preset.homepage,
    enabled,
  };
}

type DrawerMode =
  | { type: "closed" }
  | { type: "create" }
  | { type: "edit"; id: string }
  /** A curated preset chip, installed through the same wizard as the store tab. */
  | { type: "install"; catalogId: string };

/** The last sync batch, kept so its per-target detail stays inspectable. */
interface SyncBatch {
  title: string;
  serverId: string | null;
  results: McpSyncResult[];
}

interface McpManagerProps {
  /** Navigate to the unified Marketplace MCP tab. */
  onOpenMarket?: () => void;
  importRequest?: McpImportRequest | null;
  onImportRequestHandled?: () => void;
}

function matchesQuery(query: string, values: Array<string | string[] | undefined | null>): boolean {
  if (!query) return true;
  return values.some((value) => {
    if (!value) return false;
    const text = Array.isArray(value) ? value.join(" ") : value;
    return text.toLowerCase().includes(query);
  });
}

const GRID_GAP_PX = 16;
const MCP_MIN_COLUMN_WIDTH = 320;

export function McpManager({ onOpenMarket, importRequest, onImportRequestHandled }: McpManagerProps) {
  const { t } = useTranslation();
  const { profiles } = useAgentProfiles();
  const {
    servers,
    isLoading,
    error,
    createServer,
    updateServer,
    deleteServer,
    toggleTool,
    syncAll,
    syncServer,
    installFromMarket,
    importFromTools,
    syncing,
    retrySyncing,
    importing,
  } = useMcpServers();
  const { presets } = useMcpPresets();
  const { noteForTool } = useMcpToolStatuses();
  const updates = useMcpCatalogUpdates(servers);
  const probe = useMcpProbe();
  useMcpFleetProbe(
    servers.map((server) => server.id),
    probe.probeFleet,
  );
  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [saving, setSaving] = useState(false);
  const [batch, setBatch] = useState<SyncBatch | null>(null);
  // Seed values + a nonce key so picking a preset re-mounts the create form
  // (the form only reads `defaults` on mount).
  const [createSeed, setCreateSeed] = useState<{ key: number; defaults?: Partial<McpServerFormValue> }>({ key: 0 });
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  // Active tool filter: only show servers synced into this tool (null = all).
  const [toolFilter, setToolFilter] = useState<string | null>(null);
  const [healthFilter, setHealthFilter] = useState<McpFleetHealthFilter>("all");
  const [dropping, setDropping] = useState(false);
  const [pasteSeed, setPasteSeed] = useState({ key: 0, text: "" });
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  const prevColCountRef = useRef(0);
  const agentTargets = useMemo(() => selectMcpAgentTargets(profiles), [profiles]);
  const activeToolFilter = resolveMcpToolFilter(toolFilter, agentTargets);

  const gridColumnCount = useMemo(() => {
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
  }, [containerWidth]);

  const filteredServers = useMemo(
    () =>
      servers.filter((server) => {
        if (activeToolFilter && !server.enabled[activeToolFilter]) return false;
        if (!mcpFleetStatusMatches(mcpFleetStatus(probe.entryFor(server.id)), healthFilter)) return false;
        return matchesQuery(normalizedQuery, [
          server.name,
          server.description,
          server.homepage,
          server.transport,
          server.tags,
          mcpServerCommandLine(server),
        ]);
      }),
    [servers, normalizedQuery, activeToolFilter, healthFilter, probe.entryFor],
  );

  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const updateWidth = () => setContainerWidth(element.clientWidth);
    updateWidth();

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, [filteredServers.length]);

  const gridStyle = useMemo<CSSProperties>(() => {
    if (gridColumnCount > 0) {
      return {
        gridTemplateColumns: `repeat(${gridColumnCount}, minmax(0, 1fr))`,
      };
    }
    return {};
  }, [gridColumnCount]);

  // The toolbar uses the same Settings-backed target set as every MCP card.
  const toolFilterItems = useMemo(
    () =>
      agentTargets.map(({ toolId, profile }) => ({
        id: toolId,
        icon: profile.icon,
        display_name: profile.display_name,
      })),
    [agentTargets],
  );

  const installedNames = useMemo(() => new Set(servers.map((server) => server.name.trim().toLowerCase())), [servers]);

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;
  const batchReport = useMemo(() => (batch ? summarizeMcpSyncResults(batch.results) : null), [batch]);

  const openCreate = () => {
    setSelectedPresetId(null);
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: { enabled: mcpEnabledMapFromProfiles(profiles) } }));
    setDrawer({ type: "create" });
  };

  /**
   * Curated chip → install wizard, built-in chip → create form.
   *
   * Routed on the explicit `catalogId` marker, never on "open the wizard and
   * fall back if the row does not resolve": a built-in preset has no catalog
   * row at all, and a transient catalog read must not decide whether its entry
   * point still works.
   */
  const pickPreset = (preset: McpPreset) => {
    setSelectedPresetId(preset.id);
    if (preset.catalogId) {
      setDrawer({ type: "install", catalogId: preset.catalogId });
      return;
    }
    setCreateSeed((prev) => ({
      key: prev.key + 1,
      defaults: presetToDefaults(preset, mcpEnabledMapFromProfiles(profiles)),
    }));
    setDrawer({ type: "create" });
  };

  const applyPaste = (parsed: McpPasteParse) => {
    setSelectedPresetId(null);
    if (parsed.catalogId) {
      setDrawer({ type: "install", catalogId: parsed.catalogId });
      return;
    }
    const drafts = parsed.drafts ?? [];
    if (drafts.length === 0) {
      toast.error(parsed.error ?? t("mcp.pasteUnknown"));
      return;
    }
    if (drafts.length > 1) {
      toast.info(t("mcp.pasteMultiple", { count: drafts.length }));
    }
    const enabled = mcpEnabledMapFromProfiles(profiles);
    setCreateSeed((prev) => ({
      key: prev.key + 1,
      defaults: mcpDraftToFormValue(drafts[0], enabled),
    }));
    setDrawer({ type: "create" });
  };

  useEffect(() => {
    if (!importRequest) return;
    const text = mcpImportPasteText(importRequest);
    onImportRequestHandled?.();
    if (!text) return;
    let cancelled = false;
    void tauriInvoke("parse_mcp_paste", { text })
      .then((parsed) => {
        if (!cancelled) applyPaste(parsed);
      })
      .catch((err: unknown) => {
        if (!cancelled) toast.error(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [importRequest?.nonce]);

  /**
   * Same verdict handling as the store tab: a refusal is an answer the wizard
   * renders in place, not an error, so only a genuine failure gets a toast.
   */
  const handleInstall = async (submission: McpMarketInstallSubmission): Promise<McpInstallOutcome> => {
    setSaving(true);
    try {
      const outcome = await installFromMarket(submission);
      if (outcome.status === "installed") {
        const failedCount = recordBatch(
          t("mcp.syncBatchSave"),
          outcome.installed.server.id,
          outcome.installed.syncResults,
        );
        if (failedCount > 0) toast.warning(t("mcp.syncPartial", { count: failedCount }));
        else toast.success(t("mcp.added"));
        setDrawer({ type: "closed" });
      }
      return outcome;
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSaving(false);
    }
  };

  /**
   * Record a batch and toast its headline. The detail panel below the list is
   * the actual answer — the toast only says whether to go look at it.
   */
  const recordBatch = (title: string, serverId: string | null, results: McpSyncResult[]) => {
    const failed = failedMcpSyncCount(results);
    const report = summarizeMcpSyncResults(results);
    setBatch(report.consistency.consistent && failed === 0 ? null : { title, serverId, results });
    return failed;
  };

  const handleToggle = async (id: string, toolId: McpToolId, enabled: boolean) => {
    try {
      const result = await toggleTool(id, toolId, enabled);
      if (!result.success && !result.skipped) {
        toast.error(
          t("mcp.syncToolFailed", {
            toolId,
            error: result.error ?? t("common.unknown", { defaultValue: "Unknown" }),
          }),
        );
        setBatch({ title: t("mcp.syncBatchToggle"), serverId: id, results: [result] });
      } else if (result.skipped) {
        toast.info(t("mcp.syncToolSkipped", { toolId }));
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSubmit = async (value: McpServerFormValue) => {
    setSaving(true);
    try {
      let result: McpServerWithSync;
      if (drawer.type === "edit") {
        const { enabled: _enabled, ...patch } = value;
        result = await updateServer(drawer.id, patch);
      } else {
        const entry: Partial<McpServerEntry> = { ...value, timeoutMs: value.timeoutMs ?? undefined };
        result = await createServer(entry);
      }
      const failedCount = recordBatch(t("mcp.syncBatchSave"), result.server.id, result.syncResults);
      if (failedCount > 0) {
        toast.warning(t("mcp.syncPartial", { count: failedCount }));
      } else {
        toast.success(t(drawer.type === "edit" ? "mcp.saved" : "mcp.added"));
      }
      setDrawer({ type: "closed" });
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (drawer.type !== "edit") return;
    try {
      const results = await deleteServer(drawer.id);
      const failedCount = recordBatch(t("mcp.syncBatchDelete"), null, results);
      if (failedCount > 0) {
        toast.warning(t("mcp.syncPartial", { count: failedCount }));
      } else {
        toast.success(t("mcp.deleted"));
      }
      setDrawer({ type: "closed" });
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const handleImport = async () => {
    try {
      const total = await importFromTools();
      toast.success(total > 0 ? t("mcp.importedCount", { count: total }) : t("mcp.importedNone"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSyncAll = async () => {
    try {
      const results = await syncAll(false);
      const failedCount = recordBatch(t("mcp.syncBatchAll"), null, results);
      if (failedCount > 0) {
        toast.warning(t("mcp.syncPartial", { count: failedCount }));
      } else {
        toast.success(t("mcp.syncSuccess"));
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  /** Re-project the whole server; `force` so a rolled-back tool is rewritten. */
  const handleRetryAll = async () => {
    if (!batch?.serverId) return;
    try {
      const results = await syncServer(batch.serverId, true);
      const merged = mergeMcpSyncResults(batch.results, results);
      const failedCount = recordBatch(batch.title, batch.serverId, merged);
      if (failedCount === 0) toast.success(t("mcp.syncSuccess"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  /** Retry exactly one target by re-asserting its enable flag. */
  const handleRetryTool = async (toolId: McpToolId) => {
    if (!batch?.serverId) return;
    try {
      const result = await toggleTool(batch.serverId, toolId, true);
      const merged = mergeMcpSyncResults(batch.results, [result]);
      const failedCount = recordBatch(batch.title, batch.serverId, merged);
      if (failedCount === 0) toast.success(t("mcp.syncSuccess"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const filtersSlot = (
    <>
      {/* Tool filter — shared segmented pill, identical affordance to Skills' agent
          filter. Clicking a tool shows only servers synced into it. */}
      <AgentFilterPill items={toolFilterItems} value={activeToolFilter} onChange={setToolFilter} />

      {/* Count badge — standalone read-only pill (mirrors Skills' countText). */}
      <div className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-border/70 bg-background/50 px-3 text-xs font-medium tabular-nums text-foreground/80 shadow-sm">
        <Boxes className="h-3.5 w-3.5 text-muted-foreground" />
        <span>{filteredServers.length}</span>
        {filteredServers.length !== servers.length ? (
          <span className="text-muted-foreground/70">/ {servers.length}</span>
        ) : null}
      </div>

      {updates.updateCount > 0 ? (
        <div className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-sky-500/30 bg-sky-500/8 px-3 text-xs font-medium tabular-nums text-sky-600 shadow-sm dark:text-sky-400">
          {t("mcp.updatesAvailable", { count: updates.updateCount })}
        </div>
      ) : null}
    </>
  );

  const actionsSlot = (
    <>
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        onClick={() => void handleImport()}
        disabled={importing}
        title={t("mcp.importFromTools")}
        aria-label={t("mcp.importFromTools")}
      >
        <Download className="h-3.5 w-3.5" />
      </Button>
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        onClick={() => void handleSyncAll()}
        disabled={syncing}
        title={t("mcp.syncAll")}
        aria-label={t("mcp.syncAll")}
      >
        <RefreshCw className={syncing ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
      </Button>
      <Button type="button" size="sm" onClick={openCreate}>
        <Plug className="h-3.5 w-3.5" />
        {t("mcp.addServer")}
      </Button>
    </>
  );

  const hasSearch = normalizedQuery.length > 0;
  // Any active narrowing (text search or tool filter) means an empty result is a
  // "no matches" state, not a "you have no servers yet" state.
  const hasActiveFilter = hasSearch || activeToolFilter !== null || healthFilter !== "all";
  const showServers = filteredServers.length > 0;

  const closeEditor = () => {
    if (!saving) {
      setSelectedPresetId(null);
      setDrawer({ type: "closed" });
    }
  };

  useEffect(() => {
    if (drawer.type === "closed") return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeEditor();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawer.type, saving]);

  const editorTitle =
    drawer.type === "edit"
      ? (editing?.name ?? t("mcp.title"))
      : drawer.type === "install"
        ? t("mcp.installWizardTitle")
        : t("mcp.addServer");
  const editorSubtitle = drawer.type === "install" ? t("mcp.installWizardSubtitle") : t("mcp.drawerSubtitle");

  const applyDroppedText = (event: { preventDefault: () => void; dataTransfer: DataTransfer }) => {
    event.preventDefault();
    setDropping(false);
    const text = event.dataTransfer.getData("text/plain") || event.dataTransfer.getData("text/uri-list");
    if (text.trim()) setPasteSeed((prev) => ({ key: prev.key + 1, text }));
  };

  return (
    <div
      className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
      onDragEnter={(event) => {
        event.preventDefault();
        setDropping(true);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        setDropping(true);
      }}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node)) return;
        setDropping(false);
      }}
      onDrop={applyDroppedText}
    >
      <PageToolbar
        title={<h1>{t("mcp.tabFleet")}</h1>}
        search={
          <SearchInput
            containerClassName="w-64"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("mcp.searchPlaceholder")}
            className="h-8 bg-sidebar/50 text-xs focus-visible:bg-background"
            iconClassName="left-2.5"
          />
        }
        filters={filtersSlot}
        actions={actionsSlot}
      />
      {dropping ? (
        <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center bg-background/70 text-sm font-medium text-primary">
          {t("mcp.pasteDropHint")}
        </div>
      ) : null}

      <main className="ss-page-scroll">
        <div className="ss-page-stack">
          {error ? (
            <div className="rounded-lg border border-destructive/20 bg-destructive/5 px-4 py-3 text-xs text-destructive">
              {String(error)}
            </div>
          ) : null}

          <McpImportBar
            key={pasteSeed.key}
            initialText={pasteSeed.text}
            onParsed={(parsed) => applyPaste(parsed)}
            disabled={saving}
          />
          <McpFleetStrip
            servers={servers}
            entryFor={probe.entryFor}
            filter={healthFilter}
            onFilterChange={setHealthFilter}
          />
          {batch && batchReport ? (
            <section className="space-y-2">
              <div className="flex items-center gap-2 px-1">
                <h2 className="text-sm font-semibold text-foreground">{batch.title}</h2>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="ml-auto h-7 px-2 text-[11px] text-muted-foreground"
                  onClick={() => setBatch(null)}
                >
                  {t("mcp.dismissPanel")}
                </Button>
              </div>
              <McpSyncResultsPanel
                report={batchReport}
                retrying={retrySyncing}
                onRetryAll={batch.serverId ? () => void handleRetryAll() : undefined}
                onRetryTool={batch.serverId ? (toolId) => void handleRetryTool(toolId) : undefined}
              />
            </section>
          ) : null}

          <section className="space-y-2">
            <div className="flex items-center gap-2 px-1">
              <Boxes className="h-3.5 w-3.5 text-primary" />
              <h2 className="text-sm font-semibold text-foreground">{t("mcp.installedSection")}</h2>
              <span className="text-xs tabular-nums text-muted-foreground">({filteredServers.length})</span>
              {updates.uncheckedCount > 0 ? (
                <span className="text-[11px] text-muted-foreground/80">
                  {t("mcp.updatesUnchecked", { count: updates.uncheckedCount })}
                </span>
              ) : null}
            </div>

            {isLoading ? (
              <div className="flex items-center justify-center py-16">
                <LoadingLogo size="md" label={t("mcp.loading")} />
              </div>
            ) : showServers ? (
              <div ref={containerRef} className="ss-cards-grid" style={gridStyle}>
                {filteredServers.map((server) => {
                  const info = updates.byServerId.get(server.id);
                  return (
                    <div key={server.id} className="h-full">
                      <McpFleetCard
                        server={server}
                        agentTargets={selectMcpAgentTargets(profiles)}
                        updateVersion={info?.hasUpdate ? info.latestVersion : null}
                        probe={probe.entryFor(server.id)}
                        onOpen={() => setDrawer({ type: "edit", id: server.id })}
                        onToggleTool={(toolId, enabled) => void handleToggle(server.id, toolId, enabled)}
                        onProbe={() => void probe.probe(server.id)}
                      />
                    </div>
                  );
                })}
              </div>
            ) : (
              <EmptyState
                icon={<Search className="h-6 w-6" />}
                title={hasActiveFilter ? t("mcp.noMatches") : t("mcp.emptyTitle")}
                description={hasActiveFilter ? t("mcp.emptySearchDescription") : t("mcp.emptyDescription")}
                action={
                  hasActiveFilter ? null : (
                    <div className="flex flex-wrap justify-center gap-2">
                      {onOpenMarket ? (
                        <Button variant="outline" onClick={onOpenMarket}>
                          {t("mcp.openMarket")}
                        </Button>
                      ) : null}
                      <Button variant="outline" onClick={() => void handleImport()}>
                        <Download className="h-4 w-4" />
                        {t("mcp.importFromTools")}
                      </Button>
                      <Button onClick={openCreate}>
                        <Plug className="h-4 w-4" />
                        {t("mcp.addFirstServer")}
                      </Button>
                    </div>
                  )
                }
                size="lg"
              />
            )}
          </section>
        </div>
      </main>

      <ModalShell
        open={drawer.type !== "closed"}
        onClose={closeEditor}
        ariaLabel={editorTitle}
        dismissable={!saving}
        panelClassName="max-w-[760px]"
        surfaceClassName="flex max-h-[min(780px,calc(100vh-2rem))] flex-col overflow-hidden"
        contentClassName="flex min-h-0 flex-col"
      >
        <ModalHeader
          icon={
            drawer.type === "install" ? (
              <PackageSearch className="h-4 w-4 text-primary" />
            ) : (
              <Boxes className="h-4 w-4 text-primary" />
            )
          }
          title={editorTitle}
          onClose={closeEditor}
          closeDisabled={saving}
          className="px-6 pt-5 pb-4"
        />
        {drawer.type === "install" ? <p className="shrink-0 px-6 pb-3 text-caption">{editorSubtitle}</p> : null}
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
          {drawer.type === "install" ? (
            <McpInstallWizard
              key={drawer.catalogId}
              serverId={drawer.catalogId}
              submitting={saving}
              onSubmit={handleInstall}
              onCancel={() => {
                setSelectedPresetId(null);
                setDrawer({ type: "create" });
              }}
              noteForTool={noteForTool}
              defaultEnabled={mcpEnabledMapFromProfiles(profiles)}
              targets={agentTargets}
            />
          ) : drawer.type === "create" ? (
            <div className="space-y-4">
              <McpRecommendedPresets
                presets={presets}
                installedNames={installedNames}
                selectedPresetId={selectedPresetId}
                onPick={pickPreset}
                onReset={() => {
                  setSelectedPresetId(null);
                  setCreateSeed((prev) => ({
                    key: prev.key + 1,
                    defaults: { enabled: mcpEnabledMapFromProfiles(profiles) },
                  }));
                }}
              />
              <McpServerForm
                key={createSeed.key}
                defaults={createSeed.defaults}
                onSubmit={handleSubmit}
                submitting={saving}
                noteForTool={noteForTool}
                targets={agentTargets}
              />
            </div>
          ) : drawer.type === "edit" && editing ? (
            <div className="space-y-4">
              <McpProbePanel entry={probe.entryFor(editing.id)} onProbe={() => void probe.probe(editing.id)} />
              <McpServerForm
                key={editing.id}
                initial={editing}
                onSubmit={handleSubmit}
                onDelete={handleDelete}
                submitting={saving}
                noteForTool={noteForTool}
                targets={agentTargets}
              />
            </div>
          ) : drawer.type === "edit" ? (
            <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
              {t("mcp.notFound")}
            </div>
          ) : null}
        </div>
      </ModalShell>
    </div>
  );
}
