import type {
  AiKeywordSearchResult,
  LocalFirstResult,
  MarketplaceResult,
  MarketplaceSkillDetails,
  OfficialPublisher,
  PublisherRepo,
  PublisherRepoSkill,
  Skill,
  SyncStateEntry,
} from "../../../types";

/** Marketplace (skills.sh) discovery — local snapshot first, remote sync on demand. */
export interface MarketplaceCommands {
  // Remote-direct (legacy / fallback)
  search_skills_sh: { args: { query: string }; result: MarketplaceResult };
  get_skills_sh_leaderboard: { args: { category: string }; result: Skill[] };
  get_official_publishers: { args: Record<string, never>; result: OfficialPublisher[] };
  get_publisher_repos: { args: { publisherName: string }; result: PublisherRepo[] };
  get_publisher_repo_skills: {
    args: { publisherName: string; repoName: string };
    result: PublisherRepoSkill[];
  };
  get_marketplace_skill_details: {
    args: { source: string; name: string };
    result: MarketplaceSkillDetails;
  };

  // Source resolution (map skill name → git url) used by share-code export
  resolve_skill_sources: {
    args: { names: string[]; existingSources: Record<string, string> };
    result: Record<string, string>;
  };

  // AI-assisted search
  ai_extract_search_keywords: { args: { query: string }; result: string[] };
  ai_search_with_keywords: { args: { keywords: string[] }; result: AiKeywordSearchResult };

  // Local-first (preferred)
  get_leaderboard_local: { args: { category: string }; result: LocalFirstResult<Skill[]> };
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
