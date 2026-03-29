export type SkillCategory = "Hot" | "Popular" | "Rising" | "New" | "None";

export interface Skill {
  name: string;
  description: string;
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

export interface MarketplaceResult {
  skills: Skill[];
  total_count: number;
  page: number;
  has_more: boolean;
}

export interface MarketplaceDescriptionRequest {
  name: string;
  source?: string | null;
  git_url?: string | null;
}

export interface MarketplaceDescriptionPatch {
  key: string;
  name: string;
  source?: string | null;
  description: string;
  from_cache: boolean;
}

export interface OfficialPublisher {
  name: string;
  repo: string;
  repo_count: number;
  skill_count: number;
  url: string;
}

export interface PublisherRepo {
  repo: string;
  source: string;
  skill_count: number;
  installs_label: string;
  installs: number;
  url: string;
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
export type NavPage = "my-skills" | "marketplace" | "skill-cards" | "projects" | "settings";

/** Sub-page navigation for drill-down views */
export type SubPage =
  | { type: "publisher-detail"; publisher: OfficialPublisher }
  | null;

export interface SkillContent {
  name: string;
  description: string | null;
  triggers: string[];
  scopes: string[];
  "allowed-tools": string[];
  content: string;
}

export interface ProxyConfig {
  enabled: boolean;
  proxy_type: string;
  host: string;
  port: number;
  username: string | null;
  password: string | null;
  bypass: string | null;
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

export interface AiConfig {
  enabled: boolean;
  api_format: "openai" | "anthropic";
  base_url: string;
  api_key: string;
  model: string;
  target_language: string;
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

export interface UserRepo {
  full_name: string;
  url: string;
  description: string;
  is_public: boolean;
  folders: string[];
}

// ── Skill Bundle (.agentskill) ──────────────────────────────────────

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

