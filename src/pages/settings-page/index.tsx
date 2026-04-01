import { motion } from "framer-motion";
import { useState, useReducer, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { toast } from "../../lib/toast";
import { useAgentProfiles } from "../../hooks/useAgentProfiles";
import { useAiConfig } from "../../hooks/useAiConfig";
import { setLanguage } from "../../i18n";
import {
  applyBackgroundStyle,
  readBackgroundStyle,
  type BackgroundStyle,
} from "../../lib/backgroundStyle";
import type {
  AiConfig,
  CacheCleanResult,
  MymemoryUsageStats,
  ProxyConfig,
  StorageOverview,
} from "../../types";
import { AgentConnectionsSection } from "./AgentConnectionsSection";
import { ProxySection } from "./ProxySection";
import { AiProviderSection } from "./AiProviderSection";
import { ShortTextServiceSection } from "./ShortTextServiceSection";
import {
  BackgroundRunSection,
  onBackgroundRunChanged,
  readBackgroundRun,
  writeBackgroundRun,
} from "./BackgroundRunSection";
import { AppearanceSection } from "./AppearanceSection";
import { LanguageSection } from "./LanguageSection";
import { StorageSection } from "./StorageSection";
import { AboutSection } from "./AboutSection";

type ForceDeleteTarget = "hub" | "cache" | "config";

const AUTO_SAVE_DELAY_MS = 600;

function isSameProxyConfig(a: ProxyConfig, b: ProxyConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.proxy_type === b.proxy_type &&
    a.host === b.host &&
    a.port === b.port &&
    a.username === b.username &&
    a.password === b.password &&
    a.bypass === b.bypass
  );
}

function isSameAiConfig(a: AiConfig, b: AiConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.api_format === b.api_format &&
    a.base_url === b.base_url &&
    a.api_key === b.api_key &&
    a.model === b.model &&
    a.target_language === b.target_language &&
    a.short_text_priority === b.short_text_priority &&
    a.context_window_k === b.context_window_k &&
    a.max_concurrent_requests === b.max_concurrent_requests &&
    a.chunk_char_limit === b.chunk_char_limit &&
    a.scan_max_response_tokens === b.scan_max_response_tokens
  );
}

// ── Reducers ─────────────────────────────────────────────────────────────────

type ProxyAction =
  | { type: "SET_FIELD"; field: keyof ProxyConfig; value: ProxyConfig[keyof ProxyConfig] }
  | { type: "LOAD"; config: ProxyConfig }
  | { type: "MARK_SAVED_CONFIG"; config: ProxyConfig }
  | { type: "START_SAVE" }
  | { type: "FINISH_SAVE" }
  | { type: "MARK_SAVED_INDICATOR" }
  | { type: "CLEAR_SAVED_INDICATOR" }
  | { type: "TOGGLE_EXPANDED" }
  | { type: "START_LOAD" }
  | { type: "REVERT"; config: ProxyConfig };

interface ProxyState {
  config: ProxyConfig;
  savedConfig: ProxyConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  loaded: boolean;
}

const initialProxyConfig: ProxyConfig = {
  enabled: false,
  proxy_type: "http",
  host: "",
  port: 7897,
  username: null,
  password: null,
  bypass: null,
};

function proxyReducer(state: ProxyState, action: ProxyAction): ProxyState {
  switch (action.type) {
    case "SET_FIELD":
      return { ...state, config: { ...state.config, [action.field]: action.value } };
    case "LOAD":
      return { ...state, config: action.config, savedConfig: action.config, loaded: true };
    case "MARK_SAVED_CONFIG":
      return { ...state, savedConfig: action.config };
    case "START_SAVE":
      return { ...state, saving: true };
    case "FINISH_SAVE":
      return { ...state, saving: false };
    case "MARK_SAVED_INDICATOR":
      return { ...state, savedIndicator: true };
    case "CLEAR_SAVED_INDICATOR":
      return { ...state, savedIndicator: false };
    case "TOGGLE_EXPANDED":
      return { ...state, expanded: !state.expanded };
    case "START_LOAD":
      return { ...state, loaded: false };
    case "REVERT":
      return { ...state, config: action.config, saving: false };
    default:
      return state;
  }
}

type AiAction =
  | { type: "SET_FIELD"; field: keyof AiConfig; value: AiConfig[keyof AiConfig] }
  | { type: "LOAD"; config: AiConfig }
  | { type: "MARK_SAVED_CONFIG"; config: AiConfig }
  | { type: "START_SAVE" }
  | { type: "FINISH_SAVE" }
  | { type: "MARK_SAVED_INDICATOR" }
  | { type: "CLEAR_SAVED_INDICATOR" }
  | { type: "TOGGLE_EXPANDED" }
  | { type: "START_TEST" }
  | { type: "FINISH_TEST"; result: "success" | "error" }
  | { type: "CLEAR_TEST_RESULT" }
  | { type: "TOGGLE_SHOW_API_KEY" }
  | { type: "REVERT"; config: AiConfig };

interface AiState {
  config: AiConfig;
  savedConfig: AiConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  testing: boolean;
  testResult: "success" | "error" | null;
  showApiKey: boolean;
  loaded: boolean;
}

function aiReducer(state: AiState, action: AiAction): AiState {
  switch (action.type) {
    case "SET_FIELD":
      return { ...state, config: { ...state.config, [action.field]: action.value } };
    case "LOAD":
      return { ...state, config: action.config, savedConfig: action.config, loaded: true };
    case "MARK_SAVED_CONFIG":
      return { ...state, savedConfig: action.config };
    case "START_SAVE":
      return { ...state, saving: true };
    case "FINISH_SAVE":
      return { ...state, saving: false };
    case "MARK_SAVED_INDICATOR":
      return { ...state, savedIndicator: true };
    case "CLEAR_SAVED_INDICATOR":
      return { ...state, savedIndicator: false };
    case "TOGGLE_EXPANDED":
      return { ...state, expanded: !state.expanded };
    case "START_TEST":
      return { ...state, testing: true, testResult: null };
    case "FINISH_TEST":
      return { ...state, testing: false, testResult: action.result };
    case "CLEAR_TEST_RESULT":
      return { ...state, testResult: null };
    case "TOGGLE_SHOW_API_KEY":
      return { ...state, showApiKey: !state.showApiKey };
    case "REVERT":
      return { ...state, config: action.config, saving: false };
    default:
      return state;
  }
}

type AgentAction =
  | { type: "SET_EXPANDED_AGENT"; agentId: string | null }
  | { type: "SET_LINKED_SKILLS"; agentId: string; skills: string[] }
  | { type: "REMOVE_LINKED_SKILL"; agentId: string; skillName: string }
  | { type: "SET_UNLINKING_ID"; id: string | null }
  | { type: "SET_CONFIRM_DISABLE_ID"; id: string | null };

interface AgentState {
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
  unlinkingId: string | null;
  confirmDisableId: string | null;
}

function agentReducer(state: AgentState, action: AgentAction): AgentState {
  switch (action.type) {
    case "SET_EXPANDED_AGENT":
      return { ...state, expandedAgentId: action.agentId };
    case "SET_LINKED_SKILLS":
      return { ...state, linkedSkills: { ...state.linkedSkills, [action.agentId]: action.skills } };
    case "REMOVE_LINKED_SKILL":
      return {
        ...state,
        linkedSkills: {
          ...state.linkedSkills,
          [action.agentId]: (state.linkedSkills[action.agentId] ?? []).filter((s) => s !== action.skillName),
        },
      };
    case "SET_UNLINKING_ID":
      return { ...state, unlinkingId: action.id };
    case "SET_CONFIRM_DISABLE_ID":
      return { ...state, confirmDisableId: action.id };
    default:
      return state;
  }
}

// ── Component ───────────────────────────────────────────────────────────────

export function Settings() {
  const { t, i18n } = useTranslation();
  const [currentLang, setCurrentLang] = useState(i18n.language);
  const [backgroundStyle, setBackgroundStyle] = useState<BackgroundStyle>(
    () => readBackgroundStyle()
  );
  const [backgroundRun, setBackgroundRun] = useState(() => readBackgroundRun());
  const { profiles, loading: profilesLoading, toggleProfile, unlinkAllFromAgent } = useAgentProfiles();

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
    showApiKey: false,
    loaded: false,
  });

  // Agent connections reducer
  const [agentState, dispatchAgent] = useReducer(agentReducer, {
    expandedAgentId: null,
    linkedSkills: {},
    unlinkingId: null,
    confirmDisableId: null,
  });

  const [storageOverview, setStorageOverview] = useState<StorageOverview | null>(null);
  const [mymemoryUsage, setMymemoryUsage] = useState<MymemoryUsageStats | null>(null);
  const [fetchingStorage, setFetchingStorage] = useState(false);
  const [cleaningCaches, setCleaningCaches] = useState(false);
  const [cleaningBroken, setCleaningBroken] = useState(false);
  const [forceDeletingTarget, setForceDeletingTarget] = useState<ForceDeleteTarget | null>(null);
  const [ghInstalled, setGhInstalled] = useState<boolean | null>(null);

  const notifySkillsRefresh = useCallback(() => {
    window.dispatchEvent(new Event("skillstar:refresh-skills"));
  }, []);

  // ── Proxy effects ─────────────────────────────────────────────────────────

  useEffect(() => {
    dispatchProxy({ type: "START_LOAD" });
    invoke<ProxyConfig>("get_proxy_config")
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
      invoke("save_proxy_config", { config: proxyState.config })
        .then(() => {
          dispatchProxy({ type: "MARK_SAVED_CONFIG", config: proxyState.config });
          dispatchProxy({ type: "MARK_SAVED_INDICATOR" });
          setTimeout(() => dispatchProxy({ type: "CLEAR_SAVED_INDICATOR" }), 2000);
        })
        .catch((e) => {
          console.error("Failed to save proxy config:", e);
          dispatchProxy({ type: "REVERT", config: previousConfig });
          toast.error(t("settings.saveProxyFailed"));
        })
        .finally(() => dispatchProxy({ type: "FINISH_SAVE" }));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [proxyState.config, proxyState.loaded, proxyState.saving, proxyState.savedConfig, t]);

  // ── AI effects ────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!aiLoading) {
      dispatchAi({ type: "LOAD", config: aiConfig });
    }
  }, [aiConfig, aiLoading]);

  useEffect(() => {
    if (
      !aiState.loaded ||
      aiState.saving ||
      aiState.testing ||
      isSameAiConfig(aiState.config, aiState.savedConfig)
    ) {
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
      const storageOverview = await invoke<StorageOverview>("get_storage_overview");
      setStorageOverview(storageOverview);
    } catch (e) {
      console.error("Failed to fetch storage overview:", e);
    } finally {
      setFetchingStorage(false);
    }
  }, []);

  const fetchMymemoryUsage = useCallback(async () => {
    try {
      const usage = await invoke<MymemoryUsageStats>("get_mymemory_usage_stats");
      setMymemoryUsage(usage);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    fetchStorageOverview();
  }, [fetchStorageOverview]);

  useEffect(() => {
    void fetchMymemoryUsage();
  }, [fetchMymemoryUsage]);

  useEffect(() => {
    if (!aiState.expanded) return;
    void fetchMymemoryUsage();
  }, [aiState.expanded, fetchMymemoryUsage]);

  useEffect(() => {
    invoke<boolean>("check_gh_installed").then(setGhInstalled).catch(() => setGhInstalled(false));
  }, []);

  // ── AI focus from localStorage ────────────────────────────────────────────

  useEffect(() => {
    try {
      const focus = localStorage.getItem("skillstar:settings-focus");
      if (focus === "ai-provider") {
        dispatchAi({ type: "TOGGLE_EXPANDED" });
        localStorage.removeItem("skillstar:settings-focus");
      }
    } catch {
      // ignore localStorage access errors
    }
  }, []);

  // ── Agent handlers ─────────────────────────────────────────────────────────

  const handleToggle = useCallback(
    async (profile: (typeof profiles)[0]) => {
      if (profile.enabled && profile.synced_count > 0) {
        dispatchAgent({ type: "SET_CONFIRM_DISABLE_ID", id: profile.id });
        return;
      }
      try {
        await toggleProfile(profile.id);
      } catch (e) {
        console.error("Toggle failed:", e);
        toast.error(t("settings.toggleFailed"));
      }
    },
    [profiles, toggleProfile, t]
  );

  const confirmDisable = useCallback(async () => {
    const id = agentState.confirmDisableId;
    if (!id) return;
    dispatchAgent({ type: "SET_UNLINKING_ID", id });
    try {
      await unlinkAllFromAgent(id);
      await toggleProfile(id);
    } catch (e) {
      console.error("Disable failed:", e);
      toast.error(t("settings.disableFailed"));
    } finally {
      dispatchAgent({ type: "SET_UNLINKING_ID", id: null });
      dispatchAgent({ type: "SET_CONFIRM_DISABLE_ID", id: null });
    }
  }, [agentState.confirmDisableId, unlinkAllFromAgent, toggleProfile, t]);

  const toggleExpand = useCallback(
    async (agentId: string) => {
      if (agentState.expandedAgentId === agentId) {
        dispatchAgent({ type: "SET_EXPANDED_AGENT", agentId: null });
        return;
      }
      dispatchAgent({ type: "SET_EXPANDED_AGENT", agentId });
      try {
        const skills = await invoke<string[]>("list_linked_skills", { agentId });
        dispatchAgent({ type: "SET_LINKED_SKILLS", agentId, skills });
      } catch (e) {
        console.error("Failed to list linked skills:", e);
        toast.error(t("settings.listLinkedFailed"));
      }
    },
    [agentState.expandedAgentId, t]
  );

  const handleUnlinkSingle = useCallback(
    async (skillName: string, agentId: string) => {
      try {
        await invoke("unlink_skill_from_agent", { skillName, agentId });
        dispatchAgent({ type: "REMOVE_LINKED_SKILL", agentId, skillName });
      } catch (e) {
        console.error("Unlink failed:", e);
        toast.error(t("settings.unlinkFailed"));
      }
    },
    [t]
  );

  // ── Language & appearance handlers ───────────────────────────────────────

  const handleLanguageChange = useCallback(
    (lang: string) => {
      setLanguage(lang);
      setCurrentLang(lang);
      invoke("update_tray_language", { lang }).catch(() => {});
    },
    []
  );

  const handleBackgroundStyleChange = useCallback((style: BackgroundStyle) => {
    setBackgroundStyle(style);
    applyBackgroundStyle(style);
  }, []);

  const handleBackgroundRunToggle = useCallback(async (enabled: boolean) => {
    writeBackgroundRun(enabled);
    try {
      if (enabled) {
        await invoke("set_patrol_enabled", { enabled: true });
      } else {
        await invoke("stop_patrol");
      }
    } catch (e) {
      writeBackgroundRun(!enabled);
      console.error("Update patrol background run failed:", e);
    }
  }, []);

  // ── AI handlers ───────────────────────────────────────────────────────────

  const handleAiTestConnection = useCallback(async () => {
    dispatchAi({ type: "START_TEST" });
    try {
      await saveAiConfig(aiState.config);
      dispatchAi({ type: "MARK_SAVED_CONFIG", config: aiState.config });
      await testConnection();
      dispatchAi({ type: "FINISH_TEST", result: "success" });
      setTimeout(() => dispatchAi({ type: "CLEAR_TEST_RESULT" }), 3000);
    } catch (e) {
      dispatchAi({ type: "FINISH_TEST", result: "error" });
      toast.error(t("settings.connectionFailed", { error: e }));
      setTimeout(() => dispatchAi({ type: "CLEAR_TEST_RESULT" }), 5000);
    }
  }, [aiState.config, saveAiConfig, testConnection, t]);

  const handleAiEnabledChange = useCallback(
    (enabled: boolean) => {
      dispatchAi({ type: "SET_FIELD", field: "enabled", value: enabled });
    },
    []
  );

  // ── Storage handlers ───────────────────────────────────────────────────────

  const handleCleanAllCaches = useCallback(async () => {
    setCleaningCaches(true);
    try {
      const [result] = await Promise.all([
        invoke<CacheCleanResult>("clear_all_caches"),
        new Promise((resolve) => setTimeout(resolve, 600)),
      ]);

      try {
        localStorage.removeItem("publisher-avatar-source-v1");
        localStorage.removeItem("skillstar_skipped_version");
        localStorage.removeItem("skillstar_last_check");
      } catch { /* ignore */ }

      const total = result.repos_removed + result.history_cleared + result.translation_cleared;
      if (total > 0) {
        toast.success(t("settings.cacheCleanDone", { count: total }));
      } else {
        toast.info(t("settings.cacheEmpty"));
      }
      await fetchStorageOverview();
    } catch (e) {
      console.error("Cache clean failed:", e);
      toast.error("Cleanup failed");
    } finally {
      setCleaningCaches(false);
    }
  }, [fetchStorageOverview, t]);

  const handleForceDelete = useCallback(
    async (target: ForceDeleteTarget) => {
      setForceDeletingTarget(target);
      try {
        let removed = 0;
        if (target === "hub") {
          removed = await invoke<number>("force_delete_installed_skills");
        } else if (target === "cache") {
          removed = await invoke<number>("force_delete_repo_caches");
        } else {
          removed = await invoke<number>("force_delete_app_config");
        }

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
        await fetchStorageOverview();
      } catch (e) {
        console.error("Force delete failed:", e);
        toast.error(t("settings.forceDeleteFailed"));
      } finally {
        setForceDeletingTarget(null);
      }
    },
    [fetchStorageOverview, notifySkillsRefresh, t]
  );

  const handleCleanBroken = useCallback(async () => {
    setCleaningBroken(true);
    try {
      const [fixed] = await Promise.all([
        invoke<number>("clean_broken_skills"),
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
      console.error("Clean broken skills failed:", e);
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
    return parseFloat((bytes / Math.pow(unitBase, sizeIndex)).toFixed(2)) + " " + sizes[sizeIndex];
  }, []);

  // ── Proxy config change handler ────────────────────────────────────────────

  const handleProxyConfigChange = useCallback((next: ProxyConfig) => {
    Object.entries(next).forEach(([key, value]) => {
      dispatchProxy({ type: "SET_FIELD", field: key as keyof ProxyConfig, value });
    });
  }, []);

  // ── AI config change handler ───────────────────────────────────────────────

  const handleAiConfigChange = useCallback((next: AiConfig) => {
    Object.entries(next).forEach(([key, value]) => {
      dispatchAi({ type: "SET_FIELD", field: key as keyof AiConfig, value });
    });
  }, []);

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-background">
      <div className="h-[60px] flex flex-col justify-center px-8 border-b border-border/40 bg-card/40 backdrop-blur-xl z-10 shrink-0">
        <h1>{t("settings.title")}</h1>
      </div>

      <motion.main
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, ease: "easeOut" }}
        className="flex-1 overflow-y-auto p-6"
      >
        <div className="max-w-[720px] mx-auto space-y-8 pb-12">
          <AgentConnectionsSection
            profiles={profiles}
            profilesLoading={profilesLoading}
            confirmDisableId={agentState.confirmDisableId}
            unlinkingId={agentState.unlinkingId}
            expandedAgentId={agentState.expandedAgentId}
            linkedSkills={agentState.linkedSkills}
            onToggleProfile={handleToggle}
            onToggleExpand={toggleExpand}
            onCancelDisable={() => dispatchAgent({ type: "SET_CONFIRM_DISABLE_ID", id: null })}
            onConfirmDisable={confirmDisable}
            onUnlinkSkill={handleUnlinkSingle}
          />

          <ProxySection
            proxyConfig={proxyState.config}
            ready={proxyState.loaded}
            proxyExpanded={proxyState.expanded}
            proxySaving={proxyState.saving}
            proxySaved={proxyState.savedIndicator}
            onToggleExpanded={() => dispatchProxy({ type: "TOGGLE_EXPANDED" })}
            onConfigChange={handleProxyConfigChange}
          />

          <AiProviderSection
            localAiConfig={aiState.config}
            ready={aiState.loaded}
            aiExpanded={aiState.expanded}
            aiSaving={aiState.saving}
            aiSaved={aiState.savedIndicator}
            aiTesting={aiState.testing}
            aiTestResult={aiState.testResult}
            showApiKey={aiState.showApiKey}
            onToggleExpanded={() => dispatchAi({ type: "TOGGLE_EXPANDED" })}
            onEnabledChange={handleAiEnabledChange}
            onConfigChange={handleAiConfigChange}
            onToggleShowApiKey={() => dispatchAi({ type: "TOGGLE_SHOW_API_KEY" })}
            onTestConnection={handleAiTestConnection}
          />

          <ShortTextServiceSection
            localAiConfig={aiState.config}
            mymemoryUsage={mymemoryUsage}
            onConfigChange={handleAiConfigChange}
          />

          <BackgroundRunSection
            enabled={backgroundRun}
            onToggle={handleBackgroundRunToggle}
          />

          <AppearanceSection
            backgroundStyle={backgroundStyle}
            onBackgroundStyleChange={handleBackgroundStyleChange}
          />

          <LanguageSection currentLang={currentLang} onLanguageChange={handleLanguageChange} />

          <StorageSection
            overview={storageOverview}
            loading={fetchingStorage}
            cleaning={cleaningCaches}
            cleaningBroken={cleaningBroken}
            forceDeletingTarget={forceDeletingTarget}
            formatBytes={formatBytes}
            onCleanAll={handleCleanAllCaches}
            onForceDeleteHub={() => handleForceDelete("hub")}
            onForceDeleteCache={() => handleForceDelete("cache")}
            onCleanBroken={handleCleanBroken}
          />

          <AboutSection ghInstalled={ghInstalled} />
        </div>
      </motion.main>
    </div>
  );
}
