//! models domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

import type { NavPage } from "./marketplace";

export type AppMode = "skills" | "usage" | "models";
/**
 * Historically the Models mode had multiple sub-pages. They have been merged
 * into a single hub; this type is kept as a single literal for back-compat
 * with call sites that still reference it.
 */

export type ModelsNavPage = "hub";

export type AllNavPage = NavPage | ModelsNavPage;

export type AppId = "claude" | "codex";

export interface LatencyResult {
  provider_id: string;
  app_id: AppId;
  latency_ms: number | null;
  status: "ok" | "timeout" | "error";
  error_message?: string;
  tested_at: string;
}

export interface ToolConfigTarget {
  tool_id: string;
  display_name: string;
  config_path: string;
  exists: boolean;
  current_provider?: string;
}

export interface ToolSyncResult {
  tool_id: string;
  success: boolean;
  config_path?: string;
  error?: string;
  backup_path?: string;
}

// === Flat Provider Store Types (v2 architecture) ===

export interface ProviderEntryFlat {
  id: string;
  name: string;
  base_url_openai: string;
  base_url_anthropic: string;
  /**
   * Unique "fetch available models" endpoint for this provider.
   *
   * All agent configurations (Claude, Codex, …) share this single URL when
   * populating the model picker. Typically an OpenAI-compatible
   * `.../v1/models` endpoint.
   */
  models_url: string;
  api_key: string;
  models: string[];
  default_model: string;
  sort_index: number;
  preset_id?: string;
  icon_color?: string;
  notes?: string;
  created_at?: number;
  meta?: Record<string, unknown>;
  /** Codex API format: "responses" (default) or "chat". */
  codex_wire_api?: string;
  /** Codex auth mode: "api_key" (default) or "oauth". */
  codex_auth_mode?: string;
}

export interface ModelCatalogEntry {
  id: string;
  display_name?: string | null;
  source_name?: string | null;
  description?: string | null;
  context_length?: number | null;
  max_completion_tokens?: number | null;
  cost?: Record<string, unknown> | null;
  raw?: Record<string, unknown> | null;
}

export interface ModelCatalogFetchResult {
  models: string[];
  catalog: ModelCatalogEntry[];
  metadata_sources: string[];
  missing_cost_count: number;
}

/** Typed settings for Codex CLI activation (wire_api and auth_mode). */

export interface CodexSettings {
  wire_api: "responses" | "chat";
  auth_mode: "api_key" | "oauth";
}

/**
 * One provider+model binding entry for an Agent tool. Mirrors the backend
 * `ToolActivation`. Single-provider agents (currently claude-code) hold at most
 * one; multi-provider agents (codex, opencode) may hold several.
 */
export interface ToolActivation {
  provider_id: string;
  model: string;
  settings?: CodexSettings | null;
  /** Unix seconds of the last successful disk sync (baseline for conflict detection). */
  last_sync_at?: number | null;
}

/**
 * OMP thinking levels usable as the `:suffix` on a model role value.
 * Mirrors `OMP_THINKING_LEVELS` in `crates/skillstar-models/src/tool_sync/types.rs`.
 */
export const OMP_THINKING_LEVELS = [
  "inherit",
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "auto",
] as const;
export type OmpThinkingLevel = (typeof OMP_THINKING_LEVELS)[number];

/**
 * One OMP model role → provider+model assignment. `provider_id` is a SkillStar
 * provider id; the on-disk `skillstar_*` key is derived at write time.
 */
export interface OmpRoleTarget {
  provider_id: string;
  model: string;
  thinking?: OmpThinkingLevel | null;
}

/**
 * OMP-specific binding-level settings. Mirrors the backend `OmpSettings`.
 * Keys of `roles` are OMP role names (`default`, `smol`, `slow`, `plan`, …).
 */
export interface OmpSettings {
  roles: Record<string, OmpRoleTarget>;
}

/** Binding-level settings bag. Only OMP populates it today. */
export type ToolBindingSettings = OmpSettings;

/**
 * All provider+model bindings for one Agent tool, plus which one is active.
 * `entries` is the ordered list; `active_index` points at the entry that owns
 * the agent's active pointer on disk. Empty `entries` = not bound.
 *
 * `settings` is the tool-level bag, the sibling of `ToolActivation.settings`
 * (which is per-provider). Config spanning several entries lives here — OMP's
 * model roles, where one role may target a different provider than the active
 * one.
 */
export interface ToolBinding {
  entries: ToolActivation[];
  active_index: number;
  settings?: ToolBindingSettings | null;
}

export type ToolActivationsMap = Record<string, ToolBinding>;

export interface FlatProvidersResponse {
  version: number;
  providers: ProviderEntryFlat[];
  tool_activations: ToolActivationsMap;
}

export interface ProviderPatchFlat {
  name?: string;
  base_url_openai?: string;
  base_url_anthropic?: string;
  models_url?: string;
  api_key?: string;
  models?: string[];
  default_model?: string;
  sort_index?: number;
  icon_color?: string;
  notes?: string;
  meta?: Record<string, unknown>;
  codex_wire_api?: string;
  codex_auth_mode?: string;
}

export interface ProviderPresetFlat {
  id: string;
  name: string;
  category: string;
  base_url_openai: string;
  base_url_anthropic: string;
  /**
   * Unique "fetch available models" endpoint shared by every agent config.
   */
  models_url: string;
  models: string[];
  icon_color: string;
  api_key_url?: string;
  balance_endpoint?: string;
  balance_parser?: string;
  endpoint_candidates?: string[];
}

export interface ProviderUpdateFlatResult {
  provider: ProviderEntryFlat;
  tool_sync_results: ToolSyncResult[];
}

export interface ToolConfigFileInfo {
  file_id: string;
  label: string;
  path: string;
  format: "json" | "toml" | string;
  exists: boolean;
  managed_by_skillstar: boolean;
}

export interface WriteToolConfigFileResult {
  success: boolean;
  backup_path?: string | null;
  error?: string | null;
}

export interface BalanceInfo {
  available: number;
  total?: number;
  currency: string;
  updated_at: number;
}

export interface ConnectionTestResult {
  status: "ok" | "auth_failed" | "timeout" | "network_error" | "model_unavailable";
  latency_ms?: number;
  error?: string;
}

/** Per-URL result from batch endpoint latency probe. */

export interface EndpointLatencyResult {
  url: string;
  latency_ms?: number | null;
  status?: number | null;
  error?: string | null;
}
