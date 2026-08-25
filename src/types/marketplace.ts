//! marketplace domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.
//!
//! The local-first snapshot contract (`LocalFirstResult`, `SnapshotStatus`,
//! `SyncStateEntry`) and the skill-detail payload (`MarketplaceSkillDetails`,
//! `SecurityAudit`) are generated via ts-rs from
//! `skillstar_marketplace::{snapshot, remote}` — see `src/types/generated/` and
//! `bun run types:gen`. Those shapes have a single SSOT in Rust; do not
//! hand-write a copy here. The remaining interfaces below are still
//! hand-maintained mirrors of `skillstar_marketplace::remote`, and each one is
//! a place the two sides can silently drift: the hand-written
//! `MarketplaceSkillDetails` below had been missing `security_audits` since the
//! field was added on the Rust side.

import type { McpPublisherSummary } from "./mcp";
import type { Skill } from "./skill";

export type { LocalFirstResult } from "./generated/LocalFirstResult";
export type { MarketplaceSkillDetails } from "./generated/MarketplaceSkillDetails";
export type { SecurityAudit } from "./generated/SecurityAudit";
export type { SnapshotStatus } from "./generated/SnapshotStatus";
export type { SyncStateEntry } from "./generated/SyncStateEntry";

export interface RepoNewSkill {
  repo_source: string;
  repo_url: string;
  skill_id: string;
  folder_path: string;
  description: string;
  /** Installed Skill the last update check identified this one as the
   *  successor of — the source renamed or moved it here. */
  renamed_from?: string | null;
}

export interface MarketplaceResult {
  skills: Skill[];
  total_count: number;
  page: number;
  has_more: boolean;
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

export type SortOption = "stars-desc" | "updated" | "name";

export type ViewMode = "grid" | "list";

export type NavPage = "my-skills" | "marketplace" | "skill-cards" | "projects" | "mcp" | "settings";

/** Drill-down sub-page payloads; `mcp-publisher-detail` carries the generated `McpPublisherSummary` (see `./mcp`). */

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
  /** Frontmatter quality issue codes (e.g. "missing_description"); empty = valid */
  frontmatter_issues: string[];
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
