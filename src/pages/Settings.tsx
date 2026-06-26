import { motion } from "framer-motion";
import { useCallback, useEffect, useReducer, useState } from "react";
import { useTranslation } from "react-i18next";
import { DevModeBanner } from "../features/settings/components/DevModeBanner";
import { AboutSection } from "../features/settings/sections/AboutSection";
import { AcpSection } from "../features/settings/sections/AcpSection";
import { AgentConnectionsSection } from "../features/settings/sections/AgentConnectionsSection";
import { AiProviderSection } from "../features/settings/sections/AiProviderSection";
import { AppearanceSection } from "../features/settings/sections/AppearanceSection";
import {
  BackgroundRunSection,
  onBackgroundRunChanged,
  readBackgroundRun,
  writeBackgroundRun,
} from "../features/settings/sections/BackgroundRunSection";
import { FingerprintsSection } from "../features/settings/sections/FingerprintsSection";
import { GitHubMirrorSection } from "../features/settings/sections/GitHubMirrorSection";
import { LanguageSection } from "../features/settings/sections/LanguageSection";
import { ProxySection } from "../features/settings/sections/ProxySection";
import { S3SyncSection } from "../features/settings/sections/S3SyncSection";
import { StorageSection } from "../features/settings/sections/StorageSection";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { useAiConfig } from "../hooks/useAiConfig";
import { setLanguage } from "../i18n";
import { applyBackgroundStyle, type BackgroundStyle, readBackgroundStyle } from "../lib/backgroundStyle";
import { tauriInvoke } from "../lib/ipc";
import { toast } from "../lib/toast";
import type { SettingsFocusTarget } from "../lib/utils";
import type { AiConfig, GitHubMirrorConfig, ProxyConfig, StorageOverview } from "../types";
import {
  agentReducer,
  aiReducer,
  AUTO_SAVE_DELAY_MS,
  FORCE_DELETE_SLOW_HINT_MS,
  FORCE_DELETE_UI_TIMEOUT_MS,
  type ForceDeleteTarget,
  initialMirrorConfig,
  initialProxyConfig,
  isSameAiConfig,
  isSameMirrorConfig,
  isSameProxyConfig,
  mirrorReducer,
  proxyReducer,
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

  // Proxy reducer
  const [proxyState, dispatchProxy] = useReducer(proxyReducer, {
    config: initialProxyConfig,
    savedConfig: initialProxyConfig,
    saving: false,
    savedIndicator: false,
    expanded: false,
    loaded: false,
  });

  // Mirror reducer
  const [mirrorState, dispatchMirror] = useReducer(mirrorReducer, {
    config: initialMirrorConfig,
    savedConfig: initialMirrorConfig,
    saving: false,
    savedIndicator: false,
    expanded: false,
    loaded: false,
  });

  // AI reducer
  const { config: aiConfig, loading: aiLoading, saveConfig: saveAiConfig, testConnection } = useAiConfig();
  const [aiState, dispatchAi] = useReducer(aiReducer, {
    config: aiConfig,
    savedConfig: aiConfig,
    saving: false,
    savedIndicator: false,
    expanded: false,
    testing: false,
    testResult: null,
    testLatency: null,
    loaded: false,
  });

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

  // ── Proxy effects ─────────────────────────────────────────────────────────

  useEffect(() => {
    dispatchProxy({ type: "START_LOAD" });
    tauriInvoke("get_proxy_config")
      .then((config) => dispatchProxy({ type: "LOAD", config }))
      .catch(() => dispatchProxy({ type: "LOAD", config: initialProxyConfig }))
      .finally(() => dispatchProxy({ type: "FINISH_SAVE" }));
  }, []);

  useEffect(() => {
    if (!proxyState.loaded || proxyState.saving || isSameProxyConfig(proxyState.config, proxyState.savedConfig)) {
      return;
    }

    const previousConfig = proxyState.savedConfig;
    const timer = setTimeout(() => {
      dispatchProxy({ type: "START_SAVE" });
      tauriInvoke("save_proxy_config", { config: proxyState.config })
        .then(() => {
          dispatchProxy({ type: "MARK_SAVED_CONFIG", config: proxyState.config });
          dispatchProxy({ type: "MARK_SAVED_INDICATOR" });
          setTimeout(() => dispatchProxy({ type: "CLEAR_SAVED_INDICATOR" }), 2000);
        })
        .catch((e) => {
          if (import.meta.env.DEV) console.error("Failed to save proxy config:", e);
          dispatchProxy({ type: "REVERT", config: previousConfig });
          toast.error(t("settings.saveProxyFailed"));
        })
        .finally(() => dispatchProxy({ type: "FINISH_SAVE" }));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [proxyState.config, proxyState.loaded, proxyState.saving, proxyState.savedConfig, t]);

  // ── Mirror effects ────────────────────────────────────────────────────────

  useEffect(() => {
    dispatchMirror({ type: "START_LOAD" });
    tauriInvoke("get_github_mirror_config")
      .then((config) => dispatchMirror({ type: "LOAD", config }))
      .catch(() => dispatchMirror({ type: "LOAD", config: initialMirrorConfig }))
      .finally(() => dispatchMirror({ type: "FINISH_SAVE" }));
  }, []);

  useEffect(() => {
    if (!mirrorState.loaded || mirrorState.saving || isSameMirrorConfig(mirrorState.config, mirrorState.savedConfig)) {
      return;
    }

    const previousConfig = mirrorState.savedConfig;
    const timer = setTimeout(() => {
      dispatchMirror({ type: "START_SAVE" });
      tauriInvoke("save_github_mirror_config", { config: mirrorState.config })
        .then(() => {
          dispatchMirror({ type: "MARK_SAVED_CONFIG", config: mirrorState.config });
          dispatchMirror({ type: "MARK_SAVED_INDICATOR" });
          setTimeout(() => dispatchMirror({ type: "CLEAR_SAVED_INDICATOR" }), 2000);
        })
        .catch((e) => {
          if (import.meta.env.DEV) console.error("Failed to save mirror config:", e);
          dispatchMirror({ type: "REVERT", config: previousConfig });
          toast.error(t("settings.saveMirrorFailed"));
        })
        .finally(() => dispatchMirror({ type: "FINISH_SAVE" }));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [mirrorState.config, mirrorState.loaded, mirrorState.saving, mirrorState.savedConfig, t]);

  // ── AI effects ────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!aiLoading) {
      dispatchAi({ type: "LOAD", config: aiConfig });
    }
  }, [aiConfig, aiLoading]);

  useEffect(() => {
    if (!aiState.loaded || aiState.saving || aiState.testing || isSameAiConfig(aiState.config, aiState.savedConfig)) {
      return;
    }

    const previousConfig = aiState.savedConfig;
    const timer = setTimeout(() => {
      dispatchAi({ type: "START_SAVE" });
      saveAiConfig(aiState.config)
        .then(() => {
          dispatchAi({ type: "MARK_SAVED_CONFIG", config: aiState.config });
          dispatchAi({ type: "MARK_SAVED_INDICATOR" });
          setTimeout(() => dispatchAi({ type: "CLEAR_SAVED_INDICATOR" }), 2000);
        })
        .catch(() => {
          dispatchAi({ type: "REVERT", config: previousConfig });
          toast.error(t("settings.saveAiFailed"));
        })
        .finally(() => dispatchAi({ type: "FINISH_SAVE" }));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [aiState.config, aiState.loaded, aiState.saving, aiState.testing, aiState.savedConfig, saveAiConfig, t]);

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
      if (target === "ai-provider" && !aiState.expanded) {
        dispatchAi({ type: "TOGGLE_EXPANDED" });
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
    [aiState.expanded],
  );

  // ── Settings section focus from navigation intents ───────────────────────

  useEffect(() => {
    const applyStoredFocus = () => {
      try {
        const focus = localStorage.getItem("skillstar:settings-focus");
        if (focus === "ai-provider" || focus === "storage") {
          localStorage.removeItem("skillstar:settings-focus");
          focusSettingsSection(focus);
        }
      } catch {
        // ignore localStorage access errors
      }
    };

    const handleFocusEvent = (event: Event) => {
      const target = (event as CustomEvent<{ target?: SettingsFocusTarget }>).detail?.target;
      if (target === "ai-provider" || target === "storage") {
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
    dispatchAi({ type: "START_TEST" });
    try {
      await saveAiConfig(aiState.config);
      dispatchAi({ type: "MARK_SAVED_CONFIG", config: aiState.config });
      const latency = await testConnection();
      dispatchAi({ type: "FINISH_TEST", result: "success", latency });
      setTimeout(() => dispatchAi({ type: "CLEAR_TEST_RESULT" }), 3000);
    } catch (e) {
      dispatchAi({ type: "FINISH_TEST", result: "error" });
      toast.error(t("settings.connectionFailed", { error: e }));
      setTimeout(() => dispatchAi({ type: "CLEAR_TEST_RESULT" }), 5000);
    }
  }, [aiState.config, saveAiConfig, testConnection, t]);

  const handleAiEnabledChange = useCallback((enabled: boolean) => {
    dispatchAi({ type: "SET_FIELD", field: "enabled", value: enabled });
  }, []);

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

      const total = result.repos_removed + result.history_cleared + result.translation_cleared;
      if (total > 0) {
        toast.success(t("settings.cacheCleanDone", { count: total }));
      } else {
        toast.info(t("settings.cacheEmpty"));
      }
      await fetchStorageOverview();
    } catch (e) {
      if (import.meta.env.DEV) console.error("Cache clean failed:", e);
      toast.error("Cleanup failed");
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

  // ── Proxy config change handler ────────────────────────────────────────────

  const handleProxyConfigChange = useCallback((next: ProxyConfig) => {
    dispatchProxy({ type: "SET_CONFIG", config: next });
  }, []);

  // ── Mirror config change handler ───────────────────────────────────────────

  const handleMirrorConfigChange = useCallback((next: GitHubMirrorConfig) => {
    dispatchMirror({ type: "SET_CONFIG", config: next });
  }, []);

  // ── AI config change handler ───────────────────────────────────────────────

  const handleAiConfigChange = useCallback((next: AiConfig) => {
    dispatchAi({ type: "SET_CONFIG", config: next });
  }, []);

  const handleToggleProxyExpanded = useCallback(() => {
    dispatchProxy({ type: "TOGGLE_EXPANDED" });
  }, []);

  const handleToggleMirrorExpanded = useCallback(() => {
    dispatchMirror({ type: "TOGGLE_EXPANDED" });
  }, []);

  const handleToggleAiExpanded = useCallback(() => {
    dispatchAi({ type: "TOGGLE_EXPANDED" });
  }, []);

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
                  proxyConfig={proxyState.config}
                  ready={proxyState.loaded}
                  proxyExpanded={proxyState.expanded}
                  proxySaving={proxyState.saving}
                  proxySaved={proxyState.savedIndicator}
                  onToggleExpanded={handleToggleProxyExpanded}
                  onConfigChange={handleProxyConfigChange}
                />
              </section>

              <section id="settings-mirror" className="scroll-mt-3">
                <GitHubMirrorSection
                  mirrorConfig={mirrorState.config}
                  ready={mirrorState.loaded}
                  mirrorExpanded={mirrorState.expanded}
                  mirrorSaving={mirrorState.saving}
                  mirrorSaved={mirrorState.savedIndicator}
                  onToggleExpanded={handleToggleMirrorExpanded}
                  onConfigChange={handleMirrorConfigChange}
                />
              </section>

              <section id="settings-s3" className="scroll-mt-3">
                <S3SyncSection />
              </section>

              <section id="settings-ai" className="scroll-mt-3">
                <AiProviderSection
                  localAiConfig={aiState.config}
                  ready={aiState.loaded}
                  aiExpanded={aiState.expanded}
                  aiSaving={aiState.saving}
                  aiSaved={aiState.savedIndicator}
                  aiTesting={aiState.testing}
                  aiTestResult={aiState.testResult}
                  aiTestLatency={aiState.testLatency}
                  onToggleExpanded={handleToggleAiExpanded}
                  onEnabledChange={handleAiEnabledChange}
                  onConfigChange={handleAiConfigChange}
                  onTestConnection={handleAiTestConnection}
                />
              </section>

              <section id="settings-acp" className="scroll-mt-3">
                <AcpSection />
              </section>

              <section id="settings-fingerprints" className="scroll-mt-3">
                <FingerprintsSection />
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
