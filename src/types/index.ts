export type SkillCategory = "Hot" | "Popular" | "Rising" | "New" | "None";
export type ProxyType = "http" | "https" | "socks5";
export type AiStreamEvent = "start" | "delta" | "complete" | "error";

export interface Skill {
  name: string;
  description: string;
  localized_description?: string | null;
  /** "hub" for git-backed, "local" for user-authored local skills */
  skill_type: "hub" | "local";
  stars: number;
  installed: boolean;
  update_available: boolean;
  last_updated: string;
  git_url: string;
  tree_hash: string | null;
  category: SkillCategory;
  author: string | null;
  topics: string[];
  agent_links?: string[];
  /** Leaderboard rank position (1-indexed) */
  rank?: number;
  /** skills.sh source repo (e.g. "vercel-labs/skills") */
  source?: string;
}

export interface SkillUpdateState {
  name: string;
  update_available: boolean;
}

/** Return type of the `update_skill` command. When a repo-cached skill is
 *  updated the entire repo is pulled, which implicitly updates all siblings. */
export interface UpdateResult {
  skill: Skill;
  /** Names of sibling skills from the same repo whose update_available was
   *  also cleared by the repo pull. */
  siblings_cleared: string[];
  /** Per-agent re-link failures after the update ("Agent: error"). The update
   *  itself succeeded; warn the user that an agent deployment may be stale. */
  agent_link_failures: string[];
}

/** A new skill found in a cached repo that the user hasn't installed yet. */
export interface RepoNewSkill {
  repo_source: string;
  repo_url: string;
  skill_id: string;
  folder_path: string;
  description: string;
}

export interface MarketplaceResult {
  skills: Skill[];
  total_count: number;
  page: number;
  has_more: boolean;
}

export type SnapshotStatus = "fresh" | "stale" | "seeding" | "miss" | "error_fallback" | "remote_error";

export interface LocalFirstResult<T> {
  data: T;
  snapshot_status: SnapshotStatus;
  snapshot_updated_at: string | null;
  error?: string | null;
}

export interface AiKeywordSearchResult {
  skills: Skill[];
  total_count: number;
  /** Maps each keyword to the skill names it found */
  keyword_skill_map: Record<string, string[]>;
}

export interface MarketplaceSkillDetails {
  summary: string | null;
  readme: string | null;
  weekly_installs: string | null;
  github_stars: number | null;
  first_seen: string | null;
}

export interface OfficialPublisher {
  name: string;
  repo: string;
  repo_count: number;
  skill_count: number;
  url: string;
}

export interface PublisherRepoSkill {
  name: string;
  installs: number;
}

export interface PublisherRepo {
  repo: string;
  source: string;
  skill_count: number;
  installs_label: string;
  installs: number;
  url: string;
  skills: PublisherRepoSkill[];
}

export interface SyncStateEntry {
  scope: string;
  last_success_at: string | null;
  last_attempt_at: string | null;
  last_error: string | null;
  next_refresh_at: string | null;
  schema_version: number;
}
export interface AgentProfile {
  id: string;
  display_name: string;
  icon: string;
  global_skills_dir: string;
  project_skills_rel: string;
  installed: boolean;
  enabled: boolean;
  synced_count: number;
}

export interface CustomProfileDef {
  id: string;
  display_name: string;
  global_skills_dir: string;
  project_skills_rel: string;
  icon_data_uri: string | null;
}

export interface SkillCardDeck {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
  skill_sources: Record<string, string>;
  created_at: string;
  updated_at: string;
}

export type SortOption = "stars-desc" | "updated" | "name";
export type ViewMode = "grid" | "list";
export type NavPage = "my-skills" | "marketplace" | "skill-cards" | "projects" | "mcp" | "settings";

/** Sub-page navigation for drill-down views */
export type SubPage = {
  type: "publisher-detail";
  publisher: OfficialPublisher;
} | null;

export interface SkillContent {
  name: string;
  description: string | null;
  triggers: string[];
  scopes: string[];
  "allowed-tools": string[];
  content: string;
}

export interface AiConfigStatus {
  enabled: boolean;
  api_key: string;
}

export interface AiPickRecommendation {
  name: string;
  score: number;
  reason: string;
}

export interface AiPickResponse {
  recommendations: AiPickRecommendation[];
  fallbackUsed: boolean;
  roundsSucceeded: number;
}

export interface AiStreamPayload {
  requestId: string;
  event: AiStreamEvent;
  delta?: string | null;
  message?: string | null;
  providerId?: string | null;
}

/** Phases of the AST-based translation pipeline reported by the backend. */
export type AiTranslatePipelinePhase = "prepare" | "translate" | "finalize" | "guard";

/** Per-event bundle progress reported on `ai://translate-stream`. */
export interface AiTranslatePipelineProgress {
  phase: AiTranslatePipelinePhase;
  current: number;
  total: number;
}

/** Translation model speed and usage reported when SKILL.md translation completes. */
export interface AiTranslateMetrics {
  model: string;
  targetLanguage: string;
  elapsedMs: number;
  inputChars: number;
  outputChars: number;
  promptTokens?: number | null;
  completionTokens?: number | null;
  totalTokens?: number | null;
  tps?: number | null;
  cacheHit: boolean;
  modelCalls: number;
}

export interface AiTranslateSkillStreamResult {
  content: string;
  metrics: AiTranslateMetrics;
}

/** Payload emitted on the `ai://translate-stream` Tauri event. */
export interface AiTranslateStreamPayload {
  requestId: string;
  event: "start" | "progress" | "complete" | "error";
  pipelineProgress?: AiTranslatePipelineProgress | null;
  metrics?: AiTranslateMetrics | null;
  message?: string | null;
}

export interface AiProviderRef {
  app_id: string;
  provider_id: string;
}

export interface FrontmatterEntry {
  key: string;
  value: string;
}

export interface ProxyConfig {
  enabled: boolean;
  proxy_type: ProxyType;
  host: string;
  port: number;
  username: string | null;
  password: string | null;
  bypass: string | null;
}

export interface GitHubMirrorPreset {
  id: string;
  name: string;
  url: string;
  supports_clone: boolean;
}

export interface GitHubMirrorConfig {
  enabled: boolean;
  preset_id: string | null;
  custom_url: string | null;
}

export interface ProjectEntry {
  path: string;
  name: string;
  created_at: string;
}

/** Per `project_skills_rel` path (e.g. `.agents/skills`), how hub skills are materialized in the project. */
export type ProjectDeployMode = "symlink" | "copy";

export interface SkillsList {
  agents: Record<string, string[]>;
  /** Keyed by `project_skills_rel`; omitted or empty means symlink for that path. */
  deploy_modes?: Record<string, ProjectDeployMode>;
  updated_at: string;
}

export interface ScannedSkill {
  name: string;
  agent_id: string;
  is_symlink: boolean;
  in_hub: boolean;
  has_skill_md: boolean;
}

export interface ProjectScanResult {
  skills: ScannedSkill[];
  agents_found: string[];
}

export interface ImportTarget {
  name: string;
  agent_id: string;
}

export interface ImportResult {
  imported_to_hub: string[];
  skills_list_updated: boolean;
  symlink_count: number;
}

export interface ImportDone {
  hub: number;
  links: number;
}

// ── Project Agent Detection ─────────────────────────────────────────

export interface DetectedAgent {
  agent_id: string;
  display_name: string;
  icon: string;
  project_skills_rel: string;
  exists: boolean;
}

export interface AmbiguousGroup {
  path: string;
  agent_ids: string[];
  agent_names: string[];
}

export interface ProjectAgentDetection {
  detected: DetectedAgent[];
  /** Groups of agents that share the same directory and that directory exists */
  ambiguous_groups: AmbiguousGroup[];
  /** Agent IDs with a unique directory that exists — safe to auto-enable */
  auto_enable: string[];
}

export interface FormatPreset {
  base_url: string;
  api_key: string;
  model: string;
}

export interface AiConfig {
  enabled: boolean;
  api_format: "openai" | "anthropic" | "local";
  provider_ref: AiProviderRef | null;
  base_url: string;
  api_key: string;
  model: string;
  target_language: string;
  /** Model context window in K tokens (e.g. 128 = 128K tokens) */
  context_window_k: number;
  max_concurrent_requests: number;
  /** Per-format saved presets */
  openai_preset: FormatPreset;
  anthropic_preset: FormatPreset;
  local_preset: FormatPreset;
}

// ── GitHub Repo Scanner ─────────────────────────────────────────────

export interface DiscoveredSkill {
  id: string;
  folder_path: string;
  description: string;
  already_installed: boolean;
}

export interface ScanResult {
  source: string;
  source_url: string;
  skills: DiscoveredSkill[];
}

export interface SkillInstallTarget {
  id: string;
  folder_path: string;
}

export interface RepoHistoryEntry {
  source: string;
  source_url: string;
  last_used: string;
}

export interface StorageOverview {
  data_root_path: string;
  hub_root_path: string;
  is_hub_under_data: boolean;
  config_bytes: number;
  config_path: string;
  hub_bytes: number;
  hub_path: string;
  hub_count: number;
  broken_count: number;
  local_count: number;
  local_bytes: number;
  local_path: string;
  cache_bytes: number;
  cache_path: string;
  cache_count: number;
  cache_unused_count: number;
  cache_unused_bytes: number;
  history_count: number;
}

export interface CacheCleanResult {
  repos_removed: number;
  history_cleared: number;
  translation_cleared: number;
}

// ── GitHub Publish ──────────────────────────────────────────────────

export type GhStatus =
  | { status: "NotInstalled" }
  | { status: "NotAuthenticated" }
  | { status: "Ready"; username: string };

export interface PublishResult {
  url: string;
  git_url: string;
  source_folder: string;
}

export interface GitInstallInstruction {
  label: string;
  command: string;
}

export type GitStatus =
  | { status: "Installed"; version: string }
  | {
      status: "NotInstalled";
      os: string;
      install_instructions: GitInstallInstruction[];
      download_url: string;
    };

export interface UserRepo {
  full_name: string;
  url: string;
  description: string;
  is_public: boolean;
  folders: string[];
}

// ── Skill Bundle (.ags) ──────────────────────────────────────

export interface BundleManifest {
  format_version: number;
  name: string;
  description: string;
  version: string;
  author: string;
  created_at: string;
  files: string[];
  checksum: string;
}

export interface ImportBundleResult {
  name: string;
  description: string;
  file_count: number;
  replaced: boolean;
}

export interface MultiManifestEntry {
  name: string;
  description: string;
  file_count: number;
}

export interface MultiManifest {
  format_version: number;
  created_at: string;
  skills: MultiManifestEntry[];
  checksum: string;
}

export interface ImportMultiBundleResult {
  skill_names: string[];
  total_file_count: number;
  replaced_count: number;
}

// ── Share Code Install ───────────────────────────────────────

/** One entry of a share-code payload sent to the Rust installer. */
export interface ShareCodeSkillInput {
  /** Skill name. */
  n: string;
  /** Git URL (empty when `c` is provided). */
  u: string;
  /** Base64-encoded SKILL.md body (optional). */
  c?: string;
  /** `true` when the source repo requires auth. */
  p?: boolean;
}

export type ShareSkillOutcome =
  | { status: "existing"; name: string }
  | { status: "installed"; name: string }
  | { status: "embedded"; name: string }
  | { status: "skipped"; name: string; reason: string };

export interface ShareCodeInstallSummary {
  requested_count: number;
  installed_names: string[];
  existing_names: string[];
  embedded_names: string[];
  skipped: { name: string; reason: string }[];
  outcomes: ShareSkillOutcome[];
}

// === Models Mode Types ===

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

export type McpTransport = "stdio" | "http" | "sse";

/** Tool ids that can receive MCP servers (matches `MCP_TOOL_IDS`). */
export const MCP_TOOL_IDS = ["claude-code", "claude-desktop", "codex", "gemini", "opencode"] as const;
export type McpToolId = (typeof MCP_TOOL_IDS)[number];

export interface McpServerEntry {
  id: string;
  /** Server key written verbatim into each tool's config. */
  name: string;
  transport: McpTransport | string;
  // stdio
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  // http / sse
  url?: string;
  headers?: Record<string, string>;
  // metadata
  description?: string;
  homepage?: string;
  tags?: string[];
  /** Per-tool enable flags, keyed by tool id. */
  enabled: Record<string, boolean>;
  sortIndex: number;
  createdAt?: number;
  updatedAt?: number;
}

/** Partial update — only present fields are applied. */
export interface McpServerPatch {
  name?: string;
  transport?: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  description?: string;
  homepage?: string;
  tags?: string[];
}

export interface McpStore {
  version: number;
  servers: McpServerEntry[];
}

export interface McpSyncResult {
  toolId: string;
  serverId: string;
  success: boolean;
  skipped?: boolean;
  configPath?: string;
  backupPath?: string;
  error?: string;
}

export interface McpToolStatus {
  toolId: string;
  label: string;
  configPath: string;
  installed: boolean;
  /** Number of MCP servers currently present in the live config file. */
  serverCount: number;
}

export interface McpServerWithSync {
  server: McpServerEntry;
  syncResults: McpSyncResult[];
}

// --- MCP marketplace (GitHub MCP Registry) — mirrors skillstar_marketplace::mcp_models ---

export type McpServerKind = "stdio" | "remote" | "both" | "unknown";

export interface McpRegistryPackageSummary {
  /** Runner command: npx / uvx / docker / dnx / … */
  runtime: string;
  identifier: string;
  version?: string | null;
  /** Env var names the user must supply (required or secret). */
  requiredEnv: string[];
}

export interface McpRegistryRemoteSummary {
  /** Normalized transport: "http" | "sse". */
  transport: string;
  url: string;
  requiredHeaders: string[];
}

/** Card model for the MCP marketplace list/search. */
export interface McpMarketEntry {
  id: string;
  /** Cleaned display name (last path segment of `namespace`). */
  name: string;
  /** Full registry name, e.g. "io.github.netdata/mcp-server". */
  namespace: string;
  description: string;
  repoUrl: string;
  stars: number;
  license?: string | null;
  version?: string | null;
  kind: McpServerKind;
  /** Distinct runner hints across packages, e.g. ["uvx"], ["npx"]. */
  runtimes: string[];
  updatedAt?: string | null;
}

/** Detail model: card fields + readme + package/remote display. */
export interface McpMarketServerDetail extends McpMarketEntry {
  readme?: string | null;
  packages: McpRegistryPackageSummary[];
  remotes: McpRegistryRemoteSummary[];
}

/** A built-in / recommended-to-install MCP server template (mirrors `skillstar_models::mcp::McpPreset`). */
export interface McpPreset {
  id: string;
  /** Server key written verbatim into each tool's config (and the entry name). */
  name: string;
  description: string;
  homepage: string;
  transport: McpTransport | string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  tags?: string[];
  /** Env keys the user must fill in before the server works (e.g. ["API_KEY"]). */
  requiredEnv?: string[];
}

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
