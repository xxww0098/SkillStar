/**
 * DEV-ONLY browser fallback for the Tauri IPC layer.
 *
 * When the frontend runs OUTSIDE the Tauri shell (e.g. plain `vite` opened in a
 * browser for UI iteration), production rejects every `invoke()`. That makes the
 * whole app unusable in the browser. In DEV builds we instead serve realistic
 * sample data here so every screen renders populated — enabling fast visual
 * design work without a full `tauri dev` rebuild.
 *
 * This module is imported dynamically and ONLY from the `import.meta.env.DEV`
 * branch in `core.ts`, so it is dead-code-eliminated from production bundles and
 * is NEVER reachable inside the real Tauri shell. It must not be used in tests
 * (tests mock at their own layer).
 */

function iso(daysAgo = 0): string {
  // App code (not a workflow script) — Date is allowed here.
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  return d.toISOString();
}

// ── Sample data ────────────────────────────────────────────────────

const SAMPLE_SKILLS = [
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

const MARKET_SKILLS = [
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

const AGENTS = [
  ["claude", "Claude Code", "agents/claude.svg", ".claude/skills", true, true, 4],
  ["codex", "Codex CLI", "agents/codex.svg", ".codex/skills", true, true, 2],
  ["cursor", "Cursor", "agents/cursor.svg", ".cursor/skills", true, false, 1],
  ["gemini", "Gemini CLI", "agents/gemini.svg", ".gemini/skills", false, false, 0],
  ["antigravity", "Antigravity", "agents/antigravity.svg", ".agents/skills", false, false, 0],
  ["opencode", "OpenCode", "agents/opencode.svg", ".opencode/skills", true, true, 3],
  ["qoder", "Qoder", "agents/qoder-color.svg", ".qoder/skills", false, false, 0],
  ["trae", "Trae", "agents/trae-color.svg", ".trae/skills", false, false, 0],
  ["openclaw", "OpenClaw", "agents/openclaw.svg", "", false, false, 0],
  ["hermes", "Hermes", "agents/hermes.svg", ".hermes/skills", false, false, 0],
].map(([id, display_name, icon, rel, installed, enabled, synced]) => ({
  id,
  display_name,
  icon,
  global_skills_dir: `/Users/dev/${id}/skills`,
  project_skills_rel: rel,
  installed,
  enabled,
  synced_count: synced,
}));

const DECKS = [
  {
    id: "deck-web",
    name: "Web Dev Essentials",
    description: "Everything for shipping a web app fast.",
    icon: "🌐",
    skills: ["git-flow", "sql-explain", "deep-research"],
    skill_sources: {},
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
    created_at: iso(15),
    updated_at: iso(5),
  },
];

const PROJECTS = [
  { path: "/Users/dev/work/web-app", name: "web-app", created_at: iso(30) },
  { path: "/Users/dev/work/data-pipeline", name: "data-pipeline", created_at: iso(12) },
];

const FLAT_PROVIDERS = {
  version: 2,
  providers: [
    {
      id: "p-deepseek",
      name: "DeepSeek",
      base_url_openai: "https://api.deepseek.com/v1",
      base_url_anthropic: "https://api.deepseek.com/anthropic",
      models_url: "https://api.deepseek.com/v1/models",
      api_key: "sk-demo-deepseek",
      models: ["deepseek-chat", "deepseek-reasoner"],
      default_model: "deepseek-chat",
      sort_index: 0,
      preset_id: "deepseek",
      icon_color: "#4D6BFE",
      codex_wire_api: "responses",
      codex_auth_mode: "api_key",
    },
    {
      id: "p-kimi",
      name: "Kimi",
      base_url_openai: "https://api.moonshot.cn/v1",
      base_url_anthropic: "https://api.moonshot.cn/anthropic",
      models_url: "https://api.moonshot.cn/v1/models",
      api_key: "sk-demo-kimi",
      models: ["kimi-k2", "moonshot-v1-128k"],
      default_model: "kimi-k2",
      sort_index: 1,
      preset_id: "kimi",
      icon_color: "#5B45E0",
      codex_wire_api: "responses",
      codex_auth_mode: "api_key",
    },
  ],
  tool_activations: {
    "claude-code": {
      provider_id: "p-deepseek",
      model: "deepseek-chat",
      settings: null,
      last_sync_at: Math.floor(Date.now() / 1000) - 3600,
    },
    codex: {
      provider_id: "p-kimi",
      model: "kimi-k2",
      settings: { wire_api: "responses", auth_mode: "api_key" },
      last_sync_at: Math.floor(Date.now() / 1000) - 7200,
    },
  } as Record<string, unknown>,
};

const PRESETS_FLAT = [
  {
    id: "deepseek",
    name: "DeepSeek",
    category: "domestic",
    base_url_openai: "https://api.deepseek.com/v1",
    base_url_anthropic: "https://api.deepseek.com/anthropic",
    models_url: "https://api.deepseek.com/v1/models",
    models: [],
    icon_color: "#4D6BFE",
    api_key_url: "https://platform.deepseek.com/api_keys",
    balance_endpoint: "https://api.deepseek.com/user/balance",
    balance_parser: "deepseek",
  },
  {
    id: "kimi",
    name: "Kimi",
    category: "domestic",
    base_url_openai: "https://api.moonshot.cn/v1",
    base_url_anthropic: "https://api.moonshot.cn/anthropic",
    models_url: "https://api.moonshot.cn/v1/models",
    models: [],
    icon_color: "#5B45E0",
    api_key_url: "https://platform.moonshot.cn/console/api-keys",
    balance_endpoint: "https://api.moonshot.cn/v1/users/me/balance",
    balance_parser: "kimi",
  },
  {
    id: "glm",
    name: "智谱 GLM",
    category: "domestic",
    base_url_openai: "https://open.bigmodel.cn/api/paas/v4",
    base_url_anthropic: "https://open.bigmodel.cn/api/anthropic",
    models_url: "https://open.bigmodel.cn/api/paas/v4/models",
    models: [],
    icon_color: "#3366FF",
    api_key_url: "https://open.bigmodel.cn/usercenter/apikeys",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    category: "relay",
    base_url_openai: "https://openrouter.ai/api/v1",
    base_url_anthropic: "",
    models_url: "https://openrouter.ai/api/v1/models",
    models: [],
    icon_color: "#6366F1",
    api_key_url: "https://openrouter.ai/keys",
    balance_endpoint: "https://openrouter.ai/api/v1/credits",
    balance_parser: "openrouter",
  },
];

const MCP_STORE = {
  version: 1,
  servers: [
    {
      id: "mcp-fs",
      name: "filesystem",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/dev"],
      description: "Local filesystem access for the agent.",
      tags: ["files"],
      enabled: { "claude-code": true, codex: true, "claude-desktop": false, gemini: false, opencode: false },
      sortIndex: 0,
    },
    {
      id: "mcp-gh",
      name: "github",
      transport: "http",
      url: "https://api.githubcopilot.com/mcp/",
      headers: { Authorization: "Bearer ghp_demo" },
      description: "GitHub repos, issues and PRs.",
      tags: ["git", "github"],
      enabled: { "claude-code": true, codex: false, "claude-desktop": false, gemini: false, opencode: false },
      sortIndex: 1,
    },
  ],
};

const MCP_TOOL_STATUSES = [
  { toolId: "claude-code", label: "Claude Code", configPath: "~/.claude.json", installed: true, serverCount: 2 },
  {
    toolId: "claude-desktop",
    label: "Claude Desktop",
    configPath: "~/Library/.../claude_desktop_config.json",
    installed: true,
    serverCount: 0,
  },
  { toolId: "codex", label: "Codex", configPath: "~/.codex/config.toml", installed: true, serverCount: 1 },
  { toolId: "gemini", label: "Gemini CLI", configPath: "~/.gemini/settings.json", installed: false, serverCount: 0 },
  {
    toolId: "opencode",
    label: "OpenCode",
    configPath: "~/.config/opencode/opencode.json",
    installed: false,
    serverCount: 0,
  },
];

const MCP_PRESETS = [
  {
    id: "preset-fs",
    name: "filesystem",
    description: "Local filesystem access.",
    homepage: "https://github.com/modelcontextprotocol/servers",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "PATH"],
    tags: ["files"],
    requiredEnv: [],
  },
  {
    id: "preset-gh",
    name: "github",
    description: "GitHub repos, issues and PRs.",
    homepage: "https://github.com/github/github-mcp-server",
    transport: "http",
    url: "https://api.githubcopilot.com/mcp/",
    tags: ["github"],
    requiredEnv: ["GITHUB_TOKEN"],
  },
  {
    id: "preset-pg",
    name: "postgres",
    description: "Query a PostgreSQL database.",
    homepage: "https://github.com/modelcontextprotocol/servers",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-postgres"],
    tags: ["database"],
    requiredEnv: ["DATABASE_URL"],
  },
];

// MCP marketplace (GitHub MCP Registry) sample data for browser dev mode.
const MCP_MARKET = [
  {
    id: "mkt-filesystem",
    name: "server-filesystem",
    namespace: "io.github.modelcontextprotocol/server-filesystem",
    description: "Local filesystem access — read, write and search files.",
    repoUrl: "https://github.com/modelcontextprotocol/servers",
    stars: 18400,
    license: "MIT",
    version: "1.2.0",
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(2),
  },
  {
    id: "mkt-github",
    name: "github-mcp-server",
    namespace: "io.github.github/github-mcp-server",
    description: "GitHub repositories, issues and pull requests via the official server.",
    repoUrl: "https://github.com/github/github-mcp-server",
    stars: 9200,
    license: "MIT",
    version: "0.5.0",
    kind: "remote",
    runtimes: [],
    updatedAt: iso(1),
  },
  {
    id: "mkt-markitdown",
    name: "markitdown",
    namespace: "microsoft/markitdown",
    description: "Convert PDF, Word, Excel, images and audio to Markdown.",
    repoUrl: "https://github.com/microsoft/markitdown",
    stars: 33000,
    license: "MIT",
    version: "0.0.1a4",
    kind: "stdio",
    runtimes: ["uvx"],
    updatedAt: iso(5),
  },
];

const MCP_MARKET_DETAILS: Record<string, Record<string, unknown>> = {
  "mkt-filesystem": {
    readme: "# server-filesystem\n\nGives the agent scoped read/write access to a local directory.",
    packages: [
      { runtime: "npx", identifier: "@modelcontextprotocol/server-filesystem", version: "1.2.0", requiredEnv: [] },
    ],
    remotes: [],
  },
  "mkt-github": {
    readme: "# github-mcp-server\n\nRemote MCP server hosted by GitHub.",
    packages: [],
    remotes: [
      {
        transport: "http",
        url: "https://api.githubcopilot.com/mcp/",
        requiredHeaders: ["Authorization"],
      },
    ],
  },
  "mkt-markitdown": {
    readme: "# markitdown\n\nConvert many file formats to Markdown.",
    packages: [{ runtime: "uvx", identifier: "markitdown-mcp", version: "0.0.1a4", requiredEnv: [] }],
    remotes: [],
  },
};

/** Build a prefilled McpServerEntry draft for the install form (dev mock). */
function mcpMarketDraft(id: string): Record<string, unknown> {
  const detail = MCP_MARKET_DETAILS[id];
  const entry = MCP_MARKET.find((m) => m.id === id);
  const base = {
    id: "",
    name: entry?.name ?? "mcp-server",
    transport: "stdio",
    args: [] as string[],
    env: {} as Record<string, string>,
    headers: {} as Record<string, string>,
    description: entry?.description,
    homepage: entry?.repoUrl,
    tags: [] as string[],
    enabled: {},
    sortIndex: 0,
  };
  const pkg = (detail?.packages as Array<Record<string, unknown>>)?.[0];
  const remote = (detail?.remotes as Array<Record<string, unknown>>)?.[0];
  if (pkg) {
    return {
      ...base,
      transport: "stdio",
      command: pkg.runtime,
      args: [pkg.runtime === "uvx" ? `${pkg.identifier}@${pkg.version}` : "-y", `${pkg.identifier}`].filter(Boolean),
    };
  }
  if (remote) {
    return { ...base, transport: remote.transport, url: remote.url, headers: { Authorization: "Bearer {TOKEN}" } };
  }
  return base;
}

const AI_CONFIG = {
  enabled: true,
  api_format: "openai" as const,
  provider_ref: null,
  base_url: "https://api.deepseek.com/v1",
  api_key: "sk-demo-ai",
  model: "deepseek-chat",
  target_language: "zh-CN",
  context_window_k: 128,
  max_concurrent_requests: 4,
  openai_preset: { base_url: "https://api.deepseek.com/v1", api_key: "sk-demo", model: "deepseek-chat" },
  anthropic_preset: { base_url: "https://api.anthropic.com", api_key: "", model: "claude-sonnet-4-6" },
  local_preset: { base_url: "http://localhost:11434/v1", api_key: "", model: "qwen2.5" },
};

const STORAGE_OVERVIEW = {
  data_root_path: "/Users/dev/.skillstar",
  hub_root_path: "/Users/dev/.skillstar/hub",
  is_hub_under_data: true,
  config_bytes: 245_000,
  config_path: "/Users/dev/.skillstar/config",
  hub_bytes: 18_400_000,
  hub_path: "/Users/dev/.skillstar/hub",
  hub_count: 5,
  broken_count: 0,
  local_count: 1,
  local_bytes: 120_000,
  local_path: "/Users/dev/.skillstar/local",
  cache_bytes: 64_200_000,
  cache_path: "/Users/dev/.skillstar/cache",
  cache_count: 8,
  cache_unused_count: 2,
  cache_unused_bytes: 9_800_000,
  history_count: 14,
};

// ── Usage mode (subscriptions / quota tracker) ──
const nowSec = () => Math.floor(Date.now() / 1000);
const days = (n: number) => n * 86_400;

const USAGE_CATALOG = [
  {
    id: "cursor",
    display_name: "Cursor",
    description: "AI Code Editor",
    tier: "o-auth",
    auth_modes: ["o-auth", "manual"],
    brand_color: "00E5BC",
    default_currency: "USD",
    subscription_url: "https://cursor.com/settings",
    warning: null,
    regions: [],
  },
  {
    id: "codex",
    display_name: "Codex",
    description: "OpenAI Codex CLI",
    tier: "o-auth",
    auth_modes: ["o-auth", "manual"],
    brand_color: "10A37F",
    default_currency: "USD",
    subscription_url: "https://chat.openai.com/codex",
    warning: null,
    regions: [],
  },
  {
    id: "xai",
    display_name: "Grok",
    description: "xAI Grok CLI",
    tier: "o-auth",
    auth_modes: ["o-auth", "manual"],
    brand_color: "111111",
    default_currency: "USD",
    subscription_url: "https://x.ai",
    warning: null,
    regions: [],
  },
  {
    id: "deepseek",
    display_name: "DeepSeek",
    description: "API Key 余额",
    tier: "api-key",
    auth_modes: ["api-key", "manual"],
    brand_color: "1A56DB",
    default_currency: "CNY",
    subscription_url: "https://platform.deepseek.com/usage",
    warning: null,
    regions: [],
  },
  {
    id: "kimi",
    display_name: "Kimi",
    description: "Moonshot",
    tier: "api-key",
    auth_modes: ["api-key", "manual"],
    brand_color: "F5B400",
    default_currency: "CNY",
    subscription_url: "https://platform.moonshot.cn",
    warning: null,
    regions: [],
  },
  {
    id: "glm",
    display_name: "智谱 GLM",
    description: "Coding Plan",
    tier: "api-key",
    auth_modes: ["api-key", "manual"],
    brand_color: "4A90E2",
    default_currency: "CNY",
    subscription_url: "https://bigmodel.cn/usercenter/order",
    warning: null,
    regions: [],
  },
  {
    id: "opencode",
    display_name: "OpenCode",
    description: "$10/月 Go 订阅 · Zen 按量付费",
    tier: "cookie",
    auth_modes: ["cookie", "manual"],
    brand_color: "6366F1",
    default_currency: "USD",
    subscription_url: "https://opencode.ai/workspace",
    warning: null,
    regions: [],
  },
];

function sub(over: Record<string, unknown>): Record<string, unknown> {
  return {
    plan_tier: null,
    monthly_price: null,
    currency: "USD",
    billing_cycle: "monthly",
    start_date: nowSec() - days(40),
    renew_date: nowSec() + days(12),
    auto_renew: true,
    has_credential: true,
    requires_reauth: false,
    is_active: true,
    manual_quota: null,
    note: null,
    sort_index: 0,
    created_at: nowSec() - days(40),
    updated_at: nowSec() - days(1),
    usage: null,
    ...over,
  };
}

const USAGE_SUBSCRIPTIONS = [
  sub({
    id: "sub-cursor",
    catalog_id: "cursor",
    display_name: "Cursor Pro",
    auth_mode: "o-auth",
    plan_tier: "PRO",
    monthly_price: 20,
    currency: "USD",
    renew_date: nowSec() + days(3),
    sort_index: 0,
    usage: {
      subscription_id: "sub-cursor",
      fetched_at: nowSec() - 1800,
      plan_name: "PRO",
      hourly: null,
      weekly: { label: "本周", used: 412, total: 500, percent: 82, reset_at: nowSec() + days(4), breakdown: [] },
      monthly: null,
      balance: null,
      credits: [],
      error: null,
      api_keys: [],
    },
  }),
  sub({
    id: "sub-codex",
    catalog_id: "codex",
    display_name: "ChatGPT Plus · Codex",
    auth_mode: "o-auth",
    plan_tier: "PLUS",
    monthly_price: 20,
    currency: "USD",
    sort_index: 1,
    usage: {
      subscription_id: "sub-codex",
      fetched_at: nowSec() - 3600,
      plan_name: "PLUS",
      hourly: { label: "5h", used: 38, total: 150, percent: 25, reset_at: nowSec() + 9000, breakdown: [] },
      weekly: { label: "本周", used: 1240, total: 4000, percent: 31, reset_at: nowSec() + days(5), breakdown: [] },
      monthly: null,
      balance: null,
      credits: [],
      error: null,
      api_keys: [],
    },
  }),
  sub({
    id: "sub-xai",
    catalog_id: "xai",
    display_name: "Grok",
    auth_mode: "o-auth",
    plan_tier: "Grok",
    monthly_price: 20,
    currency: "USD",
    sort_index: 2,
    usage: {
      subscription_id: "sub-xai",
      fetched_at: nowSec() - 900,
      plan_name: "Grok",
      hourly: null,
      weekly: null,
      monthly: {
        label: "Monthly credits",
        used: 1260,
        total: 5000,
        percent: 25,
        reset_at: nowSec() + days(18),
        breakdown: [],
      },
      balance: null,
      credits: [
        {
          credit_type: "Pay as you go cap",
          credit_amount: "$20",
          minimum_credit_amount_for_usage: null,
        },
      ],
      error: null,
      api_keys: [],
    },
  }),
  sub({
    id: "sub-deepseek",
    catalog_id: "deepseek",
    display_name: "DeepSeek 余额",
    auth_mode: "api-key",
    plan_tier: "PAYG",
    currency: "CNY",
    billing_cycle: "one-time",
    sort_index: 3,
    usage: {
      subscription_id: "sub-deepseek",
      fetched_at: nowSec() - 600,
      plan_name: "PAYG",
      hourly: null,
      weekly: null,
      monthly: null,
      balance: { currency: "CNY", total: 48.5, granted: 5, topped_up: 43.5 },
      credits: [],
      error: null,
      api_keys: [],
    },
  }),
  sub({
    id: "sub-glm",
    catalog_id: "glm",
    display_name: "GLM Coding Plan",
    auth_mode: "api-key",
    plan_tier: "pro",
    currency: "CNY",
    requires_reauth: true,
    sort_index: 4,
    usage: {
      subscription_id: "sub-glm",
      fetched_at: nowSec() - 7200,
      plan_name: "pro",
      hourly: { label: "5h", used: 90, total: 100, percent: 90, reset_at: nowSec() + 7200, breakdown: [] },
      weekly: { label: "7d", used: 620, total: 1000, percent: 62, reset_at: nowSec() + days(3), breakdown: [] },
      monthly: null,
      balance: null,
      credits: [],
      error: "GLM 需要重新登录（凭证已过期）",
      api_keys: [],
    },
  }),
];

const USAGE_ALERTS = [
  {
    id: "al-1",
    subscription_id: "sub-cursor",
    severity: "warning",
    kind: "renew-soon",
    message: "Cursor Pro 将在 3 天后续费",
  },
  {
    id: "al-2",
    subscription_id: "sub-glm",
    severity: "danger",
    kind: "quota-critical",
    message: "GLM 5 小时额度已用 90%",
  },
  { id: "al-3", subscription_id: "sub-glm", severity: "warning", kind: "needs-reauth", message: "GLM 需要重新登录" },
];

const USAGE_SUMMARY = {
  monthly_spend: [{ currency: "USD", amount: 40 }],
  total_subscriptions: USAGE_SUBSCRIPTIONS.length,
  alert_count: USAGE_ALERTS.length,
  reauth_count: 1,
};

const HANDLERS: Record<string, (args: Record<string, unknown>) => unknown> = {
  // ── Global / app shell ──
  list_agent_profiles: () => AGENTS,
  toggle_agent_profile: () => true,
  get_patrol_status: () => ({ enabled: true, running: true, interval_secs: 3600, last_check: iso(0) }),
  set_patrol_enabled: () => undefined,
  check_developer_mode: () => true,
  check_app_update: () => ({ available: false, version: null, date: null, body: null }),

  // ── Skills mode ──
  list_skills: () => SAMPLE_SKILLS,
  refresh_skill_updates: () =>
    SAMPLE_SKILLS.filter((s) => s.update_available).map((s) => ({ name: s.name, update_available: true })),
  check_new_repo_skills: () => [],
  get_dismissed_new_skills: () => [],
  read_skill_content: (args) => ({
    name: String((args?.name as string) ?? "pdf-tools"),
    description: "Read, merge, split, and OCR PDF files with a single command.",
    triggers: ["pdf", "ocr", "merge pdf"],
    scopes: ["files"],
    "allowed-tools": ["Bash", "Read"],
    content:
      "# PDF Tools\n\nA skill for working with **PDF** files — read, merge, split and OCR.\n\n## Features\n\n- Merge & split documents\n- OCR scanned pages\n- Fill interactive forms\n\n```bash\nskillstar run pdf-tools merge a.pdf b.pdf -o out.pdf\n```\n\n> Tip: pair with the `xlsx` skill for spreadsheet exports. See the [docs](https://example.com).",
  }),
  read_skill_file_raw: (args) =>
    `---\nname: ${String((args?.name as string) ?? "pdf-tools")}\ndescription: Read, merge, split, and OCR PDF files.\n---\n\n# PDF Tools\n\nA skill for working with **PDF** files.`,
  list_skill_files: () => ["SKILL.md", "scripts/merge.py", "README.md"],
  list_skill_groups: () => DECKS,
  list_projects: () => PROJECTS,
  get_project_skills: () => ({ agents: { claude: ["pdf-tools", "xlsx"] }, updated_at: iso(1) }),
  detect_project_agents: () => ({
    detected: AGENTS.filter((a) => a.project_skills_rel).map((a) => ({
      agent_id: a.id,
      display_name: a.display_name,
      icon: a.icon,
      project_skills_rel: a.project_skills_rel,
      exists: a.enabled,
    })),
    ambiguous_groups: [],
    auto_enable: ["claude"],
  }),

  // ── Marketplace ──
  list_marketplace_skills_local: () => ({ data: MARKET_SKILLS, snapshot_status: "fresh", snapshot_updated_at: iso(0) }),
  get_leaderboard_local: () => ({ data: MARKET_SKILLS, snapshot_status: "fresh", snapshot_updated_at: iso(0) }),
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

  // ── MCP ──
  list_mcp_servers: () => MCP_STORE,
  mcp_tool_statuses: () => MCP_TOOL_STATUSES,
  get_mcp_presets: () => MCP_PRESETS,

  // MCP marketplace (GitHub MCP Registry)
  list_mcp_market_servers_local: () => ({
    data: MCP_MARKET,
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  search_mcp_market_local: (args) => {
    const q = String((args?.query as string) ?? "").toLowerCase();
    const data = q
      ? MCP_MARKET.filter((m) => m.name.toLowerCase().includes(q) || m.description.toLowerCase().includes(q))
      : MCP_MARKET;
    return { data, snapshot_status: "fresh", snapshot_updated_at: iso(0) };
  },
  get_mcp_market_server_detail_local: (args) => {
    const id = String((args?.id as string) ?? "");
    const entry = MCP_MARKET.find((m) => m.id === id);
    const detail = MCP_MARKET_DETAILS[id];
    return {
      data: entry && detail ? { ...entry, ...detail } : null,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  sync_mcp_market_scope: () => undefined,
  get_mcp_market_sync_states: () => [
    {
      scope: "mcp_registry",
      last_success_at: iso(0),
      last_attempt_at: iso(0),
      last_error: null,
      next_refresh_at: iso(-0.5),
      schema_version: 8,
    },
  ],
  mcp_market_entry_to_draft: (args) => mcpMarketDraft(String((args?.id as string) ?? "")),

  // ── Models ──
  get_providers_flat: () => FLAT_PROVIDERS,
  get_tool_activations: () => FLAT_PROVIDERS.tool_activations,
  get_provider_presets_flat: () => PRESETS_FLAT,
  get_provider_presets: () =>
    PRESETS_FLAT.map((p) => ({
      id: p.id,
      name: p.name,
      base_url: p.base_url_openai,
      api_key_url: p.api_key_url ?? "",
      icon_color: p.icon_color,
      models: p.models,
    })),
  detect_env_conflicts: () => [],
  detect_provider_conflicts: () => [],
  get_tool_config_targets: () => [],
  detect_tool_installation: () => ({ installed: true, binary_found: true, config_dir_found: true }),
  list_tool_config_files: (args) => {
    const tool = String((args?.toolId as string) ?? "claude-code");
    const isCodex = tool === "codex";
    return [
      {
        file_id: "main",
        label: isCodex ? "config.toml" : "settings.json",
        path: isCodex ? "~/.codex/config.toml" : `~/.${tool}/settings.json`,
        format: isCodex ? "toml" : "json",
        exists: true,
        managed_by_skillstar: true,
      },
    ];
  },
  read_tool_config_file: () => '{\n  "// demo": "sample tool config (browser dev mock)"\n}',

  // ── AI config ──
  get_ai_config: () => AI_CONFIG,
  ai_test_connection: () => 220,

  // ── Settings / system ──
  get_proxy_config: () => ({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 7890,
    username: null,
    password: null,
    bypass: null,
  }),
  get_github_mirror_config: () => ({ enabled: false, preset_id: "ghproxy_vip", custom_url: null }),
  get_github_mirror_presets: () => [
    { id: "ghproxy_vip", name: "ghproxy.link", url: "https://ghproxy.link/", supports_clone: true },
    { id: "gh_proxy", name: "gh-proxy.com", url: "https://gh-proxy.com/", supports_clone: true },
  ],
  get_acp_config: () => ({ enabled: false, agent_command: "", agent_label: "" }),
  get_storage_overview: () => STORAGE_OVERVIEW,
  get_repo_cache_info: () => ({ total_bytes: 64_200_000, repo_count: 8, unused_count: 2, unused_bytes: 9_800_000 }),

  // ── GitHub ──
  check_gh_installed: () => true,
  check_gh_status: () => ({ status: "Ready", username: "dev-user" }),
  check_git_status: () => ({ status: "Installed", version: "2.45.0" }),
  list_repo_history: () => [],
  list_user_repos: () => [],

  // ── Usage mode (subscriptions / quota) ──
  list_usage_catalog: () => USAGE_CATALOG,
  list_subscriptions: () => USAGE_SUBSCRIPTIONS,
  get_active_subscriptions: () => ({
    cursor: "sub-cursor",
    codex: "sub-codex",
    deepseek: "sub-deepseek",
    glm: "sub-glm",
  }),
  get_subscription_alerts: () => USAGE_ALERTS,
  get_usage_summary: () => USAGE_SUMMARY,
  get_cookie_bridge_binding_status: (args) => ({
    provider: String((args?.provider as string) ?? ""),
    bound: false,
    subscription_id: null,
    updated_at: null,
  }),
  refresh_all_subscriptions: () => USAGE_SUBSCRIPTIONS.map((s) => s.usage).filter(Boolean),
  refresh_subscription_usage: (args) => USAGE_SUBSCRIPTIONS.find((s) => s.id === args?.id)?.usage ?? null,
  get_subscription_api_key: () => "sk-demo-********",
};

/**
 * Resolve a mocked command. Known commands return realistic sample data; unknown
 * commands resolve `undefined` (rather than rejecting) so unmocked reads degrade
 * to empty state and void mutations no-op, without flooding the console.
 */
export async function devInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Small delay so loading skeletons are exercised during UI iteration.
  await new Promise((r) => setTimeout(r, 90));
  const handler = HANDLERS[command];
  if (handler) {
    return handler(args ?? {}) as T;
  }
  // Unmocked commands resolve to an empty array — safe for the dominant
  // "list" read pattern (`.length` / `.map` / `for..of`) so unmocked screens
  // degrade to empty state instead of crashing. Object-returning commands that
  // need a richer shape are mocked explicitly above.
  if (import.meta.env.DEV) {
    // eslint-disable-next-line no-console
    console.debug(`[devMock] unmocked command "${command}" → []`, args ?? {});
  }
  return [] as unknown as T;
}
