import { Boxes, Download, PackageSearch, Plug, RefreshCw, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PageToolbar } from "../../../components/layout/PageToolbar";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { cn } from "../../../lib/utils";
import type { McpPreset, McpServerEntry, McpToolId } from "../../../types";
import { useMcpServers } from "../hooks/useMcpServers";
import { useMcpPresets } from "../hooks/useMcpPresets";
import { McpServerCard, MCP_TOOL_ICON } from "./McpServerCard";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";
import { DrawerShell } from "../../models";

/** Map a recommended preset into create-form seed values. */
function presetToDefaults(preset: McpPreset): Partial<McpServerFormValue> {
  return {
    name: preset.name,
    transport: preset.transport,
    command: preset.command,
    args: preset.args,
    env: preset.env,
    url: preset.url,
    headers: preset.headers,
    description: preset.description,
    homepage: preset.homepage,
  };
}

type DrawerMode = { type: "closed" } | { type: "create" } | { type: "edit"; id: string };

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
  const {
    servers,
    toolStatuses,
    isLoading,
    error,
    createServer,
    updateServer,
    deleteServer,
    toggleTool,
    syncAll,
    importFromTool,
    syncing,
    importing,
  } = useMcpServers();
  const { presets } = useMcpPresets();
  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [saving, setSaving] = useState(false);
  // Seed values + a nonce key so picking a preset re-mounts the create form
  // (the form only reads `defaults` on mount).
  const [createSeed, setCreateSeed] = useState<{ key: number; defaults?: Partial<McpServerFormValue> }>({ key: 0 });
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();

  const filteredServers = useMemo(
    () =>
      servers.filter((server) =>
        matchesQuery(normalizedQuery, [
          server.name,
          server.description,
          server.homepage,
          server.transport,
          server.tags,
          serverCommand(server),
        ]),
      ),
    [servers, normalizedQuery],
  );

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;

  const openCreate = () => {
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: undefined }));
    setDrawer({ type: "create" });
  };

  const pickPreset = (preset: McpPreset) => {
    setCreateSeed((prev) => ({ key: prev.key + 1, defaults: presetToDefaults(preset) }));
  };

  const handleToggle = async (id: string, toolId: string, enabled: boolean) => {
    try {
      const result = await toggleTool(id, toolId, enabled);
      if (!result.success && !result.skipped) {
        toast.error(
          t("mcp.syncToolFailed", {
            toolId,
            error: result.error ?? t("common.unknown", { defaultValue: "Unknown" }),
          }),
        );
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
      if (drawer.type === "edit") {
        const { enabled: _enabled, ...patch } = value;
        await updateServer(drawer.id, patch);
        toast.success(t("mcp.saved"));
      } else {
        const entry: Partial<McpServerEntry> = { ...value };
        await createServer(entry);
        toast.success(t("mcp.added"));
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
      await deleteServer(drawer.id);
      toast.success(t("mcp.deleted"));
      setDrawer({ type: "closed" });
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const handleImport = async () => {
    let total = 0;
    for (const status of toolStatuses) {
      if (!status.installed || status.serverCount === 0) continue;
      try {
        total += await importFromTool(status.toolId);
      } catch {
        // best-effort; skip tools that can't be read
      }
    }
    toast.success(total > 0 ? t("mcp.importedCount", { count: total }) : t("mcp.importedNone"));
  };

  const handleSyncAll = async () => {
    try {
      const results = await syncAll(false);
      const failed = results.filter((r) => !r.success && !r.skipped);
      if (failed.length > 0) {
        toast.warning(t("mcp.syncPartial", { count: failed.length }));
      } else {
        toast.success(t("mcp.syncSuccess"));
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const filtersSlot = (
    <>
      <div className="flex h-8 items-center justify-center gap-1.5 rounded-lg border border-border/70 bg-background/50 px-3 text-xs font-medium tabular-nums text-foreground/80 shadow-sm">
        <Boxes className="h-3.5 w-3.5 text-muted-foreground" />
        <span>{filteredServers.length}</span>
        {filteredServers.length !== servers.length ? (
          <span className="text-muted-foreground/70">/ {servers.length}</span>
        ) : null}
      </div>
      {toolStatuses.map((status) => {
        const toolId = status.toolId as McpToolId;
        const meta = MCP_TOOL_ICON[toolId];
        return (
          <span
            key={status.toolId}
            title={status.configPath}
            className={cn(
              "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border px-2 text-[11px] transition-colors",
              status.installed
                ? "border-border/70 bg-background/40 text-muted-foreground"
                : "border-border/40 bg-muted/20 text-muted-foreground/60",
            )}
          >
            <span
              className={cn("h-1.5 w-1.5 rounded-full", status.installed ? "bg-success" : "bg-muted-foreground/35")}
            />
            {meta ? (
              <AgentIcon
                profile={{ id: meta.profileId, icon: meta.icon, display_name: meta.label }}
                className="h-3.5 w-3.5"
              />
            ) : (
              <span>{status.label}</span>
            )}
            <span className="text-muted-foreground/60">{status.serverCount}</span>
          </span>
        );
      })}
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

          <section className="space-y-3">
            <div className="flex items-center gap-2 px-1">
              <Boxes className="h-3.5 w-3.5 text-primary" />
              <h2 className="text-sm font-semibold text-foreground">{t("mcp.installedSection")}</h2>
              <span className="text-xs text-muted-foreground">({filteredServers.length})</span>
            </div>

            {isLoading ? (
              <div className="flex items-center justify-center py-16">
                <LoadingLogo size="md" label={t("mcp.loading")} />
              </div>
            ) : showServers ? (
              <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
                {filteredServers.map((server) => (
                  <McpServerCard
                    key={server.id}
                    server={server}
                    toolStatuses={toolStatuses}
                    onOpen={() => setDrawer({ type: "edit", id: server.id })}
                    onToggleTool={(toolId, enabled) => void handleToggle(server.id, toolId, enabled)}
                  />
                ))}
              </div>
            ) : (
              <EmptyState
                icon={<Search className="h-6 w-6" />}
                title={hasSearch ? t("mcp.noMatches") : t("mcp.emptyTitle")}
                description={hasSearch ? t("mcp.emptySearchDescription") : t("mcp.emptyDescription")}
                action={
                  hasSearch ? null : (
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
            <Boxes className="h-4 w-4 text-primary" />
            {drawer.type === "edit" ? (editing?.name ?? t("mcp.title")) : t("mcp.addServer")}
          </span>
        }
        subtitle={t("mcp.drawerSubtitle")}
      >
        {drawer.type === "create" ? (
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
            />
          </div>
        ) : drawer.type === "edit" && editing ? (
          <McpServerForm
            key={editing.id}
            initial={editing}
            onSubmit={handleSubmit}
            onDelete={handleDelete}
            submitting={saving}
          />
        ) : drawer.type === "edit" ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">{t("mcp.notFound")}</div>
        ) : null}
      </DrawerShell>
    </div>
  );
}
