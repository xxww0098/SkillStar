/**
 * Dev-mock sample data: skills mode — installed skills, skill groups (decks),
 * registered projects. Consumed by the
 * skills fragment (../devMock/skills.ts); SAMPLE_SKILLS is also the base for
 * the marketplace fragment's MARKET_SKILLS.
 */

import type { Skill } from "../../../types";
import { iso } from "./shared";

/** Immutable template. The mutable per-session copy lives in ./skillsUpdateStore.ts. */
export const SAMPLE_SKILLS: Skill[] = [
  {
    name: "pdf-tools",
    description: "Read, merge, split, and OCR PDF files with a single command.",
    localized_description: "用一条命令读取、合并、拆分并 OCR 处理 PDF 文件。",
    skill_type: "hub",
    stars: 1284,
    installed: true,
    update_available: true,
    last_updated: iso(2),
    git_url: "https://github.com/anthropics/skills",
    tree_hash: "a1b2c3d4",
    category: "Hot",
    author: "anthropics",
    topics: ["pdf", "documents", "ocr"],
    agent_links: ["claude", "codex"],
    source: "anthropics/skills",
  },
  {
    name: "xlsx",
    description: "Create, read and edit Excel spreadsheets, charts and formulas.",
    localized_description: "创建、读取并编辑 Excel 表格、图表与公式。",
    skill_type: "hub",
    stars: 982,
    installed: true,
    update_available: false,
    // Upstream moved it into a bucket under a new name: the card offers a
    // one-step migration and the ghost card points back here.
    upstream_change: {
      kind: "removed",
      suggested_local_name: "xlsx.local",
      successor: {
        skill_id: "xlsx-tools",
        folder_path: "skills/data/xlsx-tools",
        description: "Create, read and edit Excel spreadsheets, charts, pivots and formulas.",
        similarity: 88,
      },
    },
    last_updated: iso(6),
    git_url: "https://github.com/anthropics/skills",
    tree_hash: "e5f6a7b8",
    category: "Popular",
    author: "anthropics",
    topics: ["excel", "spreadsheet", "data"],
    agent_links: ["claude"],
    source: "anthropics/skills",
  },
  {
    name: "deep-research",
    description: "Fan-out web searches, fetch sources, verify claims, synthesize a cited report.",
    localized_description: "多源网络检索、抓取来源、核验论断，产出带引用的研究报告。",
    skill_type: "hub",
    stars: 2150,
    installed: true,
    update_available: false,
    // Dropped upstream with no successor: keep a local copy or remove.
    upstream_change: { kind: "removed", suggested_local_name: "deep-research.local", successor: null },
    last_updated: iso(1),
    git_url: "https://github.com/anthropics/skills",
    tree_hash: "c9d0e1f2",
    category: "Rising",
    author: "anthropics",
    topics: ["research", "web", "agent"],
    agent_links: ["claude", "codex", "cursor"],
    source: "anthropics/skills",
  },
  {
    name: "my-prompt-pack",
    description: "A locally authored skill with my personal prompt templates.",
    localized_description: null,
    skill_type: "local",
    stars: 0,
    installed: true,
    update_available: false,
    last_updated: iso(0),
    git_url: "",
    tree_hash: null,
    category: "None",
    author: null,
    topics: ["personal"],
    agent_links: ["claude"],
  },
  {
    name: "svg2icon",
    description: "Convert an SVG into a full multi-resolution app icon set.",
    localized_description: "把一张 SVG 转换成多分辨率的完整应用图标集。",
    skill_type: "hub",
    stars: 433,
    installed: true,
    update_available: true,
    last_updated: iso(11),
    git_url: "https://github.com/community/skills",
    tree_hash: "11aa22bb",
    category: "New",
    author: "community",
    topics: ["svg", "icons", "design"],
    agent_links: [],
    source: "community/skills",
  },
];

export const DECKS = [
  {
    id: "deck-web",
    name: "Web Dev Essentials",
    description: "Everything for shipping a web app fast.",
    icon: "🌐",
    skills: ["git-flow", "sql-explain", "deep-research"],
    skill_sources: {},
    agent_links: [],
    created_at: iso(20),
    updated_at: iso(3),
  },
  {
    id: "deck-docs",
    name: "Document Toolkit",
    description: "PDF + spreadsheet automation.",
    icon: "📄",
    skills: ["pdf-tools", "xlsx"],
    skill_sources: {},
    agent_links: [],
    created_at: iso(15),
    updated_at: iso(5),
  },
];

export const PROJECTS = [
  { path: "/Users/dev/work/web-app", name: "web-app", created_at: iso(30) },
  {
    path: "/Users/dev/work/data-pipeline",
    name: "data-pipeline",
    created_at: iso(12),
  },
];
