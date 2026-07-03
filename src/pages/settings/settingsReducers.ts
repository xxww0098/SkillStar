import type { AiConfig, GitHubMirrorConfig, ProxyConfig } from "../../types";

export type ForceDeleteTarget = "hub" | "cache" | "config";

export const FORCE_DELETE_SLOW_HINT_MS = 2500;
export const FORCE_DELETE_UI_TIMEOUT_MS = 15000;

// ── Config equality (dirty-detection for useAutoSaveConfig) ──────────────────

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

// ── Initial config values (used as useAutoSaveConfig's `fallback`) ───────────

export const initialProxyConfig: ProxyConfig = {
  enabled: false,
  proxy_type: "http",
  host: "",
  port: 7897,
  username: null,
  password: null,
  bypass: null,
};

export const initialMirrorConfig: GitHubMirrorConfig = {
  enabled: false,
  preset_id: "ghproxy_vip",
  custom_url: null,
};

// ── Agent connections reducer ─────────────────────────────────────────────────
//
// Kept as a plain reducer (not migrated to useAutoSaveConfig): this is an
// expand/cache state machine (which agent's linked-skills panel is open, and
// the last-fetched linked-skills list per agent) with no load/save/debounce
// lifecycle, so it does not share the proxy/mirror/AI auto-save shape.

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
