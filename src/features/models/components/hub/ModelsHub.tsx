import { motion } from "framer-motion";
import { AlertCircle, CheckCircle2, FileCog, Loader2, Plug, Search, Server, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { useNavigation } from "../../../../hooks/useNavigation";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat, ProviderPatchFlat } from "../../../../types";
import { useProvidersFlat } from "../../hooks/useProvidersFlat";
import { ProviderBrandIcon } from "../ProviderBrandIcon";
import type { ProviderSaveState } from "../providerForm/useProviderFormState";
import { AgentHeroCard, type AgentHeroAgent } from "./AgentHeroCard";
import { ClaudeDesktopDrawerContent } from "./ClaudeDesktopDrawerContent";
import { ClaudeDesktopHeroCard } from "./ClaudeDesktopHeroCard";
import { HealthBar } from "./HealthBar";
import { PresetPicker } from "./PresetPicker";
import { ProviderDrawer } from "./ProviderDrawer";
import { ProviderDrawerForm } from "./ProviderDrawerForm";
import { ProviderGalleryCard } from "./ProviderGalleryCard";

interface InstallStatus {
  installed: boolean;
  binary_found: boolean;
  config_dir_found: boolean;
}

const AGENTS: AgentHeroAgent[] = [
  {
    toolId: "claude-code",
    displayName: "Claude",
    iconId: "claude-code",
    requiredUrlField: "anthropic",
    installDocsUrl: "https://docs.anthropic.com/en/docs/claude-code/overview",
    tagline: "Anthropic 兼容 · 写入 ~/.claude/settings.json",
  },
  {
    toolId: "codex",
    displayName: "Codex",
    iconId: "codex",
    requiredUrlField: "openai",
    installDocsUrl: "https://github.com/openai/codex",
    tagline: "CLI · Desktop App · IDE 扩展 共用 ~/.codex/ 配置",
  },
  {
    toolId: "opencode",
    displayName: "OpenCode",
    iconId: "opencode",
    requiredUrlField: "openai",
    installDocsUrl: "https://opencode.ai/docs",
    tagline: "OpenAI 兼容 · 开源 IDE 代理",
  },
  {
    toolId: "gemini",
    displayName: "Gemini CLI",
    iconId: "gemini",
    requiredUrlField: "openai",
    installDocsUrl: "https://github.com/google-gemini/gemini-cli",
    tagline: "OpenAI 兼容 · 写入 ~/.gemini/.env",
  },
];

type DrawerMode =
  | { type: "closed" }
  | { type: "create"; autoBindToolId?: string }
  | { type: "edit"; providerId: string; autoBindToolId?: string }
  | { type: "claude-desktop" };

const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop";

function ModelsTopDragStrip() {
  return <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />;
}

function SaveBadge({ state }: { state: ProviderSaveState }) {
  if (state === "saving") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
        <Loader2 className="h-3 w-3 animate-spin" />
        保存中
      </span>
    );
  }
  if (state === "dirty") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-500">
        未保存
      </span>
    );
  }
  if (state === "error") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-destructive/25 bg-destructive/10 px-2 py-0.5 text-[11px] font-medium text-destructive">
        <AlertCircle className="h-3 w-3" />
        保存失败
      </span>
    );
  }
  if (state === "saved") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-success/20 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
        <CheckCircle2 className="h-3 w-3" />
        已保存
      </span>
    );
  }
  return null;
}

export function ModelsHub() {
  const {
    providers,
    toolActivations,
    isLoading,
    activateTool,
    deactivateTool,
    updateProvider,
    createProvider,
    deleteProvider,
  } = useProvidersFlat();
  const { selectedProviderId, setSelectedProviderId, showPresetSelector, setShowPresetSelector } = useNavigation();

  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [installStatus, setInstallStatus] = useState<Record<string, InstallStatus>>({});
  const [installLoading, setInstallLoading] = useState(true);
  const [saveState, setSaveState] = useState<ProviderSaveState>("idle");
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

  // Detect agent installations once (provider-bound agents + Claude Desktop).
  useEffect(() => {
    let cancelled = false;
    async function detect() {
      setInstallLoading(true);
      const results: Record<string, InstallStatus> = {};
      const toolIds = [...AGENTS.map((a) => a.toolId), CLAUDE_DESKTOP_TOOL_ID];
      for (const toolId of toolIds) {
        try {
          const result = await tauriInvoke("detect_tool_installation", { toolId });
          if (!cancelled) results[toolId] = result;
        } catch {
          if (!cancelled) results[toolId] = { installed: false, binary_found: false, config_dir_found: false };
        }
      }
      if (!cancelled) {
        setInstallStatus(results);
        setInstallLoading(false);
      }
    }
    detect();
    return () => {
      cancelled = true;
    };
  }, []);

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
      setSaveState("idle");
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
        const agent = AGENTS.find((a) => a.toolId === autoBind);
        const compatible = agent
          ? agent.requiredUrlField === "anthropic"
            ? !!provider.base_url_anthropic
            : !!provider.base_url_openai
          : false;
        if (compatible) {
          try {
            await activateTool(provider.id, autoBind, provider.default_model || undefined);
            toast.success(`已为 ${agent?.displayName ?? autoBind} 绑定 ${provider.name}`);
          } catch (err) {
            toast.error(err instanceof Error ? err.message : String(err));
          }
        }
      }
      setSelectedProviderId(provider.id);
      setShowPresetSelector(false);
      setSaveState("idle");
      setDrawer({ type: "edit", providerId: provider.id });
    },
    [drawer, activateTool, setSelectedProviderId, setShowPresetSelector],
  );

  const handleSave = useCallback(
    async (patch: ProviderPatchFlat) => {
      if (drawer.type !== "edit") return;
      await updateProvider(drawer.providerId, patch);
    },
    [drawer, updateProvider],
  );

  const handleAgentActivate = useCallback(
    async (toolId: string, providerId: string, model?: string) => {
      try {
        await activateTool(providerId, toolId, model);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [activateTool],
  );

  const handleAgentDeactivate = useCallback(
    async (toolId: string) => {
      try {
        await deactivateTool(toolId);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [deactivateTool],
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

          {/* Health bar */}
          <HealthBar
            providers={providers}
            toolActivations={toolActivations}
            claudeDesktopInstalled={
              installLoading ? false : (installStatus[CLAUDE_DESKTOP_TOOL_ID]?.installed ?? false)
            }
            claudeDesktopInstallLoading={installLoading}
            onOpenClaudeDesktopConfig={() => setDrawer({ type: "claude-desktop" })}
          />

          {/* Agent hero row */}
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground">Agent 绑定</h2>
            </div>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
              {AGENTS.map((agent) => {
                const status = installStatus[agent.toolId];
                return (
                  <AgentHeroCard
                    key={agent.toolId}
                    agent={agent}
                    providers={providers}
                    activation={toolActivations[agent.toolId] ?? null}
                    installed={installLoading ? true : (status?.installed ?? true)}
                    installLoading={installLoading}
                    onActivate={(providerId, model) => handleAgentActivate(agent.toolId, providerId, model)}
                    onDeactivate={() => handleAgentDeactivate(agent.toolId)}
                    onAddProvider={() => openCreateDrawer(agent.toolId)}
                    onOpenDrawer={openEditDrawer}
                  />
                );
              })}
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

      {/* ── Drawer ────────────────────────────────────────────── */}
      <ProviderDrawer
        open={drawer.type !== "closed"}
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
          ) : drawerProvider ? (
            <span className="flex min-w-0 items-center gap-2 text-foreground">
              <ProviderBrandIcon
                presetId={drawerProvider.preset_id}
                providerName={drawerProvider.name}
                iconColor={drawerProvider.icon_color}
                size="sm"
              />
              <span className="truncate">{drawerProvider.name}</span>
            </span>
          ) : (
            <span className="text-foreground">供应商</span>
          )
        }
        subtitle={
          drawer.type === "create" ? (
            "选择预设 → 填写 Key → 自动进入详细配置"
          ) : drawer.type === "claude-desktop" ? (
            "直接编辑 claude_desktop_config.json — 仅支持 mcpServers 节点"
          ) : drawer.type === "edit" ? (
            <span className="flex items-center gap-2">
              <span>连接 · Agent 同步 · 高级</span>
              <SaveBadge state={saveState} />
            </span>
          ) : null
        }
        footer={
          drawer.type === "edit" ? (
            <div className="flex items-center justify-between gap-3">
              <span className="text-[11px] text-muted-foreground">
                {saveState === "saving" ? (
                  <span className="inline-flex items-center gap-1.5">
                    <Loader2 className="h-3 w-3 animate-spin" />
                    保存中…
                  </span>
                ) : saveState === "dirty" ? (
                  "改动将自动保存"
                ) : saveState === "error" ? (
                  <span className="text-destructive">保存失败,请检查表单</span>
                ) : (
                  "所有改动自动保存到本机"
                )}
              </span>
              <Button variant="outline" size="sm" onClick={closeDrawer}>
                完成
              </Button>
            </div>
          ) : null
        }
      >
        {drawer.type === "create" ? (
          <PresetPicker onProviderCreated={(p) => void handleProviderCreated(p)} />
        ) : drawer.type === "claude-desktop" ? (
          <ClaudeDesktopDrawerContent />
        ) : drawer.type === "edit" && drawerProvider ? (
          <ProviderDrawerForm
            key={drawerProvider.id}
            provider={drawerProvider}
            onSave={handleSave}
            onSaveStateChange={setSaveState}
          />
        ) : drawer.type === "edit" && !drawerProvider ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">供应商不存在</div>
        ) : null}
      </ProviderDrawer>
    </div>
  );
}

// Help dead-code elimination — keep the obvious imports happy.
void cn;
