//! models domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

import type { Reasoning } from "./generated/Reasoning";
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

/**
 * Why a configured role never reached disk. Mirrors `RoleDropReason` in
 * `crates/skillstar-models/src/providers/roles.rs`.
 */
export type RoleDropReason =
  | "provider_not_bound"
  | "provider_has_no_endpoint"
  | "provider_missing"
  | "no_model"
  | "role_not_supported"
  | "invalid_role_name";

/** One role the last write skipped, with the reason. Mirrors `DroppedRole`. */
export interface DroppedRole {
  role: string;
  reason: RoleDropReason;
  provider_id?: string | null;
}

export interface ToolSyncResult {
  tool_id: string;
  success: boolean;
  config_path?: string;
  error?: string;
  backup_path?: string;
  /**
   * Roles the write did **not** put on disk. Absent means nothing was skipped.
   *
   * A successful sync can still have dropped half the role map — the panel then
   * shows an assignment the file does not have, which the user could previously
   * only discover by opening the file. Surfacing this is the whole reason the
   * field exists, so a caller that ignores it reintroduces the defect.
   */
  dropped_roles?: DroppedRole[];
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
  /**
   * What kind of reasoning control this model has, when the source said so.
   * Absent means "unknown", which the thinking picker treats as "offer
   * everything" — see `ompThinkingLevelsFor`.
   */
  reasoning?: Reasoning | null;
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
 * One role → provider+model assignment. `provider_id` is a SkillStar provider
 * id; the on-disk key (OMP's `skillstar_*`, Claude's env var) is derived at
 * write time.
 *
 * Domain-general, not OMP's: the backend's `ModelRef` is the one shape every
 * agent's writer consumes, and Claude's tier mapping now travels through the
 * same field. `thinking` is `ModelRef.effort` in OMP's spelling, which is the
 * spelling the v3 wire shape uses until WP-4 replaces it.
 */
export interface RoleTarget {
  provider_id: string;
  model: string;
  thinking?: OmpThinkingLevel | null;
}

/** Historical name for {@link RoleTarget}, kept while the OMP panel still uses it. */
export type OmpRoleTarget = RoleTarget;

/**
 * Binding-level settings: the role map, keyed by canonical role id.
 *
 * The keys are the ids the backend registry declares (`default`, `fast`,
 * `plan`, …), **not** the target file's spelling — the writer translates. That
 * is what lets Claude Code and OMP share one storage field despite spelling
 * `fast` as `ANTHROPIC_DEFAULT_HAIKU_MODEL` and `smol` respectively.
 */
export interface AgentRoleSettings {
  roles: Record<string, RoleTarget>;
}

/** Historical name for {@link AgentRoleSettings}. */
export type OmpSettings = AgentRoleSettings;

/** Binding-level settings bag. */
export type ToolBindingSettings = AgentRoleSettings;

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

/**
 * What kind of thing a preset describes. Generated as `PresetCategory` on the
 * Rust side; mirrored here because `ProviderPresetFlat` is still hand-written.
 *
 * `native_login` and `vendor_official` were one value (`official`) in v3, which
 * is why telling a browser login apart from an API-key vendor needed a
 * hardcoded id list. `openai_compatible` is the frontend's own synthetic
 * template and never arrives from the backend registry.
 */
export type PresetCategory = "domestic" | "relay" | "vendor_official" | "native_login" | "openai_compatible";

export interface ProviderPresetFlat {
  id: string;
  name: string;
  category: PresetCategory;
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
