import { motion } from "framer-motion";
import { useCallback, useEffect, useReducer, useState } from "react";
import { useTranslation } from "react-i18next";
import { AiProviderSection } from "../features/models/components/settings/AiProviderSection";
import { S3SyncSection } from "../features/s3/components/S3SyncSection";
import { DevModeBanner } from "../features/settings/components/DevModeBanner";
import { AboutSection } from "../features/settings/sections/AboutSection";
import { AcpSection } from "../features/settings/sections/AcpSection";
import { AgentConnectionsSection } from "../features/settings/sections/AgentConnectionsSection";
import { AppearanceSection } from "../features/settings/sections/AppearanceSection";
import {
  BackgroundRunSection,
  onBackgroundRunChanged,
  readBackgroundRun,
  writeBackgroundRun,
} from "../features/settings/sections/BackgroundRunSection";
import { GitHubMirrorSection } from "../features/settings/sections/GitHubMirrorSection";
import { LanguageSection } from "../features/settings/sections/LanguageSection";
import { ProxySection } from "../features/settings/sections/ProxySection";
import { StorageSection } from "../features/settings/sections/StorageSection";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { useAiConfig } from "../hooks/useAiConfig";
import { useAutoSaveConfig } from "../hooks/useAutoSaveConfig";
import { setLanguage } from "../i18n";
import { applyBackgroundStyle, type BackgroundStyle, readBackgroundStyle } from "../lib/backgroundStyle";
import { tauriInvoke } from "../lib/ipc";
import { toast } from "../lib/toast";
import type { SettingsFocusTarget } from "../lib/utils";
import type { AiConfig, GitHubMirrorConfig, ProxyConfig, StorageOverview } from "../types";
import {
  agentReducer,
  FORCE_DELETE_SLOW_HINT_MS,
  FORCE_DELETE_UI_TIMEOUT_MS,
  type ForceDeleteTarget,
  initialMirrorConfig,
  initialProxyConfig,
  isSameAiConfig,
  isSameMirrorConfig,
  isSameProxyConfig,
} from "./settings/settingsReducers";
import { SETTINGS_FOCUS_TO_SECTION_ID, SettingsSidebarNav } from "./settings/SettingsSidebarNav";

// ── Component ───────────────────────────────────────────────────────────────

export function Settings({
  onCheckUpdate,
  isCheckingUpdate,
}: {
  onCheckUpdate?: () => Promise<{ found: boolean; version?: string; error?: boolean }>;
  isCheckingUpdate?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const [currentLang, setCurrentLang] = useState(i18n.language);
  const [backgroundStyle, setBackgroundStyle] = useState<BackgroundStyle>(() => readBackgroundStyle());
  const [backgroundRun, setBackgroundRun] = useState(() => readBackgroundRun());
  const {
    profiles,
    loading: profilesLoading,
    toggleProfile,
    addCustomProfile,
    removeCustomProfile,
  } = useAgentProfiles();

  useEffect(() => onBackgroundRunChanged(setBackgroundRun), []);

  // Proxy auto-save
  const [proxyExpanded, setProxyExpanded] = useState(false);
  const proxyAutoSave = useAutoSaveConfig<ProxyConfig>({
    load: useCallback(() => tauriInvoke("get_proxy_config"), []),
    fallback: initialProxyConfig,
    save: useCallback((config: ProxyConfig) => tauriInvoke("save_proxy_config", { config }), []),
    isEqual: isSameProxyConfig,
    onSaveError: useCallback(
      (e: unknown) => {
        if (import.meta.env.DEV) console.error("Failed to save proxy config:", e);
        toast.error(t("settings.saveProxyFailed"));
      },
      [t],
    ),
  });

  // Mirror auto-save
  const [mirrorExpanded, setMirrorExpanded] = useState(false);
  const mirrorAutoSave = useAutoSaveConfig<GitHubMirrorConfig>({
    load: useCallback(() => tauriInvoke("get_github_mirror_config"), []),
    fallback: initialMirrorConfig,
    save: useCallback((config: GitHubMirrorConfig) => tauriInvoke("save_github_mirror_config", { config }), []),
    isEqual: isSameMirrorConfig,
    onSaveError: useCallback(
      (e: unknown) => {
        if (import.meta.env.DEV) console.error("Failed to save mirror config:", e);
        toast.error(t("settings.saveMirrorFailed"));
      },
      [t],
    ),
  });

  // AI auto-save (config sourced from useAiConfig, which owns its own load/cache).
  // `load` never resolves on its own — `hydrate()` below is the sole source of the
  // initial/re-synced config, matching the previous LOAD-on-aiConfig-change effect
  // (aiAutoSave.loaded stays false until useAiConfig's own load actually finishes).
  const { config: aiConfig, loading: aiLoading, saveConfig: saveAiConfig, testConnection } = useAiConfig();
  const [aiExpanded, setAiExpanded] = useState(false);
  const [aiTesting, setAiTesting] = useState(false);
  const [aiTestResult, setAiTestResult] = useState<"success" | "error" | null>(null);
  const [aiTestLatency, setAiTestLatency] = useState<number | null>(null);
  const neverResolves = useCallback(() => new Promise<AiConfig>(() => {}), []);
  const aiAutoSave = useAutoSaveConfig<AiConfig>({
    load: neverResolves,
    fallback: aiConfig,
    save: saveAiConfig,
    isEqual: isSameAiConfig,
    skip: aiTesting,
    onSaveError: useCallback(() => toast.error(t("settings.saveAiFailed")), [t]),
  });

  // Re-sync from useAiConfig's own load (mirrors the previous LOAD-on-aiConfig-change effect).
  // `aiAutoSave.hydrate` has a stable identity, so this only re-runs when aiConfig/aiLoading change.
  useEffect(() => {
    if (!aiLoading) {
      aiAutoSave.hydrate(aiConfig);
    }
  }, [aiConfig, aiLoading, aiAutoSave.hydrate]);

  // Agent connections reducer
  const [agentState, dispatchAgent] = useReducer(agentReducer, {
    expandedAgentId: null,
    linkedSkills: {},
  });

  const [storageOverview, setStorageOverview] = useState<StorageOverview | null>(null);
  const [fetchingStorage, setFetchingStorage] = useState(false);
  const [cleaningCaches, setCleaningCaches] = useState(false);
  const [cleaningBroken, setCleaningBroken] = useState(false);
  const [forceDeletingTarget, setForceDeletingTarget] = useState<ForceDeleteTarget | null>(null);
  const [slowForceDeletingTarget, setSlowForceDeletingTarget] = useState<ForceDeleteTarget | null>(null);
  const [ghInstalled, setGhInstalled] = useState<boolean | null>(null);

  const notifySkillsRefresh = useCallback(() => {
    window.dispatchEvent(new Event("skillstar:refresh-skills"));
  }, []);

  // ── Storage effects ───────────────────────────────────────────────────────

  const fetchStorageOverview = useCallback(async () => {
    setFetchingStorage(true);
    try {
      const storageOverview = await tauriInvoke("get_storage_overview");
      setStorageOverview(storageOverview);
    } catch (e) {
      if (import.meta.env.DEV) console.error("Failed to fetch storage overview:", e);
    } finally {
      setFetchingStorage(false);
    }
  }, []);

  useEffect(() => {
    fetchStorageOverview();
  }, [fetchStorageOverview]);

  useEffect(() => {
    tauriInvoke("check_gh_installed")
      .then(setGhInstalled)
      .catch(() => setGhInstalled(false));
  }, []);

  const focusSettingsSection = useCallback(
    (target: SettingsFocusTarget) => {
      if (target === "ai-provider" && !aiExpanded) {
        setAiExpanded(true);
      }

      const sectionId = SETTINGS_FOCUS_TO_SECTION_ID[target];

      requestAnimationFrame(() => {
        setTimeout(() => {
          const scrollRoot = document.getElementById("settings-scroll-container");
          const section = document.getElementById(sectionId);
          if (!scrollRoot || !section) return;

          const rootRect = scrollRoot.getBoundingClientRect();
          const sectionRect = section.getBoundingClientRect();
          const offset = 12;
          const targetTop = scrollRoot.scrollTop + (sectionRect.top - rootRect.top) - offset;
          scrollRoot.scrollTo({ top: Math.max(0, targetTop), behavior: "smooth" });
        }, 100);
      });
    },
    [aiExpanded],
  );

  // ── Settings section focus from navigation intents ───────────────────────

  useEffect(() => {
    const applyStoredFocus = () => {
      try {
        const focus = localStorage.getItem("skillstar:settings-focus");
        if (focus === "ai-provider" || focus === "acp" || focus === "storage") {
          localStorage.removeItem("skillstar:settings-focus");
          focusSettingsSection(focus);
        }
      } catch {
        // ignore localStorage access errors
      }
    };

    const handleFocusEvent = (event: Event) => {
      const target = (event as CustomEvent<{ target?: SettingsFocusTarget }>).detail?.target;
      if (target === "ai-provider" || target === "acp" || target === "storage") {
        focusSettingsSection(target);
      }
    };

    applyStoredFocus();
    window.addEventListener("skillstar:settings-focus", handleFocusEvent as EventListener);
    return () => window.removeEventListener("skillstar:settings-focus", handleFocusEvent as EventListener);
  }, [focusSettingsSection]);

  // ── Agent handlers ─────────────────────────────────────────────────────────

  const handleToggle = useCallback(
    async (profile: (typeof profiles)[0]) => {
      try {
        await toggleProfile(profile.id);
      } catch (e) {
        if (import.meta.env.DEV) console.error("Toggle failed:", e);
        toast.error(t("settings.toggleFailed"));
      }
    },
    [toggleProfile, t],
  );

  const toggleExpand = useCallback(
    async (agentId: string) => {
      if (agentState.expandedAgentId === agentId) {
        dispatchAgent({ type: "SET_EXPANDED_AGENT", agentId: null });
        return;
      }
      dispatchAgent({ type: "SET_EXPANDED_AGENT", agentId });
      try {
        const skills = await tauriInvoke("list_linked_skills", { agentId });
        dispatchAgent({ type: "SET_LINKED_SKILLS", agentId, skills });
      } catch (e) {
        if (import.meta.env.DEV) console.error("Failed to list linked skills:", e);
        toast.error(t("settings.listLinkedFailed"));
      }
    },
    [agentState.expandedAgentId, t],
  );

  const handleUnlinkSingle = useCallback(
    async (skillName: string, agentId: string) => {
      try {
        await tauriInvoke("unlink_skill_from_agent", { skillName, agentId });
        dispatchAgent({ type: "REMOVE_LINKED_SKILL", agentId, skillName });
        notifySkillsRefresh();
      } catch (e) {
        if (import.meta.env.DEV) console.error("Unlink failed:", e);
        toast.error(t("settings.unlinkFailed"));
      }
    },
    [t, notifySkillsRefresh],
  );

  // ── Language & appearance handlers ───────────────────────────────────────

  const handleLanguageChange = useCallback((lang: string) => {
    setLanguage(lang);
    setCurrentLang(lang);
    tauriInvoke("update_tray_language", { lang }).catch(() => {});
  }, []);

  const handleBackgroundStyleChange = useCallback((style: BackgroundStyle) => {
    setBackgroundStyle(style);
    applyBackgroundStyle(style);
  }, []);

  const handleBackgroundRunToggle = useCallback(async (enabled: boolean) => {
    writeBackgroundRun(enabled);
    try {
      if (enabled) {
        await tauriInvoke("set_patrol_enabled", { enabled: true });
      } else {
        await tauriInvoke("stop_patrol");
      }
    } catch (e) {
      writeBackgroundRun(!enabled);
      if (import.meta.env.DEV) console.error("Update patrol background run failed:", e);
    }
  }, []);

  // ── AI handlers ───────────────────────────────────────────────────────────

  const handleAiTestConnection = useCallback(async () => {
    setAiTesting(true);
    setAiTestResult(null);
    setAiTestLatency(null);
    try {
      await saveAiConfig(aiAutoSave.config);
      aiAutoSave.markSaved(aiAutoSave.config);
      const latency = await testConnection();
      setAiTesting(false);
      setAiTestResult("success");
      setAiTestLatency(latency);
      setTimeout(() => {
        setAiTestResult(null);
        setAiTestLatency(null);
      }, 3000);
    } catch (e) {
      setAiTesting(false);
      setAiTestResult("error");
      toast.error(t("settings.connectionFailed", { error: e }));
      setTimeout(() => {
        setAiTestResult(null);
        setAiTestLatency(null);
      }, 5000);
    }
  }, [aiAutoSave.config, aiAutoSave.markSaved, saveAiConfig, testConnection, t]);

  const handleAiEnabledChange = useCallback(
    (enabled: boolean) => {
      aiAutoSave.setConfig({ ...aiAutoSave.config, enabled });
    },
    [aiAutoSave.config, aiAutoSave.setConfig],
  );

  // ── Storage handlers ───────────────────────────────────────────────────────

  const handleCleanAllCaches = useCallback(async () => {
    setCleaningCaches(true);
    try {
      const [result] = await Promise.all([
        tauriInvoke("clear_all_caches"),
        new Promise((resolve) => setTimeout(resolve, 600)),
      ]);

      try {
        localStorage.removeItem("publisher-avatar-source-v1");
        localStorage.removeItem("skillstar_skipped_version");
        localStorage.removeItem("skillstar_last_check");
      } catch {
        /* ignore */
      }

      const total = result.repos_removed + result.history_cleared;
      if (total > 0) {
        toast.success(t("settings.cacheCleanDone", { count: total }));
      } else {
        toast.info(t("settings.cacheEmpty"));
      }
      await fetchStorageOverview();
    } catch (e) {
      if (import.meta.env.DEV) console.error("Cache clean failed:", e);
      toast.error(t("settings.cacheCleanFailed"));
    } finally {
      setCleaningCaches(false);
    }
  }, [fetchStorageOverview, t]);

  const handleForceDelete = useCallback(
    async (target: ForceDeleteTarget) => {
      setForceDeletingTarget(target);
      setSlowForceDeletingTarget(null);

      const slowHintTimer = window.setTimeout(() => {
        setSlowForceDeletingTarget((current) => current ?? target);
      }, FORCE_DELETE_SLOW_HINT_MS);

      const deletePromise =
        target === "hub"
          ? tauriInvoke("force_delete_installed_skills")
          : target === "cache"
            ? tauriInvoke("force_delete_repo_caches")
            : tauriInvoke("force_delete_app_config");

      const targetLabel =
        target === "hub"
          ? t("settings.storageHub")
          : target === "cache"
            ? t("settings.repoCache")
            : t("settings.storageConfig");

      const reportDeleteResult = (removed: number) => {
        if (removed > 0) {
          if (target === "hub") {
            toast.success(t("settings.forceDeleteHubDone", { count: removed }));
          } else if (target === "cache") {
            toast.success(t("settings.forceDeleteCacheDone", { count: removed }));
          } else {
            toast.success(t("settings.forceDeleteConfigDone", { count: removed }));
          }
        } else if (target === "hub") {
          toast.info(t("settings.forceDeleteHubEmpty"));
        } else if (target === "cache") {
          toast.info(t("settings.forceDeleteCacheEmpty"));
        } else {
          toast.info(t("settings.forceDeleteConfigEmpty"));
        }

        if (target === "hub") {
          notifySkillsRefresh();
        }
      };

      const timeoutSymbol = Symbol("force-delete-timeout");
      let timeoutTimer = 0;
      try {
        const raced = await Promise.race<number | typeof timeoutSymbol>([
          deletePromise,
          new Promise<typeof timeoutSymbol>((resolve) => {
            timeoutTimer = window.setTimeout(() => resolve(timeoutSymbol), FORCE_DELETE_UI_TIMEOUT_MS);
          }),
        ]);

        if (raced === timeoutSymbol) {
          toast.warning(
            t("settings.forceDeleteTimeoutHint", {
              target: targetLabel,
            }),
          );
          setForceDeletingTarget(null);
          setSlowForceDeletingTarget(null);

          void deletePromise
            .then((removed) => {
              reportDeleteResult(removed);
              toast.info(
                t("settings.forceDeleteBackgroundDone", {
                  target: targetLabel,
                }),
              );
            })
            .catch((e) => {
              if (import.meta.env.DEV) console.error("Background force delete failed:", e);
              toast.error(
                t("settings.forceDeleteBackgroundFailed", {
                  target: targetLabel,
                }),
              );
            })
            .finally(() => {
              void fetchStorageOverview();
            });

          return;
        }

        reportDeleteResult(raced);
        void fetchStorageOverview();
      } catch (e) {
        if (import.meta.env.DEV) console.error("Force delete failed:", e);
        toast.error(t("settings.forceDeleteFailed"));
      } finally {
        if (timeoutTimer) {
          window.clearTimeout(timeoutTimer);
        }
        window.clearTimeout(slowHintTimer);
        setForceDeletingTarget((current) => (current === target ? null : current));
        setSlowForceDeletingTarget((current) => (current === target ? null : current));
      }
    },
    [fetchStorageOverview, notifySkillsRefresh, t],
  );

  const handleCleanBroken = useCallback(async () => {
    setCleaningBroken(true);
    try {
      const [fixed] = await Promise.all([
        tauriInvoke("clean_broken_skills"),
        new Promise((resolve) => setTimeout(resolve, 400)),
      ]);
      if (fixed > 0) {
        toast.success(t("settings.repairDone", { count: fixed }));
      } else {
        toast.info(t("settings.repairNone"));
      }
      notifySkillsRefresh();
      await fetchStorageOverview();
    } catch (e) {
      if (import.meta.env.DEV) console.error("Clean broken skills failed:", e);
      toast.error(t("settings.forceDeleteFailed"));
    } finally {
      setCleaningBroken(false);
    }
  }, [fetchStorageOverview, notifySkillsRefresh, t]);

  const formatBytes = useCallback((bytes: number) => {
    if (bytes === 0) return "0 B";
    const unitBase = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const sizeIndex = Math.floor(Math.log(bytes) / Math.log(unitBase));
    return `${parseFloat((bytes / unitBase ** sizeIndex).toFixed(2))} ${sizes[sizeIndex]}`;
  }, []);

  // ── Proxy / mirror / AI config change + expand-toggle handlers ─────────────
  // (setConfig/expanded setters already match the child sections' expected
  // signatures, so no extra wrapping is needed.)

  const handleProxyConfigChange = proxyAutoSave.setConfig;
  const handleMirrorConfigChange = mirrorAutoSave.setConfig;
  const handleAiConfigChange = aiAutoSave.setConfig;

  const handleToggleProxyExpanded = useCallback(() => setProxyExpanded((prev) => !prev), []);
  const handleToggleMirrorExpanded = useCallback(() => setMirrorExpanded((prev) => !prev), []);
  const handleToggleAiExpanded = useCallback(() => setAiExpanded((prev) => !prev), []);

  const handleForceDeleteHub = useCallback(() => handleForceDelete("hub"), [handleForceDelete]);
  const handleForceDeleteCache = useCallback(() => handleForceDelete("cache"), [handleForceDelete]);

  return (
    <div className="flex-1 min-h-0 min-w-0 flex flex-col overflow-hidden bg-background">
      <div
        data-tauri-drag-region
        className="h-12 flex items-center px-6 border-b border-border/40 bg-card/40 backdrop-blur-xl z-10 shrink-0"
      >
        <h1 className="text-sm font-semibold leading-none">{t("settings.title")}</h1>
      </div>

      <motion.main
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, ease: "easeOut" }}
        className="flex flex-col flex-1 min-h-0 overflow-hidden relative"
      >
        {/* Content */}
        <div id="settings-scroll-container" className="flex-1 min-h-0 overflow-y-auto p-6 relative">
          <div className="flex justify-center w-full min-h-full max-w-[1400px] mx-auto">
            {/* Left elastic gutter (Centers sidebar between edge and content) */}
            <div className="hidden lg:flex flex-1 justify-center items-start relative px-4">
              <div className="sticky top-1/2 -translate-y-1/2 h-max w-full flex justify-center pt-8">
                <SettingsSidebarNav />
              </div>
            </div>

            {/* Main content block */}
            <div className="w-full max-w-[720px] shrink-0 space-y-8 pb-12 relative">
              {/* Windows Developer Mode guidance banner */}
              <DevModeBanner />

              <section id="settings-agents" className="scroll-mt-3">
                <AgentConnectionsSection
                  profiles={profiles}
                  profilesLoading={profilesLoading}
                  expandedAgentId={agentState.expandedAgentId}
                  linkedSkills={agentState.linkedSkills}
                  onToggleProfile={handleToggle}
                  onToggleExpand={toggleExpand}
                  onUnlinkSkill={handleUnlinkSingle}
                  onAddCustomProfile={addCustomProfile}
                  onRemoveCustomProfile={removeCustomProfile}
                />
              </section>

              <section id="settings-proxy" className="scroll-mt-3">
                <ProxySection
                  proxyConfig={proxyAutoSave.config}
                  ready={proxyAutoSave.loaded}
                  proxyExpanded={proxyExpanded}
                  proxySaving={proxyAutoSave.saving}
                  proxySaved={proxyAutoSave.showSaved}
                  onToggleExpanded={handleToggleProxyExpanded}
                  onConfigChange={handleProxyConfigChange}
                />
              </section>

              <section id="settings-mirror" className="scroll-mt-3">
                <GitHubMirrorSection
                  mirrorConfig={mirrorAutoSave.config}
                  ready={mirrorAutoSave.loaded}
                  mirrorExpanded={mirrorExpanded}
                  mirrorSaving={mirrorAutoSave.saving}
                  mirrorSaved={mirrorAutoSave.showSaved}
                  onToggleExpanded={handleToggleMirrorExpanded}
                  onConfigChange={handleMirrorConfigChange}
                />
              </section>

              <section id="settings-s3" className="scroll-mt-3">
                <S3SyncSection />
              </section>

              <section id="settings-ai" className="scroll-mt-3">
                <AiProviderSection
                  localAiConfig={aiAutoSave.config}
                  ready={aiAutoSave.loaded}
                  aiExpanded={aiExpanded}
                  aiSaving={aiAutoSave.saving}
                  aiSaved={aiAutoSave.showSaved}
                  aiTesting={aiTesting}
                  aiTestResult={aiTestResult}
                  aiTestLatency={aiTestLatency}
                  onToggleExpanded={handleToggleAiExpanded}
                  onEnabledChange={handleAiEnabledChange}
                  onConfigChange={handleAiConfigChange}
                  onTestConnection={handleAiTestConnection}
                />
              </section>

              <section id="settings-acp" className="scroll-mt-3">
                <AcpSection />
              </section>

              <section id="settings-background" className="scroll-mt-3">
                <BackgroundRunSection enabled={backgroundRun} onToggle={handleBackgroundRunToggle} />
              </section>

              <section id="settings-appearance" className="scroll-mt-3">
                <AppearanceSection
                  backgroundStyle={backgroundStyle}
                  onBackgroundStyleChange={handleBackgroundStyleChange}
                />
              </section>

              <section id="settings-language" className="scroll-mt-3">
                <LanguageSection currentLang={currentLang} onLanguageChange={handleLanguageChange} />
              </section>

              <section id="settings-storage" className="scroll-mt-3">
                <StorageSection
                  overview={storageOverview}
                  loading={fetchingStorage}
                  cleaning={cleaningCaches}
                  cleaningBroken={cleaningBroken}
                  forceDeletingTarget={forceDeletingTarget}
                  slowForceDeletingTarget={slowForceDeletingTarget}
                  formatBytes={formatBytes}
                  onCleanAll={handleCleanAllCaches}
                  onForceDeleteHub={handleForceDeleteHub}
                  onForceDeleteCache={handleForceDeleteCache}
                  onCleanBroken={handleCleanBroken}
                />
              </section>

              <section id="settings-about" className="scroll-mt-3">
                <AboutSection
                  ghInstalled={ghInstalled}
                  onCheckUpdate={onCheckUpdate}
                  isCheckingUpdate={isCheckingUpdate}
                />
              </section>
            </div>

            {/* Right elastic gutter to balance flex layout symmetrically */}
            <div className="hidden lg:block flex-1 border-transparent"></div>
          </div>
        </div>
      </motion.main>
    </div>
  );
}
