import { Boxes, Download, PackageSearch, Plug, RefreshCw, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageToolbar } from "../../../components/layout/PageToolbar";
import { DrawerShell } from "../../../components/shared/DrawerShell";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { AgentFilterPill } from "../../../components/ui/AgentFilterPill";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { cn } from "../../../lib/utils";
import { toast } from "../../../lib/toast";
import type {
  McpInstallOutcome,
  McpPreset,
  McpServerEntry,
  McpServerWithSync,
  McpSyncResult,
  McpToolId,
} from "../../../types";
import { useMcpCatalogUpdates } from "../hooks/useMcpCatalogUpdates";
import { useMcpProbe } from "../hooks/useMcpProbe";
import { type McpMarketInstallSubmission, useMcpServers } from "../hooks/useMcpServers";
import { useMcpPresets } from "../hooks/useMcpPresets";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import {
  mcpEnabledMapFromProfiles,
  resolveMcpToolFilter,
  selectMcpAgentTargets,
  selectMcpAgentTargetsForServer,
} from "../lib/agentTargets";
import { failedMcpSyncCount, mergeMcpSyncResults, summarizeMcpSyncResults } from "../lib/syncResults";
import { McpInstallWizard } from "./McpInstallWizard";
import { McpProbePanel } from "./McpProbePanel";
import { McpServerCard } from "./McpServerCard";
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
}

function matchesQuery(query: string, values: Array<string | string[] | undefined | null>): boolean {
  if (!query) return true;
  return values.some((value) => {
    if (!value) return false;
    const text = Array.isArray(value) ? value.join(" ") : value;
    return text.toLowerCase().includes(query);
  });
}

function serverCommand(server: McpServerEntry): string {
  if (server.transport === "http" || server.transport === "sse") return server.url ?? "";
  return [server.command, ...(server.args ?? [])].filter(Boolean).join(" ");
}

export function McpManager({ onOpenMarket }: McpManagerProps) {
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
  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [saving, setSaving] = useState(false);
  const [batch, setBatch] = useState<SyncBatch | null>(null);
  // Seed values + a nonce key so picking a preset re-mounts the create form
  // (the form only reads `defaults` on mount).
  const [createSeed, setCreateSeed] = useState<{ key: number; defaults?: Partial<McpServerFormValue> }>({ key: 0 });
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  // Active tool filter: only show servers synced into this tool (null = all).
  const [toolFilter, setToolFilter] = useState<string | null>(null);
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
          serverCommand(server),
        ]);
      }),
    [servers, normalizedQuery, activeToolFilter],
  );

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

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;
  const batchReport = useMemo(() => (batch ? summarizeMcpSyncResults(batch.results) : null), [batch]);

  const openCreate = () => {
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
    if (preset.catalogId) {
      setDrawer({ type: "install", catalogId: preset.catalogId });
      return;
    }
    setCreateSeed((prev) => ({
      key: prev.key + 1,
      defaults: presetToDefaults(preset, mcpEnabledMapFromProfiles(profiles)),
    }));
  };

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
      {onOpenMarket ? (
        <Button type="button" variant="outline" size="sm" onClick={onOpenMarket}>
          <PackageSearch className="h-3.5 w-3.5" />
          {t("mcp.openMarket")}
        </Button>
      ) : null}
      <Button type="button" variant="outline" size="sm" onClick={() => void handleImport()} disabled={importing}>
        <Download className="h-3.5 w-3.5" />
        {t("mcp.importFromTools")}
      </Button>
      <Button type="button" variant="outline" size="sm" onClick={() => void handleSyncAll()} disabled={syncing}>
        <RefreshCw className={syncing ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
        {t("mcp.syncAll")}
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
  const hasActiveFilter = hasSearch || activeToolFilter !== null;
  const showServers = filteredServers.length > 0;

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <PageToolbar
        title={<h1>{t("mcp.title")}</h1>}
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

          <section className="space-y-3">
            <div className="flex items-center gap-2 px-1">
              <Boxes className="h-3.5 w-3.5 text-primary" />
              <h2 className="text-sm font-semibold text-foreground">{t("mcp.installedSection")}</h2>
              <span className="text-xs text-muted-foreground">({filteredServers.length})</span>
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
              <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
                {filteredServers.map((server) => {
                  const info = updates.byServerId.get(server.id);
                  return (
                    <McpServerCard
                      key={server.id}
                      server={server}
                      agentTargets={selectMcpAgentTargetsForServer(profiles, server.enabled)}
                      updateVersion={info?.hasUpdate ? info.latestVersion : null}
                      onOpen={() => setDrawer({ type: "edit", id: server.id })}
                      onToggleTool={(toolId, enabled) => void handleToggle(server.id, toolId, enabled)}
                    />
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

      <DrawerShell
        open={drawer.type !== "closed"}
        onOpenChange={(open) => {
          if (!open) setDrawer({ type: "closed" });
        }}
        title={
          <span className="flex items-center gap-2 text-foreground">
            {drawer.type === "install" ? (
              <PackageSearch className="h-4 w-4 text-primary" />
            ) : (
              <Boxes className="h-4 w-4 text-primary" />
            )}
            {drawer.type === "edit"
              ? (editing?.name ?? t("mcp.title"))
              : drawer.type === "install"
                ? t("mcp.installWizardTitle")
                : t("mcp.addServer")}
          </span>
        }
        subtitle={drawer.type === "install" ? t("mcp.installWizardSubtitle") : t("mcp.drawerSubtitle")}
      >
        {drawer.type === "install" ? (
          <McpInstallWizard
            key={drawer.catalogId}
            serverId={drawer.catalogId}
            submitting={saving}
            onSubmit={handleInstall}
            // Back to the chips rather than closed: a mis-clicked chip should
            // not cost the drawer.
            onCancel={() => setDrawer({ type: "create" })}
            noteForTool={noteForTool}
            defaultEnabled={mcpEnabledMapFromProfiles(profiles)}
          />
        ) : drawer.type === "create" ? (
          <div className="space-y-4">
            {presets.length > 0 ? (
              <div className="rounded-lg border border-border/60 bg-background/40 p-3">
                <p className="mb-2 flex items-center gap-1.5 text-xs font-medium text-foreground">
                  <PackageSearch className="h-3.5 w-3.5 text-primary" />
                  {t("mcp.presetsTitle")}
                </p>
                <div className="flex flex-wrap gap-1.5">
                  {presets.map((preset) => (
                    <button
                      key={preset.id}
                      type="button"
                      title={preset.description}
                      onClick={() => pickPreset(preset)}
                      className={cn(
                        "rounded-md border px-2 py-1 text-[11px] transition-colors",
                        createSeed.defaults?.name === preset.name
                          ? "border-primary/60 bg-primary/10 text-primary"
                          : "border-border/70 bg-background/50 text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                      )}
                    >
                      {preset.name}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
            <McpServerForm
              key={createSeed.key}
              defaults={createSeed.defaults}
              onSubmit={handleSubmit}
              submitting={saving}
              noteForTool={noteForTool}
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
            />
          </div>
        ) : drawer.type === "edit" ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">{t("mcp.notFound")}</div>
        ) : null}
      </DrawerShell>
    </div>
  );
}
