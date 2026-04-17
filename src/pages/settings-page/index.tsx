import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { DevModeBanner } from "../../features/settings/components/DevModeBanner";
import { AboutSection } from "../../features/settings/sections/AboutSection";
import { AcpSection } from "../../features/settings/sections/AcpSection";
import { AgentConnectionsSection } from "../../features/settings/sections/AgentConnectionsSection";
import { AiProviderSection } from "../../features/settings/sections/AiProviderSection";
import { AppearanceSection } from "../../features/settings/sections/AppearanceSection";
import {
  BackgroundRunSection,
  onBackgroundRunChanged,
  readBackgroundRun,
  writeBackgroundRun,
} from "../../features/settings/sections/BackgroundRunSection";
import { GitHubMirrorSection } from "../../features/settings/sections/GitHubMirrorSection";
import { LanguageSection } from "../../features/settings/sections/LanguageSection";
import { ProxySection } from "../../features/settings/sections/ProxySection";
import { StorageSection } from "../../features/settings/sections/StorageSection";
import { TranslationSection } from "../../features/settings/sections/TranslationSection";
import { useAgentProfiles } from "../../hooks/useAgentProfiles";
import { useAiConfig } from "../../hooks/useAiConfig";
import { setLanguage } from "../../i18n";
import { applyBackgroundStyle, type BackgroundStyle, readBackgroundStyle } from "../../lib/backgroundStyle";
import { toast } from "../../lib/toast";
import type { SettingsFocusTarget } from "../../lib/utils";
import type { AiConfig, CacheCleanResult, GitHubMirrorConfig, ProxyConfig, StorageOverview } from "../../types";

type ForceDeleteTarget = "hub" | "cache" | "config";

const AUTO_SAVE_DELAY_MS = 600;
const FORCE_DELETE_SLOW_HINT_MS = 2500;
const FORCE_DELETE_UI_TIMEOUT_MS = 15000;

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

function isSameMirrorConfig(a: GitHubMirrorConfig, b: GitHubMirrorConfig): boolean {
  return a.enabled === b.enabled && a.preset_id === b.preset_id && a.custom_url === b.custom_url;
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
    a.scan_max_response_tokens === b.scan_max_response_tokens &&
    a.security_scan_telemetry_enabled === b.security_scan_telemetry_enabled &&
    JSON.stringify(a.openai_preset) === JSON.stringify(b.openai_preset) &&
    JSON.stringify(a.anthropic_preset) === JSON.stringify(b.anthropic_preset) &&
    JSON.stringify(a.local_preset) === JSON.stringify(b.local_preset)
  );
}

// ── Reducers ─────────────────────────────────────────────────────────────────

type ProxyAction =
  | { type: "SET_FIELD"; field: keyof ProxyConfig; value: ProxyConfig[keyof ProxyConfig] }
  | { type: "SET_CONFIG"; config: ProxyConfig }
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
    case "SET_CONFIG":
      return { ...state, config: action.config };
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

// ── Mirror reducer ────────────────────────────────────────────────────────────

type MirrorAction =
  | { type: "SET_FIELD"; field: keyof GitHubMirrorConfig; value: GitHubMirrorConfig[keyof GitHubMirrorConfig] }
  | { type: "SET_CONFIG"; config: GitHubMirrorConfig }
  | { type: "LOAD"; config: GitHubMirrorConfig }
  | { type: "MARK_SAVED_CONFIG"; config: GitHubMirrorConfig }
  | { type: "START_SAVE" }
  | { type: "FINISH_SAVE" }
  | { type: "MARK_SAVED_INDICATOR" }
  | { type: "CLEAR_SAVED_INDICATOR" }
  | { type: "TOGGLE_EXPANDED" }
  | { type: "START_LOAD" }
  | { type: "REVERT"; config: GitHubMirrorConfig };

interface MirrorState {
  config: GitHubMirrorConfig;
  savedConfig: GitHubMirrorConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  loaded: boolean;
}

const initialMirrorConfig: GitHubMirrorConfig = {
  enabled: false,
  preset_id: "ghproxy_vip",
  custom_url: null,
};

function mirrorReducer(state: MirrorState, action: MirrorAction): MirrorState {
  switch (action.type) {
    case "SET_FIELD":
      return { ...state, config: { ...state.config, [action.field]: action.value } };
    case "SET_CONFIG":
      return { ...state, config: action.config };
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
  | { type: "SET_CONFIG"; config: AiConfig }
  | { type: "LOAD"; config: AiConfig }
  | { type: "MARK_SAVED_CONFIG"; config: AiConfig }
  | { type: "START_SAVE" }
  | { type: "FINISH_SAVE" }
  | { type: "MARK_SAVED_INDICATOR" }
  | { type: "CLEAR_SAVED_INDICATOR" }
  | { type: "TOGGLE_EXPANDED" }
  | { type: "START_TEST" }
  | { type: "FINISH_TEST"; result: "success" | "error"; latency?: number }
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
  testLatency: number | null;
  showApiKey: boolean;
  loaded: boolean;
}

function aiReducer(state: AiState, action: AiAction): AiState {
  switch (action.type) {
    case "SET_FIELD":
      return { ...state, config: { ...state.config, [action.field]: action.value } };
    case "SET_CONFIG":
      return { ...state, config: action.config };
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
      return { ...state, testing: true, testResult: null, testLatency: null };
    case "FINISH_TEST":
      return { ...state, testing: false, testResult: action.result, testLatency: action.latency ?? null };
    case "CLEAR_TEST_RESULT":
      return { ...state, testResult: null, testLatency: null };
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
  | { type: "REMOVE_LINKED_SKILL"; agentId: string; skillName: string };

interface AgentState {
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
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
    default:
      return state;
  }
}

// ── Settings Sidebar Navigation ─────────────────────────────────────────────

import {
  Bot,
  EyeOff,
  Globe,
  HardDrive,
  Languages as LanguagesIcon,
  type LucideIcon,
  Paintbrush,
  Sparkles,
  Terminal,
  Unlink,
  Zap,
} from "lucide-react";

const SETTINGS_SECTIONS: { id: string; labelKey: string; icon: LucideIcon }[] = [
  { id: "settings-agents", labelKey: "settings.agentConnections", icon: Unlink },
  { id: "settings-proxy", labelKey: "settings.networkProxy", icon: Globe },
  { id: "settings-mirror", labelKey: "settings.githubMirror", icon: Zap },
  { id: "settings-ai", labelKey: "settings.aiProvider", icon: Sparkles },
  { id: "settings-translation", labelKey: "settings.translationApis", icon: Globe },
  { id: "settings-acp", labelKey: "settings.acpTitle", icon: Bot },
  { id: "settings-background", labelKey: "settings.backgroundRun", icon: EyeOff },
  { id: "settings-appearance", labelKey: "settings.backgroundStyle", icon: Paintbrush },
  { id: "settings-language", labelKey: "settings.language", icon: LanguagesIcon },
  { id: "settings-storage", labelKey: "settings.storage", icon: HardDrive },
  { id: "settings-about", labelKey: "settings.about", icon: Terminal },
];

const SETTINGS_FOCUS_TO_SECTION_ID: Record<SettingsFocusTarget, string> = {
  "ai-provider": "settings-ai",
  translation: "settings-translation",
  storage: "settings-storage",
};

function SettingsSidebarNav() {
  const { t } = useTranslation();
  const [activeId, setActiveId] = useState(SETTINGS_SECTIONS[0].id);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pendingIdRef = useRef(SETTINGS_SECTIONS[0].id);

  useEffect(() => {
    const scrollRoot = document.getElementById("settings-scroll-container");
    if (!scrollRoot) return;

    const visibleIds = new Set<string>();

    const updateActiveId = () => {
      // If we are at the very bottom, forcefully select the last item
      // Add a 5px threshold to account for fractional pixel rounding errors
      if (Math.abs(scrollRoot.scrollHeight - scrollRoot.scrollTop - scrollRoot.clientHeight) < 5) {
        const lastId = SETTINGS_SECTIONS[SETTINGS_SECTIONS.length - 1].id;
        if (pendingIdRef.current !== lastId) {
          pendingIdRef.current = lastId;
          clearTimeout(timerRef.current);
          timerRef.current = setTimeout(() => setActiveId(lastId), 100);
        }
        return;
      }

      // Otherwise evaluate based on IntersectionObserver visibleIds
      let newId = pendingIdRef.current;
      for (const section of SETTINGS_SECTIONS) {
        if (visibleIds.has(section.id)) {
          newId = section.id;
          break;
        }
      }
      if (newId !== pendingIdRef.current) {
        pendingIdRef.current = newId;
        clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => setActiveId(newId), 100);
      }
    };

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            visibleIds.add(entry.target.id);
          } else {
            visibleIds.delete(entry.target.id);
          }
        }
        updateActiveId();
      },
      {
        root: scrollRoot,
        rootMargin: "-10px 0px -70% 0px",
        threshold: 0,
      },
    );

    const handleScroll = () => {
      updateActiveId();
    };

    scrollRoot.addEventListener("scroll", handleScroll, { passive: true });

    for (const section of SETTINGS_SECTIONS) {
      const el = document.getElementById(section.id);
      if (el) observer.observe(el);
    }

    return () => {
      scrollRoot.removeEventListener("scroll", handleScroll);
      observer.disconnect();
      clearTimeout(timerRef.current);
    };
  }, []);

  const handleClick = (id: string) => {
    const scrollRoot = document.getElementById("settings-scroll-container");
    const el = document.getElementById(id);
    if (el && scrollRoot) {
      const rootRect = scrollRoot.getBoundingClientRect();
      const sectionRect = el.getBoundingClientRect();
      const offset = 12;
      const targetTop = scrollRoot.scrollTop + (sectionRect.top - rootRect.top) - offset;
      scrollRoot.scrollTo({ top: Math.max(0, targetTop), behavior: "smooth" });
      clearTimeout(timerRef.current);
      pendingIdRef.current = id;
      setActiveId(id);
    }
  };

  return (
    <nav className="hidden lg:flex z-20 flex-col items-center gap-1.5 py-3 px-1.5 rounded-2xl border border-border/50 bg-card/80 backdrop-blur-2xl shadow-[0_8px_40px_-12px_rgba(0,0,0,0.3),0_0_0_1px_rgba(255,255,255,0.04)]">
      {SETTINGS_SECTIONS.map((section) => {
        const isActive = activeId === section.id;
        const Icon = section.icon;

        let nudgeClass = "";
        if (section.id === "settings-storage") nudgeClass = "translate-y-[1px]";
        if (section.id === "settings-about") nudgeClass = "translate-y-[1px] translate-x-[1px]";

        return (
          <button
            key={section.id}
            type="button"
            onClick={() => handleClick(section.id)}
            title={t(section.labelKey)}
            className={`w-9 h-9 flex items-center justify-center rounded-xl cursor-pointer ${
              isActive
                ? "bg-primary/15 text-primary"
                : "text-muted-foreground/45 hover:text-foreground hover:bg-muted/50"
            }`}
          >
            <Icon className={`w-[18px] h-[18px] ${nudgeClass}`} strokeWidth={isActive ? 2.2 : 1.7} />
          </button>
        );
      })}
    </nav>
  );
}

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
    showApiKey: false,
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

  // ── Mirror effects ────────────────────────────────────────────────────────

  useEffect(() => {
    dispatchMirror({ type: "START_LOAD" });
    invoke<GitHubMirrorConfig>("get_github_mirror_config")
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
      invoke("save_github_mirror_config", { config: mirrorState.config })
        .then(() => {
          dispatchMirror({ type: "MARK_SAVED_CONFIG", config: mirrorState.config });
          dispatchMirror({ type: "MARK_SAVED_INDICATOR" });
          setTimeout(() => dispatchMirror({ type: "CLEAR_SAVED_INDICATOR" }), 2000);
        })
        .catch((e) => {
          console.error("Failed to save mirror config:", e);
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
      const storageOverview = await invoke<StorageOverview>("get_storage_overview");
      setStorageOverview(storageOverview);
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
    invoke<boolean>("check_gh_installed")
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
        if (focus === "ai-provider" || focus === "translation" || focus === "storage") {
          localStorage.removeItem("skillstar:settings-focus");
          focusSettingsSection(focus);
        }
      } catch {
        // ignore localStorage access errors
      }
    };

    const handleFocusEvent = (event: Event) => {
      const target = (event as CustomEvent<{ target?: SettingsFocusTarget }>).detail?.target;
      if (target === "ai-provider" || target === "translation" || target === "storage") {
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
        console.error("Toggle failed:", e);
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
        const skills = await invoke<string[]>("list_linked_skills", { agentId });
        dispatchAgent({ type: "SET_LINKED_SKILLS", agentId, skills });
      } catch (e) {
        console.error("Failed to list linked skills:", e);
        toast.error(t("settings.listLinkedFailed"));
      }
    },
    [agentState.expandedAgentId, t],
  );

  const handleUnlinkSingle = useCallback(
    async (skillName: string, agentId: string) => {
      try {
        await invoke("unlink_skill_from_agent", { skillName, agentId });
        dispatchAgent({ type: "REMOVE_LINKED_SKILL", agentId, skillName });
        notifySkillsRefresh();
      } catch (e) {
        console.error("Unlink failed:", e);
        toast.error(t("settings.unlinkFailed"));
      }
    },
    [t, notifySkillsRefresh],
  );

  // ── Language & appearance handlers ───────────────────────────────────────

  const handleLanguageChange = useCallback((lang: string) => {
    setLanguage(lang);
    setCurrentLang(lang);
    invoke("update_tray_language", { lang }).catch(() => {});
  }, []);

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
        invoke<CacheCleanResult>("clear_all_caches"),
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
      console.error("Cache clean failed:", e);
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
          ? invoke<number>("force_delete_installed_skills")
          : target === "cache"
            ? invoke<number>("force_delete_repo_caches")
            : invoke<number>("force_delete_app_config");

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
              console.error("Background force delete failed:", e);
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
        console.error("Force delete failed:", e);
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

  return (
    <div className="flex-1 min-h-0 min-w-0 flex flex-col overflow-hidden bg-background">
      <div className="h-[60px] flex flex-col justify-center px-8 border-b border-border/40 bg-card/40 backdrop-blur-xl z-10 shrink-0">
        <h1>{t("settings.title")}</h1>
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
                  onToggleExpanded={() => dispatchProxy({ type: "TOGGLE_EXPANDED" })}
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
                  onToggleExpanded={() => dispatchMirror({ type: "TOGGLE_EXPANDED" })}
                  onConfigChange={handleMirrorConfigChange}
                />
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
                  showApiKey={aiState.showApiKey}
                  onToggleExpanded={() => dispatchAi({ type: "TOGGLE_EXPANDED" })}
                  onEnabledChange={handleAiEnabledChange}
                  onConfigChange={handleAiConfigChange}
                  onToggleShowApiKey={() => dispatchAi({ type: "TOGGLE_SHOW_API_KEY" })}
                  onTestConnection={handleAiTestConnection}
                />
              </section>

              <section id="settings-translation" className="scroll-mt-3">
                <TranslationSection />
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
                  onForceDeleteHub={() => handleForceDelete("hub")}
                  onForceDeleteCache={() => handleForceDelete("cache")}
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
