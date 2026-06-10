import { Boxes, Check, Download, ExternalLink, PackageSearch, Plug, RefreshCw, Search, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PageToolbar } from "../../../components/layout/PageToolbar";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SearchInput } from "../../../components/ui/SearchInput";
import { cn } from "../../../lib/utils";
import type { McpPreset, McpServerEntry } from "../../../types";
import { useMcpPresets } from "../hooks/useMcpPresets";
import { useMcpServers } from "../hooks/useMcpServers";
import { McpServerCard } from "./McpServerCard";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";
import { ProviderDrawer } from "../../models/components/hub/ProviderDrawer";

type DrawerMode =
  | { type: "closed" }
  | { type: "create" }
  | { type: "create-preset"; preset: McpPreset }
  | { type: "edit"; id: string };

interface McpManagerProps {
  /** Navigate to the unified Marketplace MCP tab. */
  onOpenMarket?: () => void;
}

/** Seed the create form from a recommended preset (required-env keys left blank). */
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
    enabled: {},
  };
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

  const filteredPresets = useMemo(
    () =>
      presets.filter((preset) =>
        matchesQuery(normalizedQuery, [
          preset.name,
          preset.description,
          preset.homepage,
          preset.transport,
          preset.tags,
          preset.command,
          preset.args,
          preset.url,
        ]),
      ),
    [presets, normalizedQuery],
  );

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;

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
      {toolStatuses.map((status) => (
        <span
          key={status.toolId}
          title={status.configPath}
          className={cn(
            "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-[11px] transition-colors",
            status.installed
              ? "border-border/70 bg-background/40 text-muted-foreground"
              : "border-border/40 bg-muted/20 text-muted-foreground/60",
          )}
        >
          <span
            className={cn("h-1.5 w-1.5 rounded-full", status.installed ? "bg-success" : "bg-muted-foreground/35")}
          />
          <span>{status.label}</span>
          <span className="text-muted-foreground/60">{status.serverCount}</span>
        </span>
      ))}
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
      <Button type="button" size="sm" onClick={() => setDrawer({ type: "create" })}>
        <Plug className="h-3.5 w-3.5" />
        {t("mcp.addServer")}
      </Button>
    </>
  );

  const hasSearch = normalizedQuery.length > 0;
  const showPresets = filteredPresets.length > 0;
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

          {showPresets ? (
            <section className="space-y-3">
              <div className="flex items-center gap-2 px-1">
                <Sparkles className="h-3.5 w-3.5 text-primary" />
                <h2 className="text-sm font-semibold text-foreground">{t("mcp.recommendedSection")}</h2>
                <span className="text-xs text-muted-foreground">({filteredPresets.length})</span>
              </div>
              <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
                {filteredPresets.map((preset) => {
                  const added = servers.some((server) => server.name === preset.name);
                  return (
                    <div
                      key={preset.id}
                      className="group flex min-h-[144px] flex-col gap-2 rounded-xl border border-dashed border-border/60 bg-card/45 p-4 transition hover:border-primary/30 hover:bg-card/70"
                    >
                      <div className="flex min-w-0 items-center gap-2">
                        <Boxes className="h-4 w-4 shrink-0 text-primary" />
                        <span className="truncate text-sm font-semibold text-foreground">{preset.name}</span>
                        <span className="ml-auto shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                          {preset.transport}
                        </span>
                      </div>
                      {preset.description ? (
                        <p className="line-clamp-2 text-[11px] leading-5 text-muted-foreground">{preset.description}</p>
                      ) : null}
                      <div className="mt-auto flex items-center justify-between gap-2 pt-1">
                        {preset.homepage ? (
                          <ExternalAnchor
                            href={preset.homepage}
                            className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
                          >
                            <ExternalLink className="h-3 w-3" />
                            {t("mcp.homepage")}
                          </ExternalAnchor>
                        ) : (
                          <span />
                        )}
                        <Button
                          size="sm"
                          variant={added ? "outline" : "default"}
                          disabled={added}
                          onClick={() => setDrawer({ type: "create-preset", preset })}
                        >
                          {added ? (
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
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
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
                      <Button onClick={() => setDrawer({ type: "create" })}>
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

      <ProviderDrawer
        open={drawer.type !== "closed"}
        onOpenChange={(open) => {
          if (!open) setDrawer({ type: "closed" });
        }}
        title={
          <span className="flex items-center gap-2 text-foreground">
            <Boxes className="h-4 w-4 text-primary" />
            {drawer.type === "edit"
              ? (editing?.name ?? t("mcp.title"))
              : drawer.type === "create-preset"
                ? drawer.preset.name
                : t("mcp.addServer")}
          </span>
        }
        subtitle={drawer.type === "create-preset" ? t("mcp.drawerPresetSubtitle") : t("mcp.drawerSubtitle")}
      >
        {drawer.type === "create" ? (
          <McpServerForm onSubmit={handleSubmit} submitting={saving} />
        ) : drawer.type === "create-preset" ? (
          <McpServerForm
            key={drawer.preset.id}
            defaults={presetToDefaults(drawer.preset)}
            submitLabel={t("common.add")}
            onSubmit={handleSubmit}
            submitting={saving}
          />
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
      </ProviderDrawer>
    </div>
  );
}
