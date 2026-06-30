import { motion } from "framer-motion";
import { Loader2, Plug, Search, Server, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { useNavigation } from "../../../../hooks/useNavigation";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import { getProviderToolBadges, useProvidersFlat } from "../../hooks/useProvidersFlat";
import { useToolInstallStatuses } from "../../api/install";
import { useAgentHealth } from "../../hooks/useAgentHealth";
import { CLAUDE_DESKTOP_TOOL_ID, PROVIDER_AGENTS, type ProviderToolId } from "../../lib/agentRegistry";
import { computeAgentStatus, summarizeAgentStatuses } from "../../lib/agentStatus";
import { activeEntry as bindingActiveEntry } from "../../lib/toolBinding";
import { AgentHeroCard } from "../agents/AgentHeroCard";
import { AgentSettingsDialog } from "../agents/AgentSettingsDialog";
import { AppAiCard } from "../agents/AppAiCard";
import { ClaudeDesktopCard } from "../agents/ClaudeDesktopCard";
import { ClaudeDesktopConfigDialog } from "../agents/ClaudeDesktopConfigDialog";
import { MultiProviderCard } from "../agents/MultiProviderCard";
import { PresetPicker } from "../provider/PresetPicker";
import { DrawerShell } from "../shared/DrawerShell";
import { ProviderEditorDrawer } from "../provider/ProviderEditorDrawer";
import { DeleteProviderDialog } from "./DeleteProviderDialog";
import { ProviderGalleryCard } from "./ProviderGalleryCard";

const HUB_TOOL_IDS = [...PROVIDER_AGENTS.map((a) => a.toolId), CLAUDE_DESKTOP_TOOL_ID];

type DrawerMode =
  | { type: "closed" }
  | { type: "create"; autoBindToolId?: string }
  | { type: "edit"; providerId: string; autoBindToolId?: string; postCreate?: boolean };

function ModelsTopDragStrip() {
  return <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />;
}

export function ModelsHub() {
  const { t } = useTranslation();
  const { providers, toolActivations, isLoading, activateTool, createProvider, deleteProvider } = useProvidersFlat();
  const { selectedProviderId, setSelectedProviderId, navigate, modelsDrawerRequest, clearModelsDrawerRequest } =
    useNavigation();

  const [drawer, setDrawer] = useState<DrawerMode>({ type: "closed" });
  const [settingsTool, setSettingsTool] = useState<ProviderToolId | null>(null);
  const [desktopConfigOpen, setDesktopConfigOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ProviderEntryFlat | null>(null);
  const [connectHintDismissed, setConnectHintDismissed] = useState(false);
  const { byTool: installStatus, isLoading: installLoading } = useToolInstallStatuses(HUB_TOOL_IDS);
  const health = useAgentHealth(providers, toolActivations);
  const [galleryQuery, setGalleryQuery] = useState("");

  // Deep-link requests from the sidebar / other surfaces (request-nonce pattern).
  useEffect(() => {
    if (!modelsDrawerRequest) return;
    const req = modelsDrawerRequest;
    clearModelsDrawerRequest();
    if (req.kind === "create") {
      setDrawer({ type: "create", autoBindToolId: req.autoBindToolId });
    } else if (req.providerId) {
      setSelectedProviderId(req.providerId);
      setDrawer({ type: "edit", providerId: req.providerId });
    }
  }, [modelsDrawerRequest, clearModelsDrawerRequest, setSelectedProviderId]);

  // ── Drawer handlers ────────────────────────────────────────────
  const openCreateDrawer = useCallback((autoBindToolId?: string) => {
    setDrawer({ type: "create", autoBindToolId });
  }, []);

  const openEditDrawer = useCallback(
    (providerId: string) => {
      setSelectedProviderId(providerId);
      setDrawer({ type: "edit", providerId });
    },
    [setSelectedProviderId],
  );

  const closeDrawer = useCallback(() => {
    setDrawer({ type: "closed" });
  }, []);

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
            .then(() =>
              toast.success(
                t("models.toasts.boundAgent", { agent: agent?.displayName ?? autoBind, provider: provider.name }),
              ),
            )
            .catch(() => {});
        }
      }
      setSelectedProviderId(provider.id);
      setDrawer({ type: "edit", providerId: provider.id, postCreate: true });
    },
    [drawer, activateTool, setSelectedProviderId],
  );

  const handleDuplicateProvider = useCallback(
    async (p: ProviderEntryFlat) => {
      try {
        await createProvider({ ...p, id: "", name: t("models.hub.duplicateSuffix", { name: p.name }) });
        toast.success(t("models.toasts.duplicated"));
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [createProvider],
  );

  const confirmDeleteProvider = useCallback(
    async (p: ProviderEntryFlat) => {
      setDeleteTarget(null);
      try {
        await deleteProvider(p.id);
        if (selectedProviderId === p.id) setSelectedProviderId(null);
        setDrawer((prev) => (prev.type === "edit" && prev.providerId === p.id ? { type: "closed" } : prev));
        toast.success(t("models.toasts.deleted", { name: p.name }));
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
        const activation = bindingActiveEntry(toolActivations[agent.toolId]);
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

  const providerLatency = useMemo(() => {
    const map: Record<string, number | null> = {};
    for (const [toolId, binding] of Object.entries(toolActivations)) {
      const activation = bindingActiveEntry(binding);
      const result = health.results[toolId];
      if (activation?.provider_id && result?.status === "ok") {
        map[activation.provider_id] = result.latency_ms ?? null;
      }
    }
    return map;
  }, [toolActivations, health.results]);

  const noAgentConnected = useMemo(
    () => providers.length > 0 && Object.values(toolActivations).every((b) => !b || b.entries.length === 0),
    [providers.length, toolActivations],
  );

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
                {t("models.hub.title")}
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">{t("models.hub.subtitle")}</p>
            </div>
            <Button onClick={() => openCreateDrawer()} className="gap-1.5">
              <Plug className="h-4 w-4" />
              {t("models.hub.addProvider")}
            </Button>
          </header>

          {/* Agent hero row */}
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
                {t("models.hub.agentsSection")}
              </h2>
              <span className="text-[11px] text-muted-foreground/75">
                {t("models.hub.connectedSummary", { connected: agentSummary.connected, total: agentSummary.total })}
                {agentSummary.problems > 0 ? ` · ${t("models.hub.problems", { count: agentSummary.problems })}` : ""}
              </span>
            </div>
            {noAgentConnected && !connectHintDismissed ? (
              <div className="mb-3 flex items-center justify-between gap-2 rounded-xl border border-primary/20 bg-primary/[0.05] px-3 py-2 text-[11px] text-muted-foreground">
                <span>{t("models.hub.connectHint", { count: providers.length })}</span>
                <button
                  type="button"
                  onClick={() => setConnectHintDismissed(true)}
                  className="shrink-0 font-medium text-primary hover:underline"
                >
                  {t("models.hub.gotIt")}
                </button>
              </div>
            ) : null}
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {PROVIDER_AGENTS.map((agent) => {
                const cardProps = {
                  agent,
                  health,
                  onAddProvider: () => openCreateDrawer(agent.toolId),
                  onOpenSettings: () => setSettingsTool(agent.toolId),
                  onOpenProviderDrawer: openEditDrawer,
                };
                return agent.kind === "multi" ? (
                  <MultiProviderCard key={agent.toolId} {...cardProps} />
                ) : (
                  <AgentHeroCard key={agent.toolId} {...cardProps} />
                );
              })}
              <ClaudeDesktopCard
                installed={installLoading ? false : (installStatus[CLAUDE_DESKTOP_TOOL_ID]?.installed ?? false)}
                installLoading={installLoading}
                onOpenConfig={() => setDesktopConfigOpen(true)}
              />
              <AppAiCard onOpenSettings={() => navigate("settings")} />
            </div>
          </section>

          {/* Providers gallery */}
          <section>
            <div className="mb-3 flex items-center justify-between gap-3">
              <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted-foreground">
                <Server className="h-3.5 w-3.5" />
                {t("models.hub.providersSection")}{" "}
                <span className="text-muted-foreground/70">({providers.length})</span>
              </h2>
              <div className="relative w-64">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                <Input
                  value={galleryQuery}
                  onChange={(e) => setGalleryQuery(e.target.value)}
                  placeholder={t("models.gallery.searchPlaceholder")}
                  className="h-9 pl-9 text-xs"
                />
              </div>
            </div>

            {providers.length === 0 ? (
              <motion.div
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                className="rounded-xl border border-dashed border-border/60 bg-card/50 px-8 py-10 text-center"
              >
                <h3 className="text-sm font-semibold text-foreground">{t("models.gallery.emptyTitle")}</h3>
                <div className="mx-auto mt-3 flex max-w-md flex-wrap items-center justify-center gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded-full border border-border/55 px-2.5 py-1">{t("models.gallery.step1")}</span>
                  <span aria-hidden>→</span>
                  <span className="rounded-full border border-border/55 px-2.5 py-1">{t("models.gallery.step2")}</span>
                  <span aria-hidden>→</span>
                  <span className="rounded-full border border-border/55 px-2.5 py-1">{t("models.gallery.step3")}</span>
                </div>
                <Button onClick={() => openCreateDrawer()} className="mt-5 gap-1.5">
                  <Plug className="h-4 w-4" />
                  {t("models.gallery.addFirst")}
                </Button>
              </motion.div>
            ) : filteredProviders.length === 0 ? (
              <div className="rounded-xl border border-border/55 bg-card/55 px-6 py-10 text-center text-sm text-muted-foreground">
                {t("models.gallery.noMatch")}
              </div>
            ) : (
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {filteredProviders.map((p) => (
                  <ProviderGalleryCard
                    key={p.id}
                    provider={p}
                    toolActivations={toolActivations}
                    latencyMs={providerLatency[p.id]}
                    onOpen={() => openEditDrawer(p.id)}
                    onDuplicate={() => void handleDuplicateProvider(p)}
                    onDelete={() => setDeleteTarget(p)}
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      </main>

      {/* ── Create drawer ─────────────────────────────────────── */}
      <DrawerShell
        open={drawer.type === "create" || (drawer.type === "edit" && !drawerProvider)}
        onOpenChange={(open) => {
          if (!open) closeDrawer();
        }}
        title={
          drawer.type === "create" ? (
            <span className="flex items-center gap-2 text-foreground">
              <Plug className="h-4 w-4 text-primary" />
              {t("models.hub.addProvider")}
            </span>
          ) : (
            <span className="text-foreground">{t("models.drawer.titleFallback")}</span>
          )
        }
        subtitle={drawer.type === "create" ? t("models.drawer.createSubtitle") : null}
      >
        {drawer.type === "create" ? (
          <PresetPicker onProviderCreated={(p) => void handleProviderCreated(p)} />
        ) : (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
            {t("models.drawer.providerMissing")}
          </div>
        )}
      </DrawerShell>

      {/* ── Claude Desktop MCP config dialog ──────────────────── */}
      <ClaudeDesktopConfigDialog open={desktopConfigOpen} onClose={() => setDesktopConfigOpen(false)} />

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
          showPostCreateGuide={drawer.type === "edit" && !!drawer.postCreate}
          agentBoundOnCreate={getProviderToolBadges(drawerProvider.id, toolActivations).length > 0}
          onClose={closeDrawer}
          onDuplicate={(p) => void handleDuplicateProvider(p)}
          onDelete={(p) => setDeleteTarget(p)}
        />
      ) : null}

      {/* ── Delete confirmation ───────────────────────────────── */}
      <DeleteProviderDialog
        provider={deleteTarget}
        affectedToolIds={deleteTarget ? getProviderToolBadges(deleteTarget.id, toolActivations) : []}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={(p) => void confirmDeleteProvider(p)}
      />
    </div>
  );
}

// Help dead-code elimination — keep the obvious imports happy.
void cn;
