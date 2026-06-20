//! skill domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export type SkillCategory = "Hot" | "Popular" | "Rising" | "New" | "None";

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
