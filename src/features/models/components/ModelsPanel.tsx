import { AnimatePresence, motion } from "framer-motion";
import { CheckCircle2, FileCode2, Loader2, Package } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SKILLSTAR_MODELS_PENDING_APP_KEY } from "../../../hooks/useNavigation";
import { openExternalUrl } from "../../../lib/externalOpen";
import { DRAG_CSS, useDragReorder } from "../hooks/useDragReorder";
import { type ProviderEntry, useModelProviders, useOpenCodeNativeProviders } from "../hooks/useModelProviders";
import { useProviderHealthDashboard } from "../hooks/useProviderHealthDashboard";
import { AppCapsuleSwitcher, type ModelAppId } from "./AppCapsuleSwitcher";
import { BehaviorStrip } from "./BehaviorStrip";
import { CodexAccountSection } from "./CodexAccountSection";
import { ConfigFileEditor, type ConfigFileKey } from "./ConfigFileEditor";
import { GeminiAccountSection } from "./GeminiAccountSection";
import { OpenCodeQuickLinks } from "./OpenCodeQuickLinks";
import { PresetCatalog } from "./PresetCatalog";
import { ProviderCard } from "./ProviderCard";
import { ProviderHealthDashboardCard } from "./ProviderHealthDashboard";
import { AgentIcon } from "./shared/ProviderIcon";

function readInitialModelsApp(): ModelAppId {
  try {
    const pending = sessionStorage.getItem(SKILLSTAR_MODELS_PENDING_APP_KEY);
    if (pending === "claude" || pending === "codex" || pending === "opencode" || pending === "gemini") {
      sessionStorage.removeItem(SKILLSTAR_MODELS_PENDING_APP_KEY);
      return pending;
    }
  } catch {
    /* ignore */
  }
  try {
    const saved = localStorage.getItem("models-active-app");
    if (saved === "claude" || saved === "codex" || saved === "opencode" || saved === "gemini") return saved;
  } catch {
    /* ignore */
  }
  return "claude";
}

const APP_COLORS: Record<ModelAppId, string> = {
  claude: "#D97757",
  codex: "#00A67E",
  opencode: "#6366F1",
  gemini: "#3B82F6",
};

/** Config files available for each app */
const APP_CONFIG_FILES: Record<ModelAppId, { key: ConfigFileKey; label: string; path: string }[]> = {
  claude: [{ key: "claude", label: "settings.json", path: "~/.claude/settings.json" }],
  codex: [{ key: "codex_config", label: "config.toml", path: "~/.codex/config.toml" }],
  opencode: [{ key: "opencode", label: "opencode.json", path: "~/.config/opencode/opencode.json" }],
  gemini: [],
};

/* ── Drag state is now fully managed in useDragReorder hook ── */

export function ModelsPanel() {
  const { t } = useTranslation();
  const [activeApp, setActiveApp] = useState<ModelAppId>(readInitialModelsApp);

  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [configEditorOpen, setConfigEditorOpen] = useState<ConfigFileKey | null>(null);
  const [configDropdownOpen, setConfigDropdownOpen] = useState(false);

  const genericProviders = useModelProviders(activeApp);
  const opencodeProviders = useOpenCodeNativeProviders();
  const providers = activeApp === "opencode" ? opencodeProviders : genericProviders;
  const healthDashboard = useProviderHealthDashboard(activeApp);
  const [localProviders, setLocalProviders] = useState<ProviderEntry[]>([]);
  const localProvidersRef = useRef(localProviders);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const listContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLocalProviders(providers.sortedProviders);
  }, [providers.sortedProviders]);

  useEffect(() => {
    localProvidersRef.current = localProviders;
  }, [localProviders]);

  useEffect(() => {
    localStorage.setItem("models-active-app", activeApp);
    setExpandedId(null);
  }, [activeApp]);

  const handleToggleExpand = useCallback((id: string) => {
    setExpandedId((prev) => (prev === id ? null : id));
  }, []);

  const handleAddPreset = useCallback(
    async (entry: ProviderEntry) => {
      await providers.addProvider(entry);
      setExpandedId(entry.id);
    },
    [providers],
  );

  // OAuth is the active auth method when no provider card is current.
  // Backend clears provider current when switching to an OAuth account.
  const isOAuthActive = !providers.currentId;

  // After switching OAuth account, reload provider state (backend cleared current).
  const handleAccountSwitched = useCallback(() => {
    providers.load(false);
  }, [providers]);

  const appColor = APP_COLORS[activeApp];

  // ── High-performance drag-to-reorder (zero React re-renders during drag) ──
  const { handleDragStart } = useDragReorder({
    items: localProviders,
    ghostRef,
    scrollContainerRef,
    listContainerRef,
    appColor,
    onReorder: (reordered: ProviderEntry[]) => {
      setLocalProviders(reordered);
      providers.reorderProviders(reordered.map((p: ProviderEntry) => p.id));
    },
  });

  return (
    <div className="flex-1 flex flex-col overflow-hidden relative">
      {/* Header */}
      <div className="shrink-0 px-6 py-4 border-b border-border space-y-2">
        <div className="flex items-center justify-between gap-4">
          <div className="flex flex-col sm:flex-row sm:items-center gap-2 sm:gap-6 min-w-0">
            <div className="flex items-center gap-3 min-w-0">
              <div
                className="w-10 h-10 rounded-xl border border-border flex items-center justify-center transition-colors duration-300 shrink-0"
                style={{ backgroundColor: `${appColor}15` }}
              >
                <AgentIcon appId={activeApp} color={appColor} size="w-5 h-5" className="transition-all duration-300" />
              </div>
              <div className="min-w-0">
                <h1 className="text-lg font-semibold text-foreground leading-tight">{t("modelPage.title")}</h1>
                <p className="text-[11px] text-muted-foreground leading-snug mt-0.5 max-w-xl">
                  {t("modelPage.subtitle")}
                </p>
              </div>
            </div>
            <AppCapsuleSwitcher value={activeApp} onChange={setActiveApp} />
          </div>

          <div className="flex items-center gap-1.5 shrink-0">
            {/* Config file editor button */}
            {APP_CONFIG_FILES[activeApp]?.length > 0 && (
              <div className="relative">
                <button
                  type="button"
                  onClick={() => {
                    const files = APP_CONFIG_FILES[activeApp];
                    if (files.length === 1) {
                      setConfigEditorOpen(files[0].key);
                    } else {
                      setConfigDropdownOpen((v) => !v);
                    }
                  }}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 border border-border transition-all"
                >
                  <FileCode2 className="w-3.5 h-3.5" />
                  {t("modelPage.configFiles")}
                </button>
                {/* Dropdown for multi-file apps (Codex) */}
                {configDropdownOpen && APP_CONFIG_FILES[activeApp].length > 1 && (
                  <>
                    <button
                      type="button"
                      className="fixed inset-0 z-40 cursor-default"
                      onClick={() => setConfigDropdownOpen(false)}
                      tabIndex={-1}
                      aria-label="Close dropdown"
                    />
                    <div className="absolute right-0 top-full mt-1 z-50 min-w-[160px] rounded-lg border border-border bg-card shadow-lg py-1">
                      {APP_CONFIG_FILES[activeApp].map((f) => (
                        <button
                          key={f.key}
                          type="button"
                          onClick={() => {
                            setConfigEditorOpen(f.key);
                            setConfigDropdownOpen(false);
                          }}
                          className="w-full text-left px-3 py-2 text-xs text-foreground hover:bg-muted/50 transition-colors flex items-center gap-2"
                        >
                          <FileCode2 className="w-3 h-3 text-muted-foreground" />
                          {f.label}
                          <span className="text-[10px] text-muted-foreground/50 ml-auto font-mono">{f.path}</span>
                        </button>
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
        {activeApp === "opencode" && (
          <div className="pt-1">
            <OpenCodeQuickLinks />
          </div>
        )}
      </div>

      {/* Floating Side Tools */}
      <BehaviorStrip
        appId={activeApp}
        appColor={appColor}
        currentProvider={providers.currentId ? providers.providers[providers.currentId] : undefined}
      />

      {/* Provider List */}
      <div className="flex-1 overflow-y-auto scrollbar-thin" ref={scrollContainerRef}>
        <div className="max-w-2xl mx-auto px-6 py-5">
          <AnimatePresence mode="wait">
            <motion.div
              key={activeApp}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.15 }}
            >
              {providers.loading ? (
                <div className="flex items-center justify-center py-20">
                  <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
                </div>
              ) : providers.sortedProviders.length === 0 ? (
                /* Empty state */
                <div className="space-y-5">
                  {/* Open AI / Google OAuth account section (even when no providers) */}
                  {activeApp === "codex" && (
                    <CodexAccountSection isOAuthActive={isOAuthActive} onAccountSwitched={handleAccountSwitched} />
                  )}
                  {activeApp === "gemini" && <GeminiAccountSection onAccountSwitched={handleAccountSwitched} />}

                  <div className="flex flex-col items-center justify-center py-12 text-center">
                    <div
                      className="w-14 h-14 rounded-2xl border border-border flex items-center justify-center mb-4"
                      style={{ backgroundColor: `${appColor}10` }}
                    >
                      <Package className="w-7 h-7" style={{ color: appColor }} />
                    </div>
                    <p className="text-sm font-medium text-foreground mb-1">{t("modelPage.emptyProvidersTitle")}</p>
                    <p className="text-xs text-muted-foreground max-w-sm">
                      {activeApp === "opencode"
                        ? t("modelPage.emptyProvidersOpenCode")
                        : t("modelPage.emptyProvidersHint")}
                    </p>
                  </div>

                  {/* Preset catalog (auto-expanded when no providers) — not shown for OpenCode */}
                  {activeApp !== "opencode" && (
                    <PresetCatalog
                      appId={activeApp}
                      appColor={appColor}
                      onAddPreset={handleAddPreset}
                      existingProviders={localProviders}
                    />
                  )}
                </div>
              ) : (
                /* Provider card list */
                <div className="space-y-3">
                  {/* Codex/Gemini account section */}
                  {activeApp === "codex" && (
                    <CodexAccountSection isOAuthActive={isOAuthActive} onAccountSwitched={handleAccountSwitched} />
                  )}
                  {activeApp === "gemini" && <GeminiAccountSection onAccountSwitched={handleAccountSwitched} />}

                  {activeApp !== "opencode" && providers.sortedProviders.length > 0 && (
                    <UnifiedProviderSwitcher
                      appColor={appColor}
                      providers={providers.sortedProviders}
                      currentId={providers.currentId}
                      saving={providers.saving}
                      onSwitch={providers.switchTo}
                    />
                  )}

                  {activeApp !== "opencode" && (
                    <ProviderHealthDashboardCard
                      dashboard={healthDashboard.dashboard}
                      loading={healthDashboard.loading}
                      refreshing={healthDashboard.refreshing}
                      onRefresh={healthDashboard.refresh}
                      appId={activeApp}
                      appColor={appColor}
                    />
                  )}

                  <div className="space-y-3" ref={listContainerRef}>
                    <AnimatePresence>
                      {localProviders
                        .filter(
                          (provider) =>
                            !(provider.id.startsWith("gemini_oauth_") || provider.id.startsWith("gemini_apikey_")),
                        )
                        .map((provider) => (
                          <ProviderCard
                            key={provider.id}
                            provider={provider}
                            isCurrent={provider.id === providers.currentId}
                            expanded={expandedId === provider.id}
                            appId={activeApp}
                            appColor={appColor}
                            dragId={provider.id}
                            onSwitch={() => providers.switchTo(provider.id)}
                            onToggleExpand={() => handleToggleExpand(provider.id)}
                            onUpdate={(entry) => providers.updateProvider(entry)}
                            onDelete={() => providers.deleteProvider(provider.id)}
                            onOpenWebsite={
                              provider.websiteUrl ? () => void openExternalUrl(provider.websiteUrl!) : undefined
                            }
                            onDragHandlePointerDown={(e) => handleDragStart(provider.id, e)}
                            readOnly={activeApp === "opencode"}
                          />
                        ))}
                    </AnimatePresence>
                  </div>

                  {/* Inline preset catalog at bottom — not shown for OpenCode */}
                  {activeApp !== "opencode" && (
                    <PresetCatalog
                      appId={activeApp}
                      appColor={appColor}
                      onAddPreset={handleAddPreset}
                      existingProviders={localProviders}
                    />
                  )}
                </div>
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </div>

      {/* Drag ghost — GPU-accelerated floating clone */}
      <div ref={ghostRef} className={DRAG_CSS.ghost} style={{ display: "none" }} />

      {/* Config file editor drawer */}
      {configEditorOpen &&
        (() => {
          const file =
            APP_CONFIG_FILES[activeApp].find((f) => f.key === configEditorOpen) || APP_CONFIG_FILES[activeApp][0];
          return (
            <>
              <button
                type="button"
                aria-label={t("common.close")}
                className="fixed inset-0 z-40 cursor-default border-0 bg-transparent p-0"
                onClick={() => setConfigEditorOpen(null)}
              />
              <ConfigFileEditor
                fileKey={configEditorOpen}
                title={file.label}
                filePath={file.path}
                onClose={() => setConfigEditorOpen(null)}
              />
            </>
          );
        })()}
    </div>
  );
}

function UnifiedProviderSwitcher({
  appColor,
  providers,
  currentId,
  saving,
  onSwitch,
}: {
  appColor: string;
  providers: ProviderEntry[];
  currentId: string | null;
  saving: boolean;
  onSwitch: (providerId: string) => void | Promise<void>;
}) {
  const currentProvider = currentId ? providers.find((provider) => provider.id === currentId) : null;

  return (
    <div className="rounded-2xl border border-border/70 bg-card/60 backdrop-blur-sm px-4 py-3">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2 min-w-0">
          <span
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-xl border"
            style={{ borderColor: `${appColor}40`, backgroundColor: `${appColor}14`, color: appColor }}
          >
            {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
          </span>
          <div className="min-w-0">
            <div className="text-xs font-semibold text-foreground">统一供应商切换</div>
            <div className="truncate text-[11px] text-muted-foreground">
              {currentProvider ? `当前：${currentProvider.name}` : "当前：OAuth / 账户授权"}
            </div>
          </div>
        </div>

        <select
          value={currentId ?? ""}
          disabled={saving}
          onChange={(event) => {
            const next = event.target.value;
            if (next) void onSwitch(next);
          }}
          className="h-8 min-w-0 rounded-xl border border-border bg-background/80 px-3 text-xs text-foreground outline-none transition-colors hover:bg-muted/40 focus:border-primary disabled:cursor-not-allowed disabled:opacity-60 sm:w-64"
          aria-label="切换模型供应商"
        >
          {!currentId && <option value="">OAuth / 账户授权</option>}
          {providers.map((provider) => (
            <option key={provider.id} value={provider.id}>
              {provider.name}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
