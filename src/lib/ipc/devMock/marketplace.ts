/**
 * Dev-mock fragment: skills marketplace — local-first marketplace lists,
 * leaderboard, publishers, and search. MARKET_SKILLS derives from the skills
 * fragment's SAMPLE_SKILLS (same rows, uninstalled + ranked) plus a few
 * marketplace-only entries.
 */

import { type DevMockHandlers, iso } from "./shared";
import { SAMPLE_SKILLS } from "./skillsData";

export const MARKET_SKILLS = [
  ...SAMPLE_SKILLS.map((s, i) => ({ ...s, installed: false, rank: i + 1 })),
  {
    name: "git-flow",
    description: "Opinionated git workflow helper: branch, commit, PR, release.",
    localized_description: "一套有主张的 git 工作流助手：分支、提交、PR、发布。",
    skill_type: "hub",
    stars: 766,
    installed: false,
    update_available: false,
    last_updated: iso(4),
    git_url: "https://github.com/community/skills",
    tree_hash: "ab12cd34",
    category: "Popular",
    author: "community",
    topics: ["git", "workflow"],
    rank: 6,
    source: "community/skills",
  },
  {
    name: "sql-explain",
    description: "Explain, optimize, and lint SQL queries across dialects.",
    localized_description: "跨方言解释、优化并检查 SQL 查询。",
    skill_type: "hub",
    stars: 540,
    installed: false,
    update_available: false,
    last_updated: iso(9),
    git_url: "https://github.com/data-tools/skills",
    tree_hash: "ff00aa11",
    category: "Rising",
    author: "data-tools",
    topics: ["sql", "database"],
    rank: 7,
    source: "data-tools/skills",
  },
];

export const MARKETPLACE_HANDLERS: DevMockHandlers = {
  list_marketplace_skills_local: () => ({
    data: MARKET_SKILLS,
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  get_leaderboard_local: () => ({
    data: MARKET_SKILLS,
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  get_publishers_local: () => ({
    data: [
      {
        name: "anthropics",
        repo: "anthropics/skills",
        repo_count: 3,
        skill_count: 24,
        url: "https://github.com/anthropics/skills",
      },
      {
        name: "community",
        repo: "community/skills",
        repo_count: 5,
        skill_count: 41,
        url: "https://github.com/community/skills",
      },
    ],
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  get_marketplace_sync_states: () => [],
  search_marketplace_local: (args) => {
    const q = String((args?.query as string) ?? "").toLowerCase();
    return {
      data: MARKET_SKILLS.filter((s) => s.name.includes(q) || s.description.toLowerCase().includes(q)),
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
};
