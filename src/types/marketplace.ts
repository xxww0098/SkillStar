//! marketplace domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

import type { McpPublisherSummary } from "./mcp";
import type { Skill } from "./skill";

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

export type SortOption = "stars-desc" | "updated" | "name";

export type ViewMode = "grid" | "list";

export type NavPage = "my-skills" | "marketplace" | "skill-cards" | "projects" | "mcp" | "settings";

/** One official MCP publisher on the marketplace grid (mirrors `McpPublisherSummary`). */

export type SubPage =
  | {
      type: "publisher-detail";
      publisher: OfficialPublisher;
    }
  | {
      type: "mcp-publisher-detail";
      publisher: McpPublisherSummary;
    }
  | null;

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

export interface RepoHistoryEntry {
  source: string;
  source_url: string;
  last_used: string;
}
