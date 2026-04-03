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
}

export interface MarketplaceResult {
  skills: Skill[];
  total_count: number;
  page: number;
  has_more: boolean;
}

export type SnapshotStatus =
  | "fresh"
  | "stale"
  | "seeding"
  | "miss"
  | "error_fallback"
  | "remote_error";

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

export interface SecurityAudit {
  name: string;
  result: string;
}

export interface MarketplaceSkillDetails {
  summary: string | null;
  readme: string | null;
  weekly_installs: string | null;
  github_stars: number | null;
  first_seen: string | null;
  security_audits: SecurityAudit[];
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
export type NavPage =
  | "my-skills"
  | "marketplace"
  | "skill-cards"
  | "projects"
  | "security-scan"
  | "settings";

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
}

export type ShortTextTranslationSource = "ai" | "mymemory";

export interface ShortTextTranslationResult {
  text: string;
  source: ShortTextTranslationSource;
}

export interface MymemoryUsageStats {
  total_chars_sent: number;
  daily_chars_sent: number;
  daily_reset_date: string;
  updated_at: string;
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

export interface SkillsList {
  agents: Record<string, string[]>;
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
  base_url: string;
  api_key: string;
  model: string;
  target_language: string;
  short_text_priority: "ai_first" | "mymemory_first";
  /** Model context window in K tokens (e.g. 128 = 128K tokens) */
  context_window_k: number;
  /** Override: 0 = auto-derive from context_window_k */
  max_concurrent_requests: number;
  /** Override: 0 = auto-derive from context_window_k */
  chunk_char_limit: number;
  /** Override: 0 = auto-derive from context_window_k */
  scan_max_response_tokens: number;
  /** Optional anonymous security scan telemetry (aggregate only, no skill names/content). */
  security_scan_telemetry_enabled: boolean;
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

// ── Security Scan ───────────────────────────────────────────────────

export type RiskLevel = "Safe" | "Low" | "Medium" | "High" | "Critical";

export interface StaticFinding {
  file_path: string;
  line_number: number;
  pattern_id: string;
  snippet: string;
  severity: RiskLevel;
  confidence?: number;
  description: string;
  owasp_agentic_tags?: string[];
}

export interface AiFinding {
  category: string;
  severity: RiskLevel;
  confidence?: number;
  file_path: string;
  description: string;
  evidence: string;
  recommendation: string;
  owasp_agentic_tags?: string[];
}

export interface SecurityScanAnalyzerExecution {
  id: string;
  status: string;
  findings: number;
  error?: string | null;
}

export interface SecurityScanResult {
  skill_name: string;
  scanned_at: string;
  tree_hash: string | null;
  scan_mode: "static" | "smart" | "deep" | string;
  scanner_version: string;
  target_language?: string;
  risk_level: RiskLevel;
  risk_score?: number;
  confidence_score?: number;
  meta_deduped_count?: number;
  meta_consensus_count?: number;
  analyzer_executions?: SecurityScanAnalyzerExecution[];
  static_findings: StaticFinding[];
  ai_findings: AiFinding[];
  summary: string;
  files_scanned: number;
  total_chars_analyzed: number;
  incomplete: boolean;
  ai_files_analyzed: number;
  chunks_used: number;
}

export interface SecurityScanEvent {
  requestId: string;
  event:
    | "skill-start"
    | "file-start"
    | "skill-complete"
    | "chunk-error"
    | "error"
    | "done"
    | "progress";
  skillName?: string;
  fileName?: string;
  result?: SecurityScanResult;
  scanned?: number;
  total?: number;
  skillFileScanned?: number;
  skillFileTotal?: number;
  skillChunkCompleted?: number;
  skillChunkTotal?: number;
  activeChunkWorkers?: number;
  maxChunkWorkers?: number;
  message?: string;
  phase?:
    | "collect"
    | "static"
    | "triage"
    | "ai-analyze"
    | "aggregate"
    | "done"
    | "error"
    | string;
}

export interface SecurityScanEstimate {
  requestedMode: "static" | "smart" | "deep" | string;
  effectiveMode: "static" | "smart" | "deep" | string;
  totalSkills: number;
  totalFiles: number;
  aiEligibleFiles: number;
  estimatedChunks: number;
  estimatedApiCalls: number;
  estimatedTotalChars: number;
  chunkCharLimit: number;
}

export interface SecurityScanTrailItem {
  fileName: string;
  skillName: string | null;
  stage: string | null;
  riskLevel?: RiskLevel;
  reasonLabels?: string[];
  timestamp: number;
}

export interface SecurityScanLogEntry {
  file_name: string;
  path: string;
  created_at: string;
  size_bytes: number;
}

export interface SecurityScanRuleOverride {
  enabled?: boolean;
  severity?: string;
}

export interface SecurityScanPolicy {
  preset: string;
  severity_threshold: string;
  enabled_analyzers: string[];
  ignore_rules: string[];
  rule_overrides: Record<string, SecurityScanRuleOverride>;
}
