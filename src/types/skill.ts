//! skill domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export type SkillCategory = "Hot" | "Popular" | "Rising" | "New" | "None";

/** Where a removed Skill went at the tracked revision, when detection found it. */
export interface UpstreamSuccessor {
  skill_id: string;
  /** Repository-relative folder at the tracked revision. */
  folder_path: string;
  description: string;
  /** `git diff -M` similarity (0–100) of the two SKILL.md files, or null when
   *  the match came from an identical frontmatter `name` instead. */
  similarity: number | null;
}

/** The tracked source no longer ships this Skill at its installed path — found
 *  by an update check, before any pull. `update_available` stays a separate,
 *  content-only signal. */
export type UpstreamChange = {
  kind: "removed";
  /** Non-colliding `<name>.local` candidate for the "keep a local copy" exit. */
  suggested_local_name: string;
  successor: UpstreamSuccessor | null;
};

export interface Skill {
  name: string;
  description: string;
  localized_description?: string | null;
  /** "hub" for git-backed, "local" for user-authored local skills */
  skill_type: "hub" | "local";
  stars: number;
  installed: boolean;
  update_available: boolean;
  upstream_change?: UpstreamChange | null;
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
  upstream_change?: UpstreamChange | null;
}

export interface UpdateResult {
  skill: Skill;
  /** Names of sibling skills from the same repo whose update_available was
   *  also cleared by the repo pull. */
  siblings_cleared: string[];
  /** Per-agent re-link failures after the update ("Agent: error"). The update
   *  itself succeeded; warn the user that an agent deployment may be stale. */
  agent_link_failures: string[];
}

export interface SkillUpdateFailure {
  name: string;
  error: string;
}

/** A Skill a shared channel owns, which the generic update path declines by
 *  design rather than fails on. */
export interface SkillUpdateChannelManaged {
  name: string;
  repository_id: number;
}

/** Return type of the `update_skills` batch command. `skipped` names were not
 *  pulled because their upstream no longer ships them. A failed update reports
 *  every name it would have covered, so nothing is quietly counted as done.
 *  `channel_managed` is declined-by-design, not a failure: those Skills update
 *  through the shared channel flow. */
export interface SkillUpdateReport {
  updated: UpdateResult[];
  failed: SkillUpdateFailure[];
  skipped: string[];
  channel_managed: SkillUpdateChannelManaged[];
}

export interface SkillCardDeck {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
  skill_sources: Record<string, string>;
  /** Agent ids this deck is explicitly linked to. A new deck starts empty —
   *  it never inherits the links its Skills already have. `null` only reaches
   *  the UI if the backend backfill could not resolve a pre-existing deck. */
  agent_links: string[] | null;
  created_at: string;
  updated_at: string;
}

export interface SkillContent {
  name: string;
  description: string | null;
  triggers: string[];
  scopes: string[];
  "allowed-tools": string[];
  content: string;
}

export interface FrontmatterEntry {
  key: string;
  value: string;
}

export interface SkillInstallTarget {
  id: string;
  folder_path: string;
}
