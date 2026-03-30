import { motion } from "framer-motion";
import { useState, useEffect, useCallback } from "react";
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
import {
  readSkillUpdateRefreshMode,
  writeSkillUpdateRefreshMode,
  type SkillUpdateRefreshMode,
} from "../../lib/skillUpdateRefresh";
import type { AiConfig, ProxyConfig } from "../../types";
import { AgentConnectionsSection } from "./AgentConnectionsSection";
import { ProxySection } from "./ProxySection";
import { AiProviderSection } from "./AiProviderSection";
import { UpdateRefreshSection } from "./UpdateRefreshSection";
import { AppearanceSection } from "./AppearanceSection";
import { LanguageSection } from "./LanguageSection";
import { StorageSection, type StorageOverview } from "./StorageSection";
import { AboutSection } from "./AboutSection";

interface CacheCleanResult {
  repos_removed: number;
  history_cleared: number;
}

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
    a.target_language === b.target_language
  );
}

export function Settings() {
  const { t, i18n } = useTranslation();
  const [currentLang, setCurrentLang] = useState(i18n.language);
  const [backgroundStyle, setBackgroundStyle] = useState<BackgroundStyle>(
    () => readBackgroundStyle()
  );
  const [updateRefreshMode, setUpdateRefreshMode] = useState<SkillUpdateRefreshMode>(
    () => readSkillUpdateRefreshMode()
  );
  const { profiles, loading: profilesLoading, toggleProfile, unlinkAllFromAgent } = useAgentProfiles();
  const [unlinkingId, setUnlinkingId] = useState<string | null>(null);
  const [expandedAgentId, setExpandedAgentId] = useState<string | null>(null);
  const [linkedSkills, setLinkedSkills] = useState<Record<string, string[]>>({});
  const [ghInstalled, setGhInstalled] = useState<boolean | null>(null);
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 7897,
    username: null,
    password: null,
    bypass: null,
  });
  const [proxySaving, setProxySaving] = useState(false);
  const [proxySaved, setProxySaved] = useState(false);
  const [proxyExpanded, setProxyExpanded] = useState(false);
  const [savedProxyConfig, setSavedProxyConfig] = useState<ProxyConfig>({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 7897,
    username: null,
    password: null,
    bypass: null,
  });
  const [proxyLoaded, setProxyLoaded] = useState(false);

  const { config: aiConfig, loading: aiLoading, saveConfig: saveAiConfig, testConnection } = useAiConfig();
  const [localAiConfig, setLocalAiConfig] = useState(aiConfig);
  const [savedAiConfig, setSavedAiConfig] = useState(aiConfig);
  const [aiExpanded, setAiExpanded] = useState(false);
  const [aiSaving, setAiSaving] = useState(false);
  const [aiSaved, setAiSaved] = useState(false);
  const [aiTesting, setAiTesting] = useState(false);
  const [aiTestResult, setAiTestResult] = useState<"success" | "error" | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);

  const [storageOverview, setStorageOverview] = useState<StorageOverview | null>(null);
  const [fetchingStorage, setFetchingStorage] = useState(false);
  const [cleaningCaches, setCleaningCaches] = useState(false);
  const [cleaningBroken, setCleaningBroken] = useState(false);
  const [forceDeletingTarget, setForceDeletingTarget] = useState<ForceDeleteTarget | null>(null);

  const [confirmDisableId, setConfirmDisableId] = useState<string | null>(null);

  const fetchStorageOverview = useCallback(async () => {
    setFetchingStorage(true);
    try {
      const info = await invoke<StorageOverview>("get_storage_overview");
      setStorageOverview(info);
    } catch (e) {
      console.error("Failed to fetch storage overview:", e);
    } finally {
      setFetchingStorage(false);
    }
  }, []);

  useEffect(() => {
    fetchStorageOverview();
  }, [fetchStorageOverview]);

  useEffect(() => {
    invoke<boolean>("check_gh_installed").then(setGhInstalled).catch(() => setGhInstalled(false));
  }, []);

  useEffect(() => {
    invoke<ProxyConfig>("get_proxy_config")
      .then((config) => {
        setProxyConfig(config);
        setSavedProxyConfig(config);
      })
      .catch(() => {})
      .finally(() => setProxyLoaded(true));
  }, []);

  useEffect(() => {
    if (!proxyLoaded || proxySaving || isSameProxyConfig(proxyConfig, savedProxyConfig)) {
      return;
    }

    const nextConfig = proxyConfig;
    const previousConfig = savedProxyConfig;
    const timer = setTimeout(() => {
      setProxySaving(true);
      invoke("save_proxy_config", { config: nextConfig })
        .then(() => {
          setSavedProxyConfig(nextConfig);
          setProxySaved(true);
          setTimeout(() => setProxySaved(false), 2000);
        })
        .catch((e) => {
          console.error("Failed to save proxy config:", e);
          setProxyConfig(previousConfig);
          toast.error(t("settings.saveProxyFailed"));
        })
        .finally(() => {
          setProxySaving(false);
        });
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [proxyConfig, proxyLoaded, proxySaving, savedProxyConfig, t]);

  useEffect(() => {
    if (aiLoading) return;
    setLocalAiConfig(aiConfig);
    setSavedAiConfig(aiConfig);
  }, [aiConfig, aiLoading]);

  useEffect(() => {
    if (
      aiLoading ||
      aiSaving ||
      aiTesting ||
      isSameAiConfig(localAiConfig, savedAiConfig)
    ) {
      return;
    }

    const nextConfig = localAiConfig;
    const previousConfig = savedAiConfig;
    const timer = setTimeout(() => {
      setAiSaving(true);
      saveAiConfig(nextConfig)
        .then(() => {
          setSavedAiConfig(nextConfig);
          setAiSaved(true);
          setTimeout(() => setAiSaved(false), 2000);
        })
        .catch(() => {
          setLocalAiConfig(previousConfig);
          toast.error(t("settings.saveAiFailed"));
        })
        .finally(() => {
          setAiSaving(false);
        });
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [aiLoading, aiSaving, aiTesting, localAiConfig, saveAiConfig, savedAiConfig, t]);

  const openAiProviderIfRequested = useCallback(() => {
    try {
      const focus = localStorage.getItem("skillstar:settings-focus");
      if (focus === "ai-provider") {
        setAiExpanded(true);
        localStorage.removeItem("skillstar:settings-focus");
      }
    } catch {
      // ignore localStorage access errors
    }
  }, []);

  useEffect(() => {
    openAiProviderIfRequested();
  }, [openAiProviderIfRequested]);

  const handleToggle = async (profile: (typeof profiles)[0]) => {
    if (profile.enabled && profile.synced_count > 0) {
      setConfirmDisableId(profile.id);
      return;
    }
    try {
      await toggleProfile(profile.id);
    } catch (e) {
      console.error("Toggle failed:", e);
      toast.error(t("settings.toggleFailed"));
    }
  };

  const confirmDisable = async () => {
    if (!confirmDisableId) return;
    setUnlinkingId(confirmDisableId);
    try {
      await unlinkAllFromAgent(confirmDisableId);
      await toggleProfile(confirmDisableId);
    } catch (e) {
      console.error("Disable failed:", e);
      toast.error(t("settings.disableFailed"));
    } finally {
      setUnlinkingId(null);
      setConfirmDisableId(null);
    }
  };

  const toggleExpand = async (agentId: string) => {
    if (expandedAgentId === agentId) {
      setExpandedAgentId(null);
      return;
    }
    setExpandedAgentId(agentId);
    try {
      const skills = await invoke<string[]>("list_linked_skills", { agentId });
      setLinkedSkills((p) => ({ ...p, [agentId]: skills }));
    } catch (e) {
      console.error("Failed to list linked skills:", e);
      toast.error(t("settings.listLinkedFailed"));
    }
  };

  const handleUnlinkSingle = async (skillName: string, agentId: string) => {
    try {
      await invoke("unlink_skill_from_agent", { skillName, agentId });
      setLinkedSkills((p) => ({
        ...p,
        [agentId]: (p[agentId] ?? []).filter((s) => s !== skillName),
      }));
    } catch (e) {
      console.error("Unlink failed:", e);
      toast.error(t("settings.unlinkFailed"));
    }
  };

  const handleLanguageChange = (lang: string) => {
    setLanguage(lang);
    setCurrentLang(lang);
  };

  const handleBackgroundStyleChange = (style: BackgroundStyle) => {
    setBackgroundStyle(style);
    applyBackgroundStyle(style);
  };

  const handleUpdateRefreshModeChange = useCallback(
    (mode: SkillUpdateRefreshMode) => {
      const saved = writeSkillUpdateRefreshMode(mode);
      setUpdateRefreshMode(saved);
    },
    []
  );

  const handleAiTestConnection = useCallback(async () => {
    setAiTesting(true);
    setAiTestResult(null);
    try {
      await saveAiConfig(localAiConfig);
      setSavedAiConfig(localAiConfig);
      await testConnection();
      setAiTestResult("success");
      setTimeout(() => setAiTestResult(null), 3000);
    } catch (e) {
      setAiTestResult("error");
      toast.error(t("settings.connectionFailed", { error: e }));
      setTimeout(() => setAiTestResult(null), 5000);
    } finally {
      setAiTesting(false);
    }
  }, [localAiConfig, saveAiConfig, t, testConnection]);

  const handleAiEnabledChange = useCallback((enabled: boolean) => {
    setLocalAiConfig((prev) => (prev.enabled === enabled ? prev : { ...prev, enabled }));
  }, []);

  const handleCleanAllCaches = useCallback(async () => {
    setCleaningCaches(true);
    try {
      const [result] = await Promise.all([
        invoke<CacheCleanResult>("clear_all_caches"),
        new Promise((resolve) => setTimeout(resolve, 600)),
      ]);

      // Also clear frontend localStorage caches
      try {
        localStorage.removeItem("publisher-avatar-source-v1");
        localStorage.removeItem("skillstar_skipped_version");
        localStorage.removeItem("skillstar_last_check");
      } catch { /* ignore */ }

      const total = result.repos_removed + result.history_cleared;
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

  const handleForceDelete = useCallback(async (target: ForceDeleteTarget) => {
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

      await fetchStorageOverview();
    } catch (e) {
      console.error("Force delete failed:", e);
      toast.error(t("settings.forceDeleteFailed"));
    } finally {
      setForceDeletingTarget(null);
    }
  }, [fetchStorageOverview, t]);

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
      await fetchStorageOverview();
    } catch (e) {
      console.error("Clean broken skills failed:", e);
      toast.error(t("settings.forceDeleteFailed"));
    } finally {
      setCleaningBroken(false);
    }
  }, [fetchStorageOverview, t]);

  const formatBytes = useCallback((bytes: number) => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }, []);

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-background">
      <div className="h-[60px] flex flex-col justify-center px-8 border-b border-border/40 bg-card/40 backdrop-blur-xl z-10 shrink-0">
        <h1 className="text-heading-md font-semibold tracking-tight text-foreground">{t("settings.title")}</h1>
      </div>

      <motion.main
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, ease: "easeOut" }}
        className="flex-1 overflow-y-auto px-8 py-8"
      >
        <div className="max-w-[720px] mx-auto space-y-8 pb-12">
          <AgentConnectionsSection
            profiles={profiles}
            profilesLoading={profilesLoading}
            confirmDisableId={confirmDisableId}
            unlinkingId={unlinkingId}
            expandedAgentId={expandedAgentId}
            linkedSkills={linkedSkills}
            onToggleProfile={handleToggle}
            onToggleExpand={toggleExpand}
            onCancelDisable={() => setConfirmDisableId(null)}
            onConfirmDisable={confirmDisable}
            onUnlinkSkill={handleUnlinkSingle}
          />

          <ProxySection
            proxyConfig={proxyConfig}
            ready={proxyLoaded}
            proxyExpanded={proxyExpanded}
            proxySaving={proxySaving}
            proxySaved={proxySaved}
            onToggleExpanded={() => setProxyExpanded((prev) => !prev)}
            onConfigChange={setProxyConfig}
          />

          <AiProviderSection
            localAiConfig={localAiConfig}
            ready={!aiLoading}
            aiExpanded={aiExpanded}
            aiSaving={aiSaving}
            aiSaved={aiSaved}
            aiTesting={aiTesting}
            aiTestResult={aiTestResult}
            showApiKey={showApiKey}
            onToggleExpanded={() => setAiExpanded((prev) => !prev)}
            onEnabledChange={handleAiEnabledChange}
            onConfigChange={setLocalAiConfig}
            onToggleShowApiKey={() => setShowApiKey((prev) => !prev)}
            onTestConnection={handleAiTestConnection}
          />

          <UpdateRefreshSection
            mode={updateRefreshMode}
            onModeChange={handleUpdateRefreshModeChange}
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
