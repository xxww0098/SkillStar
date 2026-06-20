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

export interface ProviderSettings {
  base_url: string;
  api_key: string;
  models: ModelMapping[];
  timeout_ms?: number;
  max_retries?: number;
}

export interface ModelMapping {
  source_model: string;
  target_model: string;
  enabled: boolean;
}

export interface ProviderEntry {
  id: string;
  name: string;
  category: string;
  settings_config: ProviderSettings;
  preset_id?: string;
  website_url?: string;
  api_key_url?: string;
  icon_color?: string;
  notes?: string;
  created_at?: number;
  sort_index?: number;
  meta?: Record<string, unknown>;
}

export interface AppProviders {
  providers: Record<string, ProviderEntry>;
  current: string | null;
}

export interface ProvidersStore {
  claude: AppProviders;
  codex: AppProviders;
}

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

export interface SwitchResult {
  app_id: AppId;
  provider_id: string;
  provider_name: string;
  tools_synced: ToolSyncResult[];
}

// === MCP (Model Context Protocol) Types ===
// NOTE: these mirror `skillstar_models::mcp` structs, which serialize with
// `#[serde(rename_all = "camelCase")]` — hence camelCase fields here.

export interface ProviderPreset {
  id: string;
  name: string;
  base_url: string;
  api_key_url: string;
  icon_color: string;
  models: string[];
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

export interface ToolActivation {
  provider_id: string;
  model: string;
  settings?: CodexSettings | null;
  /** Unix seconds of the last successful disk sync (baseline for conflict detection). */
  last_sync_at?: number | null;
}

export type ToolActivationsMap = Record<string, ToolActivation | null>;

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
