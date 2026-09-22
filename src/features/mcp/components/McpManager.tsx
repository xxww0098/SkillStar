import { Boxes, PackageSearch, Plug, RefreshCw, Search, Wrench } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
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
import { useCardGridColumns } from "../hooks/useCardGrid";
import { useMcpCatalogUpdates } from "../hooks/useMcpCatalogUpdates";
import { useMcpFleetProbe, useMcpProbe } from "../hooks/useMcpProbe";
import { type McpMarketInstallSubmission, useMcpServers } from "../hooks/useMcpServers";
import { useMcpPresets } from "../hooks/useMcpPresets";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import { mcpEnabledMapFromProfiles, resolveMcpToolFilter, selectMcpAgentTargets } from "../lib/agentTargets";
import { mcpDraftToFormValue, mcpServerCommandLine } from "../lib/pasteDraft";
import { failedMcpSyncCount, mergeMcpSyncResults, summarizeMcpSyncResults } from "../lib/syncResults";
import { McpAddDialog, type McpAddMode } from "./McpAddDialog";
import { McpFleetCard } from "./McpFleetCard";
import { McpInstallWizard } from "./McpInstallWizard";
import { McpProbePanel } from "./McpProbePanel";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";
import { McpSyncResultsPanel } from "./McpSyncResultsPanel";

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
  | { type: "edit"; id: string }
  /** Catalog hit from a preset, paste or deep link — same wizard as a store install. */
  | { type: "install"; catalogId: string }
  /** The one "add a server" surface; `McpAddDialog` owns what it looks like. */
  | { type: "add" };

/** The last sync batch, kept so its per-target detail stays inspectable. */
interface SyncBatch {
  title: string;
  serverId: string | null;
  results: McpSyncResult[];
}

interface McpManagerProps {
  /** View switch rendered as the toolbar title (Config | Store). */
  title?: ReactNode;
  /** Switch the config page to the store view. */
  onOpenStore?: () => void;
  /** Open the agent-config inspector. */
  onOpenTools?: () => void;
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

/**
 * MCP config page — the installed servers, and nothing else.
 *
 * The page used to carry a health strip, an update badge row, a permanent
 * paste bar, a whole-page drop overlay and a per-card probe button on top of
 * the list. None of that is "which servers do I have, and are they wired into
 * the Agent I want", which is the only question this surface answers. Health
 * survives as the status dot on each card plus the probe panel in its editor;
 * adding a server is one button, one modal, four sources.
 *
 * The list stays mounted while the store view is showing so the one-shot
 * background probe survives the hop.
 */
export function McpManager({
  title,
  onOpenStore,
  onOpenTools,
  importRequest,
  onImportRequestHandled,
}: McpManagerProps) {
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
  const [addMode, setAddMode] = useState<McpAddMode>("recommended");
  const [saving, setSaving] = useState(false);
  const [batch, setBatch] = useState<SyncBatch | null>(null);
  // Seed values + a nonce key so the manual form re-mounts with fresh defaults
  // (the form only reads `defaults` on mount).
  const [createSeed, setCreateSeed] = useState<{ key: number; defaults?: Partial<McpServerFormValue> }>({ key: 0 });
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  // Active tool filter: only show servers synced into this tool (null = all).
  const [toolFilter, setToolFilter] = useState<string | null>(null);
  const [pasteSeed, setPasteSeed] = useState({ key: 0, text: "" });
  const containerRef = useRef<HTMLDivElement>(null);
  const agentTargets = useMemo(() => selectMcpAgentTargets(profiles), [profiles]);
  const activeToolFilter = resolveMcpToolFilter(toolFilter, agentTargets);

  const filteredServers = useMemo(
    () =>
      servers.filter((server) => {
        if (activeToolFilter && !server.enabled[activeToolFilter]) return false;
        return matchesQuery(normalizedQuery, [
          server.name,
          server.description,
          server.homepage,
          server.transport,
          server.tags,
          mcpServerCommandLine(server),
        ]);
      }),
    [servers, normalizedQuery, activeToolFilter],
  );

  const { gridStyle } = useCardGridColumns(containerRef, filteredServers.length);

  // The toolbar uses the same Settings-backed target set as every MCP card.
  // The filter value is the MCP tool id while the glyph and label come from
  // the Agent profile it maps to (`claude-code` → `claude`).
  const toolFilterItems = useMemo(
    () => agentTargets.map(({ toolId, profile }) => ({ id: toolId, profile })),
    [agentTargets],
  );

  const installedNames = useMemo(() => new Set(servers.map((server) => server.name.trim().toLowerCase())), [servers]);

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;
  const batchReport = useMemo(() => (batch ? summarizeMcpSyncResults(batch.results) : null), [batch]);

  const seedBase = () => ({ enabled: mcpEnabledMapFromProfiles(profiles) });

  const openAdd = () => {
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: seedBase() }));
    setPasteSeed((prev) => ({ key: prev.key + 1, text: "" }));
    setAddMode("recommended");
    setDrawer({ type: "add" });
  };

  const pickPreset = (preset: McpPreset) => {
    if (preset.catalogId) {
      setDrawer({ type: "install", catalogId: preset.catalogId });
      return;
    }
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: presetToDefaults(preset, seedBase().enabled) }));
    setAddMode("manual");
  };

  const applyPaste = (parsed: McpPasteParse) => {
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
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: mcpDraftToFormValue(drafts[0], seedBase().enabled) }));
    setAddMode("manual");
  };

  useEffect(() => {
    if (!importRequest) return;
    const text = mcpImportPasteText(importRequest);
    onImportRequestHandled?.();
    if (!text) return;
    setPasteSeed((prev) => ({ key: prev.key + 1, text }));
    setAddMode("paste");
    setDrawer({ type: "add" });
  }, [importRequest?.nonce]);

  /**
   * Same verdict handling as the store view: a refusal is an answer the wizard
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
      if (total > 0) setDrawer({ type: "closed" });
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

  const closeEditor = () => {
    if (!saving) setDrawer({ type: "closed" });
  };

  const editorTitle =
    drawer.type === "edit"
      ? (editing?.name ?? t("mcp.title"))
      : drawer.type === "install"
        ? t("mcp.installWizardTitle")
        : t("mcp.addServer");

  const hasActiveFilter = normalizedQuery.length > 0 || activeToolFilter !== null;

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <PageToolbar
        title={title ?? <h1>{t("mcp.title")}</h1>}
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
        filters={
          <AgentFilterPill
            items={toolFilterItems}
            value={activeToolFilter}
            onChange={setToolFilter}
            maxVisible={toolFilterItems.length}
          />
        }
        actions={
          <>
            {onOpenTools ? (
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                onClick={onOpenTools}
                title={t("mcp.toolStatusTitle")}
                aria-label={t("mcp.toolStatusTitle")}
              >
                <Wrench className="h-3.5 w-3.5" />
              </Button>
            ) : null}
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
            <Button type="button" size="sm" onClick={openAdd}>
              <Plug className="h-3.5 w-3.5" />
              {t("mcp.addServer")}
            </Button>
          </>
        }
      />

      <main className="ss-page-scroll">
        <div className="ss-page-stack">
          {error ? (
            <div className="rounded-lg border border-destructive/20 bg-destructive/5 px-4 py-3 text-xs text-destructive">
              {String(error)}
            </div>
          ) : null}

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

          <section>
            {isLoading ? (
              <div className="flex items-center justify-center py-16">
                <LoadingLogo size="md" label={t("mcp.loading")} />
              </div>
            ) : filteredServers.length > 0 ? (
              <div ref={containerRef} className="ss-cards-grid" style={gridStyle}>
                {filteredServers.map((server) => {
                  const info = updates.byServerId.get(server.id);
                  return (
                    <div key={server.id} className="h-full">
                      <McpFleetCard
                        server={server}
                        agentTargets={agentTargets}
                        updateVersion={info?.hasUpdate ? info.latestVersion : null}
                        probe={probe.entryFor(server.id)}
                        onOpen={() => setDrawer({ type: "edit", id: server.id })}
                        onToggleTool={(toolId, enabled) => void handleToggle(server.id, toolId, enabled)}
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
                      {onOpenStore ? (
                        <Button variant="outline" onClick={onOpenStore}>
                          {t("mcp.browseStore")}
                        </Button>
                      ) : null}
                      <Button onClick={openAdd}>
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
        panelClassName="max-w-[680px]"
        surfaceClassName="flex max-h-[min(700px,calc(100vh-2.5rem))] flex-col overflow-hidden"
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
          className="px-5 pt-4 pb-3"
        />
        {drawer.type === "install" ? (
          <p className="shrink-0 px-5 pb-2 text-caption">{t("mcp.installWizardSubtitle")}</p>
        ) : drawer.type === "add" ? (
          <p className="shrink-0 px-5 pb-2 text-caption">{t("mcp.drawerSubtitle")}</p>
        ) : null}
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">
          {drawer.type === "install" ? (
            <McpInstallWizard
              key={drawer.catalogId}
              serverId={drawer.catalogId}
              submitting={saving}
              onSubmit={handleInstall}
              onCancel={() => {
                setAddMode("recommended");
                setDrawer({ type: "add" });
              }}
              noteForTool={noteForTool}
              defaultEnabled={mcpEnabledMapFromProfiles(profiles)}
              targets={agentTargets}
            />
          ) : drawer.type === "add" ? (
            <McpAddDialog
              mode={addMode}
              onModeChange={setAddMode}
              presets={presets}
              installedNames={installedNames}
              formKey={createSeed.key}
              defaults={createSeed.defaults}
              pasteSeed={pasteSeed}
              submitting={saving}
              importing={importing}
              noteForTool={noteForTool}
              targets={agentTargets}
              onPickPreset={pickPreset}
              onSubmit={handleSubmit}
              onImport={() => void handleImport()}
              onParsed={applyPaste}
            />
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
