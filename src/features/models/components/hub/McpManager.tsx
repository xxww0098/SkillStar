import { Boxes, Check, Download, ExternalLink, Plug, RefreshCw, Sparkles } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import type { McpPreset, McpServerEntry } from "../../../../types";
import { useMcpPresets } from "../../hooks/useMcpPresets";
import { tauriInvoke } from "../../../../lib/ipc";
import { McpMarketBrowser } from "../../../mcp/components/McpMarketBrowser";
import { useMcpServers } from "../../hooks/useMcpServers";
import { McpServerCard } from "./McpServerCard";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";
import { ProviderDrawer } from "./ProviderDrawer";

type DrawerMode =
  | { type: "closed" }
  | { type: "create" }
  | { type: "create-preset"; preset: McpPreset }
  | { type: "install-market"; sourceName: string; defaults: Partial<McpServerFormValue> }
  | { type: "edit"; id: string };

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

/** Seed the create form from a marketplace install draft (secrets left blank). */
function draftToDefaults(draft: McpServerEntry): Partial<McpServerFormValue> {
  return {
    name: draft.name,
    transport: draft.transport,
    command: draft.command,
    args: draft.args,
    env: draft.env,
    url: draft.url,
    headers: draft.headers,
    description: draft.description,
    homepage: draft.homepage,
    enabled: {},
  };
}

export function McpManager() {
  const {
    servers,
    toolStatuses,
    isLoading,
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
  const [view, setView] = useState<"installed" | "market">("installed");

  const handleInstallFromMarket = async (id: string) => {
    try {
      const draft = await tauriInvoke("mcp_market_entry_to_draft", { id });
      setDrawer({ type: "install-market", sourceName: draft.name, defaults: draftToDefaults(draft) });
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  const editing = drawer.type === "edit" ? (servers.find((s) => s.id === drawer.id) ?? null) : null;

  const handleToggle = async (id: string, toolId: string, enabled: boolean) => {
    try {
      const result = await toggleTool(id, toolId, enabled);
      if (!result.success && !result.skipped) {
        toast.error(`同步到 ${toolId} 失败：${result.error ?? "未知错误"}`);
      } else if (result.skipped) {
        toast.info(`${toolId} 未安装，已记录开关但未写入配置`);
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
        toast.success("已保存 MCP 服务器");
      } else {
        const entry: Partial<McpServerEntry> = { ...value };
        await createServer(entry);
        toast.success("已添加 MCP 服务器");
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
      toast.success("已删除 MCP 服务器");
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
    toast.success(total > 0 ? `已从已安装工具导入 ${total} 个 MCP 服务器` : "没有发现可导入的新服务器");
  };

  const handleSyncAll = async () => {
    try {
      const results = await syncAll(false);
      const failed = results.filter((r) => !r.success && !r.skipped);
      if (failed.length > 0) {
        toast.warning(`同步完成，但 ${failed.length} 项失败`);
      } else {
        toast.success("已将所有 MCP 服务器同步到对应工具");
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <section>
      <div className="mb-4 inline-flex rounded-lg border border-border/50 bg-card/40 p-0.5 text-sm">
        <button
          type="button"
          onClick={() => setView("installed")}
          className={
            view === "installed"
              ? "rounded-md bg-primary px-3 py-1 font-medium text-primary-foreground"
              : "rounded-md px-3 py-1 text-muted-foreground hover:text-foreground"
          }
        >
          已安装 ({servers.length})
        </button>
        <button
          type="button"
          onClick={() => setView("market")}
          className={
            view === "market"
              ? "rounded-md bg-primary px-3 py-1 font-medium text-primary-foreground"
              : "rounded-md px-3 py-1 text-muted-foreground hover:text-foreground"
          }
        >
          市场
        </button>
      </div>

      {view === "market" ? (
        <McpMarketBrowser
          installedNames={new Set(servers.map((s) => s.name))}
          onInstall={(id) => void handleInstallFromMarket(id)}
        />
      ) : (
        <>
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
              <Boxes className="h-3.5 w-3.5" />
              MCP 服务器 <span className="text-muted-foreground/70">({servers.length})</span>
            </h2>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void handleImport()}
                disabled={importing}
                className="gap-1.5"
              >
                <Download className="h-3.5 w-3.5" />
                从工具导入
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void handleSyncAll()}
                disabled={syncing}
                className="gap-1.5"
              >
                <RefreshCw className={syncing ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
                全部同步
              </Button>
              <Button size="sm" onClick={() => setDrawer({ type: "create" })} className="gap-1.5">
                <Plug className="h-3.5 w-3.5" />
                新增 MCP
              </Button>
            </div>
          </div>

          {/* Per-tool status strip */}
          {toolStatuses.length > 0 ? (
            <div className="mb-4 flex flex-wrap gap-2">
              {toolStatuses.map((s) => (
                <span
                  key={s.toolId}
                  title={s.configPath}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-border/50 bg-card/50 px-2.5 py-1 text-[11px] text-muted-foreground"
                >
                  <span
                    className={
                      s.installed
                        ? "h-1.5 w-1.5 rounded-full bg-emerald-500"
                        : "h-1.5 w-1.5 rounded-full bg-muted-foreground/40"
                    }
                  />
                  {s.label}
                  <span className="text-muted-foreground/60">· {s.serverCount}</span>
                </span>
              ))}
            </div>
          ) : null}

          {/* Recommended / built-in MCP presets */}
          {presets.length > 0 ? (
            <div className="mb-5">
              <h3 className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                <Sparkles className="h-3.5 w-3.5" />
                推荐安装
              </h3>
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {presets.map((preset) => {
                  const added = servers.some((s) => s.name === preset.name);
                  return (
                    <div
                      key={preset.id}
                      className="group flex flex-col gap-2 rounded-xl border border-dashed border-border/60 bg-card/40 p-4 transition hover:border-primary/30"
                    >
                      <div className="flex items-center gap-2">
                        <Boxes className="h-4 w-4 shrink-0 text-primary" />
                        <span className="truncate text-sm font-semibold text-foreground">{preset.name}</span>
                        <span className="ml-auto shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
                          {preset.transport}
                        </span>
                      </div>
                      {preset.description ? (
                        <p className="line-clamp-2 text-[11px] text-muted-foreground/80">{preset.description}</p>
                      ) : null}
                      <div className="mt-auto flex items-center justify-between gap-2 pt-1">
                        {preset.homepage ? (
                          <ExternalAnchor
                            href={preset.homepage}
                            className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
                          >
                            <ExternalLink className="h-3 w-3" />
                            主页
                          </ExternalAnchor>
                        ) : (
                          <span />
                        )}
                        <Button
                          size="sm"
                          variant={added ? "outline" : "default"}
                          disabled={added}
                          onClick={() => setDrawer({ type: "create-preset", preset })}
                          className="gap-1.5"
                        >
                          {added ? (
                            <>
                              <Check className="h-3.5 w-3.5" />
                              已添加
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
                })}
              </div>
            </div>
          ) : null}

          {isLoading ? (
            <div className="rounded-xl border border-border/55 bg-card/40 px-6 py-10 text-center text-sm text-muted-foreground">
              加载中…
            </div>
          ) : servers.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/60 bg-card/50 px-8 py-12 text-center">
              <p className="text-sm text-muted-foreground">尚未配置任何 MCP 服务器</p>
              <p className="mt-1 text-xs text-muted-foreground/70">
                添加后可一键启用到 Claude Code / Codex / Gemini / OpenCode
              </p>
              <div className="mt-4 flex justify-center gap-2">
                <Button variant="outline" onClick={() => void handleImport()} className="gap-1.5">
                  <Download className="h-4 w-4" />
                  从工具导入
                </Button>
                <Button onClick={() => setDrawer({ type: "create" })} className="gap-1.5">
                  <Plug className="h-4 w-4" />
                  新增第一个 MCP
                </Button>
              </div>
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
              {servers.map((s) => (
                <McpServerCard
                  key={s.id}
                  server={s}
                  toolStatuses={toolStatuses}
                  onOpen={() => setDrawer({ type: "edit", id: s.id })}
                  onToggleTool={(toolId, enabled) => void handleToggle(s.id, toolId, enabled)}
                />
              ))}
            </div>
          )}
        </>
      )}

      <ProviderDrawer
        open={drawer.type !== "closed"}
        onOpenChange={(open) => {
          if (!open) setDrawer({ type: "closed" });
        }}
        title={
          <span className="flex items-center gap-2 text-foreground">
            <Boxes className="h-4 w-4 text-primary" />
            {drawer.type === "edit"
              ? (editing?.name ?? "MCP 服务器")
              : drawer.type === "create-preset"
                ? drawer.preset.name
                : drawer.type === "install-market"
                  ? drawer.sourceName
                  : "新增 MCP 服务器"}
          </span>
        }
        subtitle={
          drawer.type === "create-preset" || drawer.type === "install-market"
            ? "已预填市场配置 — 请补全所需密钥并勾选工具后添加"
            : "一处编辑 → 勾选工具即写入对应配置文件"
        }
      >
        {drawer.type === "create" ? (
          <McpServerForm onSubmit={handleSubmit} submitting={saving} />
        ) : drawer.type === "install-market" ? (
          <McpServerForm
            key={drawer.sourceName}
            defaults={drawer.defaults}
            submitLabel="添加"
            onSubmit={handleSubmit}
            submitting={saving}
          />
        ) : drawer.type === "create-preset" ? (
          <McpServerForm
            key={drawer.preset.id}
            defaults={presetToDefaults(drawer.preset)}
            submitLabel="添加"
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
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">服务器不存在</div>
        ) : null}
      </ProviderDrawer>
    </section>
  );
}
