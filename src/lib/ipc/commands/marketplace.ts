import type {
  AiKeywordSearchResult,
  LocalFirstResult,
  MarketplaceSkillDetails,
  OfficialPublisher,
  PublisherRepo,
  Skill,
  SyncStateEntry,
} from "../../../types";

/** Marketplace (skills.sh) discovery — local snapshot first, remote sync on demand. */
export interface MarketplaceCommands {
  // Source resolution (map skill name → git url) used by share-code export
  resolve_skill_sources: {
    args: { names: string[]; existingSources: Record<string, string> };
    result: Record<string, string>;
  };

  // AI-assisted search
  ai_extract_search_keywords: { args: { query: string }; result: string[] };

  // Local-first (preferred)
  get_leaderboard_local: { args: { category: string }; result: LocalFirstResult<Skill[]> };
  list_marketplace_skills_local: { args: Record<string, never>; result: LocalFirstResult<Skill[]> };
  search_marketplace_local: {
    args: { query: string; limit?: number };
    result: LocalFirstResult<Skill[]>;
  };
  get_publishers_local: {
    args: Record<string, never>;
    result: LocalFirstResult<OfficialPublisher[]>;
  };
  get_publisher_repos_local: {
    args: { publisherName: string };
    result: LocalFirstResult<PublisherRepo[]>;
  };
  get_repo_skills_local: { args: { source: string }; result: LocalFirstResult<Skill[]> };
  get_skill_detail_local: {
    args: { source: string; name: string };
    result: LocalFirstResult<MarketplaceSkillDetails>;
  };
  ai_search_marketplace_local: {
    args: { keywords: string[]; limit?: number };
    result: LocalFirstResult<AiKeywordSearchResult>;
  };

  // Snapshot maintenance
  sync_marketplace_scope: { args: { scope: string }; result: void };
  get_marketplace_sync_states: { args: Record<string, never>; result: SyncStateEntry[] };
}
