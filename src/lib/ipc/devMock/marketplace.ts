/**
 * Dev-mock fragment: skills marketplace — local-first marketplace lists,
 * leaderboard, publishers, and search. MARKET_SKILLS derives from the skills
 * fragment's SAMPLE_SKILLS (same rows, uninstalled + ranked) plus a few
 * marketplace-only entries.
 */

import { type DevMockHandlers, iso } from "./shared";
import { SAMPLE_SKILLS } from "./skillsData";

const PUBLISHER_REPOS: Record<
  string,
  Array<{
    repo: string;
    source: string;
    skill_count: number;
    installs_label: string;
    installs: number;
    url: string;
    skills: Array<{ name: string; installs: number }>;
  }>
> = {
  anthropics: [
    {
      repo: "skills",
      source: "anthropics/skills",
      skill_count: 3,
      installs_label: "4.4K",
      installs: 4400,
      url: "https://github.com/anthropics/skills",
      skills: [
        { name: "pdf-tools", installs: 1300 },
        { name: "xlsx", installs: 982 },
        { name: "deep-research", installs: 2100 },
      ],
    },
    {
      repo: "computer-use",
      source: "anthropics/computer-use",
      skill_count: 0,
      installs_label: "120",
      installs: 120,
      url: "https://github.com/anthropics/computer-use",
      skills: [],
    },
    {
      repo: "examples",
      source: "anthropics/examples",
      skill_count: 0,
      installs_label: "80",
      installs: 80,
      url: "https://github.com/anthropics/examples",
      skills: [],
    },
  ],
  community: [
    {
      repo: "skills",
      source: "community/skills",
      skill_count: 2,
      installs_label: "1.2K",
      installs: 1200,
      url: "https://github.com/community/skills",
      skills: [
        { name: "svg2icon", installs: 433 },
        { name: "git-flow", installs: 766 },
      ],
    },
  ],
};

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
  get_publisher_repos_local: (args) => {
    const name = String((args?.publisherName as string) ?? "").toLowerCase();
    const repos = PUBLISHER_REPOS[name] ?? [];
    return {
      data: repos,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  get_repo_skills_local: (args) => {
    const source = String((args?.source as string) ?? "").toLowerCase();
    const data = MARKET_SKILLS.filter((s) => (s.source ?? "").toLowerCase() === source);
    return {
      data,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  sync_marketplace_scope: () => undefined,
};
