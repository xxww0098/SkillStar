import type { AiConfig, GitHubMirrorConfig, ProxyConfig } from "../../types";

export type ForceDeleteTarget = "hub" | "cache" | "config";

export const AUTO_SAVE_DELAY_MS = 600;
export const FORCE_DELETE_SLOW_HINT_MS = 2500;
export const FORCE_DELETE_UI_TIMEOUT_MS = 15000;

export function isSameProxyConfig(a: ProxyConfig, b: ProxyConfig): boolean {
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

export function isSameMirrorConfig(a: GitHubMirrorConfig, b: GitHubMirrorConfig): boolean {
  return a.enabled === b.enabled && a.preset_id === b.preset_id && a.custom_url === b.custom_url;
}

export function isSameAiConfig(a: AiConfig, b: AiConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.api_format === b.api_format &&
    (a.provider_ref?.app_id ?? "") === (b.provider_ref?.app_id ?? "") &&
    (a.provider_ref?.provider_id ?? "") === (b.provider_ref?.provider_id ?? "") &&
    a.base_url === b.base_url &&
    a.api_key === b.api_key &&
    a.model === b.model &&
    a.target_language === b.target_language &&
    a.context_window_k === b.context_window_k &&
    a.max_concurrent_requests === b.max_concurrent_requests &&
    JSON.stringify(a.openai_preset) === JSON.stringify(b.openai_preset) &&
    JSON.stringify(a.anthropic_preset) === JSON.stringify(b.anthropic_preset) &&
    JSON.stringify(a.local_preset) === JSON.stringify(b.local_preset)
  );
}

// ── Reducers ─────────────────────────────────────────────────────────────────

export type ProxyAction =
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

export interface ProxyState {
  config: ProxyConfig;
  savedConfig: ProxyConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  loaded: boolean;
}

export const initialProxyConfig: ProxyConfig = {
  enabled: false,
  proxy_type: "http",
  host: "",
  port: 7897,
  username: null,
  password: null,
  bypass: null,
};

export function proxyReducer(state: ProxyState, action: ProxyAction): ProxyState {
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

export type MirrorAction =
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

export interface MirrorState {
  config: GitHubMirrorConfig;
  savedConfig: GitHubMirrorConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  loaded: boolean;
}

export const initialMirrorConfig: GitHubMirrorConfig = {
  enabled: false,
  preset_id: "ghproxy_vip",
  custom_url: null,
};

export function mirrorReducer(state: MirrorState, action: MirrorAction): MirrorState {
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

export type AiAction =
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
  | { type: "REVERT"; config: AiConfig };

export interface AiState {
  config: AiConfig;
  savedConfig: AiConfig;
  saving: boolean;
  savedIndicator: boolean;
  expanded: boolean;
  testing: boolean;
  testResult: "success" | "error" | null;
  testLatency: number | null;
  loaded: boolean;
}

export function aiReducer(state: AiState, action: AiAction): AiState {
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
    case "REVERT":
      return { ...state, config: action.config, saving: false };
    default:
      return state;
  }
}

export type AgentAction =
  | { type: "SET_EXPANDED_AGENT"; agentId: string | null }
  | { type: "SET_LINKED_SKILLS"; agentId: string; skills: string[] }
  | { type: "REMOVE_LINKED_SKILL"; agentId: string; skillName: string };

export interface AgentState {
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
}

export function agentReducer(state: AgentState, action: AgentAction): AgentState {
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
