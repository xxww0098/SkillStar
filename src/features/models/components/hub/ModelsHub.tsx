import { motion } from "framer-motion";
import { FileCog, Loader2, Plug, Search, Server, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { useNavigation } from "../../../../hooks/useNavigation";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import { useProvidersFlat } from "../../hooks/useProvidersFlat";
import { useToolInstallStatuses } from "../../api/install";
import { useAgentHealth } from "../../hooks/useAgentHealth";
import { CLAUDE_DESKTOP_TOOL_ID, PROVIDER_AGENTS, type ProviderToolId } from "../../lib/agentRegistry";
import { computeAgentStatus, summarizeAgentStatuses } from "../../lib/agentStatus";
import { AgentHeroCard } from "../agents/AgentHeroCard";
import { AgentSettingsDialog } from "../agents/AgentSettingsDialog";
import { ClaudeDesktopDrawerContent } from "./ClaudeDesktopDrawerContent";
import { ClaudeDesktopHeroCard } from "./ClaudeDesktopHeroCard";
import { PresetPicker } from "../provider/PresetPicker";
import { DrawerShell } from "../shared/DrawerShell";
import { ProviderEditorDrawer } from "../provider/ProviderEditorDrawer";
import { ProviderGalleryCard } from "./ProviderGalleryCard";

const HUB_TOOL_IDS = [...PROVIDER_AGENTS.map((a) => a.toolId), CLAUDE_DESKTOP_TOOL_ID];

type DrawerMode =
  | { type: "closed" }
  | { type: "create"; autoBindToolId?: string }
  | { type: "edit"; providerId: string; autoBindToolId?: string }
  | { type: "claude-desktop" };

function ModelsTopDragStrip() {
  return <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />;
}

export function ModelsHub() {
  const { providers, toolActivations, isLoading, activateTool, createProvider, deleteProvider } = useProvidersFlat();
  const { selectedProviderId, setSelectedProviderId, showPresetSelector, setShowPresetSelector } = useNavigation();

  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [settingsTool, setSettingsTool] = useState<ProviderToolId | null>(null);
  const { byTool: installStatus, isLoading: installLoading } = useToolInstallStatuses(HUB_TOOL_IDS);
  const health = useAgentHealth(providers, toolActivations);
  const [galleryQuery, setGalleryQuery] = useState("");

  // Bridge: rehydrate drawer from URL/persisted state on first mount.
  useEffect(() => {
    if (showPresetSelector) {
      setDrawer({ type: "create" });
    } else if (selectedProviderId && drawer.type === "closed") {
      // Only auto-open if the user previously had a provider selected from a deep-link.
      // We don't auto-open by default — too aggressive.
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showPresetSelector]);

  // ── Drawer handlers ────────────────────────────────────────────
  const openCreateDrawer = useCallback(
    (autoBindToolId?: string) => {
      setShowPresetSelector(true);
      setDrawer({ type: "create", autoBindToolId });
    },
    [setShowPresetSelector],
  );

  const openEditDrawer = useCallback(
    (providerId: string) => {
      setSelectedProviderId(providerId);
      setShowPresetSelector(false);
      setDrawer({ type: "edit", providerId });
    },
    [setSelectedProviderId, setShowPresetSelector],
  );

  const closeDrawer = useCallback(() => {
    setShowPresetSelector(false);
    setDrawer({ type: "closed" });
  }, [setShowPresetSelector]);

  const handleProviderCreated = useCallback(
    async (provider: ProviderEntryFlat) => {
      const autoBind = drawer.type === "create" ? drawer.autoBindToolId : undefined;
      if (autoBind) {
        const agent = PROVIDER_AGENTS.find((a) => a.toolId === autoBind);
        const compatible = agent
          ? agent.requiredUrlField === "anthropic"
            ? !!provider.base_url_anthropic
            : !!provider.base_url_openai
          : false;
        if (compatible) {
          await activateTool(provider.id, autoBind, provider.default_model || undefined)
            .then(() => toast.success(`已为 ${agent?.displayName ?? autoBind} 绑定 ${provider.name}`))
            .catch(() => {});
        }
      }
      setSelectedProviderId(provider.id);
      setShowPresetSelector(false);
      setDrawer({ type: "edit", providerId: provider.id });
    },
    [drawer, activateTool, setSelectedProviderId, setShowPresetSelector],
  );

  const handleDuplicateProvider = useCallback(
    async (p: ProviderEntryFlat) => {
      try {
        await createProvider({ ...p, id: "", name: `${p.name} (副本)` });
        toast.success("已复制供应商");
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [createProvider],
  );

  const handleDeleteProvider = useCallback(
    async (p: ProviderEntryFlat) => {
      try {
        await deleteProvider(p.id);
        if (selectedProviderId === p.id) setSelectedProviderId(null);
        toast.success(`已删除 ${p.name}`);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [deleteProvider, selectedProviderId, setSelectedProviderId],
  );

  // Provider gallery search filter.
  const filteredProviders = useMemo(() => {
    const q = galleryQuery.trim().toLowerCase();
    if (!q) return providers;
    return providers.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.preset_id?.toLowerCase().includes(q) ||
        p.default_model.toLowerCase().includes(q),
    );
  }, [providers, galleryQuery]);

  const agentStatuses = useMemo(
    () =>
      PROVIDER_AGENTS.map((agent) => {
        const activation = toolActivations[agent.toolId] ?? null;
        const boundProvider = activation?.provider_id
          ? (providers.find((p) => p.id === activation.provider_id) ?? null)
          : null;
        return computeAgentStatus({
          agent,
          activation,
          boundProvider,
          installed: installStatus[agent.toolId]?.installed ?? true,
          installLoading,
          probe: health.results[agent.toolId] ?? null,
          probing: health.testing[agent.toolId] ?? false,
        });
      }),
    [toolActivations, providers, installStatus, installLoading, health.results, health.testing],
  );
  const agentSummary = useMemo(() => summarizeAgentStatuses(agentStatuses), [agentStatuses]);

  const drawerProvider = useMemo(() => {
    if (drawer.type !== "edit") return null;
    return providers.find((p) => p.id === drawer.providerId) ?? null;
  }, [drawer, providers]);

  // ── Render ─────────────────────────────────────────────────────
  if (isLoading) {
    return (
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <ModelsTopDragStrip />
        <main className="ss-page-scroll">
          <div className="flex min-h-[60vh] items-center justify-center">
            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          </div>
        </main>
      </div>
    );
  }

  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <ModelsTopDragStrip />
      <main className="ss-page-scroll">
        <div className="mx-auto w-full max-w-6xl px-6 py-6 space-y-6">
          {/* Hero header */}
          <header className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight text-foreground">
                <Sparkles className="h-5 w-5 text-primary" />
                模型工作台
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                一处管理 Agent 的供应商绑定 与 Claude Desktop MCP 配置,所有改动自动保存。
              </p>
            </div>
            <Button onClick={() => openCreateDrawer()} className="gap-1.5">
              <Plug className="h-4 w-4" />
              新增供应商
            </Button>
          </header>

          {/* Agent hero row */}
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground">Agent 接入</h2>
              <span className="text-[11px] text-muted-foreground/75">
                {agentSummary.connected}/{agentSummary.total} 已接入
                {agentSummary.problems > 0 ? ` · ${agentSummary.problems} 异常` : ""}
              </span>
            </div>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {PROVIDER_AGENTS.map((agent) => (
                <AgentHeroCard
                  key={agent.toolId}
                  agent={agent}
                  health={health}
                  onAddProvider={() => openCreateDrawer(agent.toolId)}
                  onOpenSettings={() => setSettingsTool(agent.toolId)}
                  onOpenProviderDrawer={openEditDrawer}
                />
              ))}
              <ClaudeDesktopHeroCard
                installed={installLoading ? false : (installStatus[CLAUDE_DESKTOP_TOOL_ID]?.installed ?? false)}
                installLoading={installLoading}
                onOpenConfig={() => setDrawer({ type: "claude-desktop" })}
              />
            </div>
          </section>

          {/* Providers gallery */}
          <section>
            <div className="mb-3 flex items-center justify-between gap-3">
              <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
                <Server className="h-3.5 w-3.5" />
                供应商 <span className="text-muted-foreground/70">({providers.length})</span>
              </h2>
              <div className="relative w-64">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                <Input
                  value={galleryQuery}
                  onChange={(e) => setGalleryQuery(e.target.value)}
                  placeholder="搜索供应商..."
                  className="h-9 pl-9 text-xs"
                />
              </div>
            </div>

            {providers.length === 0 ? (
              <motion.div
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                className="rounded-xl border border-dashed border-border/60 bg-card/50 px-8 py-12 text-center"
              >
                <p className="text-sm text-muted-foreground">尚未配置任何供应商</p>
                <Button onClick={() => openCreateDrawer()} className="mt-4 gap-1.5">
                  <Plug className="h-4 w-4" />
                  新增第一个供应商
                </Button>
              </motion.div>
            ) : filteredProviders.length === 0 ? (
              <div className="rounded-xl border border-border/55 bg-card/55 px-6 py-10 text-center text-sm text-muted-foreground">
                没有匹配的供应商
              </div>
            ) : (
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {filteredProviders.map((p) => (
                  <ProviderGalleryCard
                    key={p.id}
                    provider={p}
                    toolActivations={toolActivations}
                    onOpen={() => openEditDrawer(p.id)}
                    onDuplicate={() => void handleDuplicateProvider(p)}
                    onDelete={() => void handleDeleteProvider(p)}
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      </main>

      {/* ── Create / Claude Desktop drawer ───────────────────── */}
      <DrawerShell
        open={
          drawer.type === "create" || drawer.type === "claude-desktop" || (drawer.type === "edit" && !drawerProvider)
        }
        onOpenChange={(open) => {
          if (!open) closeDrawer();
        }}
        title={
          drawer.type === "create" ? (
            <span className="flex items-center gap-2 text-foreground">
              <Plug className="h-4 w-4 text-primary" />
              新增供应商
            </span>
          ) : drawer.type === "claude-desktop" ? (
            <span className="flex items-center gap-2 text-foreground">
              <FileCog className="h-4 w-4 text-primary" />
              Claude Desktop · MCP 配置
            </span>
          ) : (
            <span className="text-foreground">供应商</span>
          )
        }
        subtitle={
          drawer.type === "create"
            ? "选择预设 → 填写 Key → 自动进入详细配置"
            : drawer.type === "claude-desktop"
              ? "直接编辑 claude_desktop_config.json — 仅支持 mcpServers 节点"
              : null
        }
      >
        {drawer.type === "create" ? (
          <PresetPicker onProviderCreated={(p) => void handleProviderCreated(p)} />
        ) : drawer.type === "claude-desktop" ? (
          <ClaudeDesktopDrawerContent />
        ) : (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">供应商不存在</div>
        )}
      </DrawerShell>

      {/* ── Agent settings dialog ─────────────────────────────── */}
      {settingsTool ? (
        <AgentSettingsDialog
          toolId={settingsTool}
          open
          onClose={() => setSettingsTool(null)}
          onAddProvider={() => {
            setSettingsTool(null);
            openCreateDrawer(settingsTool);
          }}
          onOpenProviderDrawer={(providerId) => {
            setSettingsTool(null);
            openEditDrawer(providerId);
          }}
        />
      ) : null}

      {/* ── Provider editor drawer (tabbed, owns its save state) ── */}
      {drawerProvider ? (
        <ProviderEditorDrawer
          provider={drawerProvider}
          open={drawer.type === "edit"}
          onClose={closeDrawer}
          onDuplicate={(p) => void handleDuplicateProvider(p)}
          onDelete={(p) => {
            void handleDeleteProvider(p);
            closeDrawer();
          }}
        />
      ) : null}
    </div>
  );
}

// Help dead-code elimination — keep the obvious imports happy.
void cn;
