import { open } from "@tauri-apps/plugin-shell";
import { AnimatePresence, motion } from "framer-motion";
import { FileCode2, Loader2, Package } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { DRAG_CSS, useDragReorder } from "../hooks/useDragReorder";
import { type ProviderEntry, useModelProviders, useOpenCodeNativeProviders } from "../hooks/useModelProviders";
import { AppCapsuleSwitcher, type ModelAppId } from "./AppCapsuleSwitcher";
import { BehaviorStrip } from "./BehaviorStrip";
import { CodexAccountSection } from "./CodexAccountSection";
import { GeminiAccountSection } from "./GeminiAccountSection";
import { ConfigFileEditor, type ConfigFileKey } from "./ConfigFileEditor";
import { PresetCatalog } from "./PresetCatalog";
import { ProviderCard } from "./ProviderCard";
import { AgentIcon } from "./shared/ProviderIcon";

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
  const [activeApp, setActiveApp] = useState<ModelAppId>(() => {
    try {
      const saved = localStorage.getItem("models-active-app");
      if (saved === "claude" || saved === "codex" || saved === "opencode") return saved;
    } catch {
      /* ignore */
    }
    return "claude";
  });

  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [configEditorOpen, setConfigEditorOpen] = useState<ConfigFileKey | null>(null);
  const [configDropdownOpen, setConfigDropdownOpen] = useState(false);

  const genericProviders = useModelProviders(activeApp);
  const opencodeProviders = useOpenCodeNativeProviders();
  const providers = activeApp === "opencode" ? opencodeProviders : genericProviders;
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
      <div className="shrink-0 px-6 py-4 border-b border-border flex items-center justify-between">
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-3">
            <div
              className="w-10 h-10 rounded-xl border border-border flex items-center justify-center transition-colors duration-300"
              style={{ backgroundColor: `${appColor}15` }}
            >
              <AgentIcon appId={activeApp} color={appColor} size="w-5 h-5" className="transition-all duration-300" />
            </div>
            <h1 className="text-lg font-semibold text-foreground">模型</h1>
          </div>

          <AppCapsuleSwitcher value={activeApp} onChange={setActiveApp} />
        </div>

        <div className="flex items-center gap-1.5">
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
                配置文件
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
                  {activeApp === "gemini" && (
                    <GeminiAccountSection onAccountSwitched={handleAccountSwitched} />
                  )}

                  <div className="flex flex-col items-center justify-center py-12 text-center">
                    <div
                      className="w-14 h-14 rounded-2xl border border-border flex items-center justify-center mb-4"
                      style={{ backgroundColor: `${appColor}10` }}
                    >
                      <Package className="w-7 h-7" style={{ color: appColor }} />
                    </div>
                    <p className="text-sm font-medium text-foreground mb-1">暂无供应商配置</p>
                    <p className="text-xs text-muted-foreground">
                      {activeApp === "opencode"
                        ? "请通过配置文件或 OpenCode CLI 管理供应商授权"
                        : "从下方预设中选择，一键添加"}
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
                  {activeApp === "gemini" && (
                    <GeminiAccountSection onAccountSwitched={handleAccountSwitched} />
                  )}

                  <div className="space-y-3" ref={listContainerRef}>
                    <AnimatePresence>
                      {localProviders
                        .filter(
                          (provider) =>
                            !(
                              provider.id.startsWith("gemini_oauth_") ||
                              provider.id.startsWith("gemini_apikey_")
                            )
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
                          onOpenWebsite={provider.websiteUrl ? () => open(provider.websiteUrl!) : undefined}
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
              {/* Invisible backdrop to close when clicking outside */}
              <div className="fixed inset-0 z-40 cursor-default" onClick={() => setConfigEditorOpen(null)} />
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
