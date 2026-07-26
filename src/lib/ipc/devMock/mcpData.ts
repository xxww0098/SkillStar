/**
 * Dev-mock sample data: MCP — the managed server store, per-tool sync
 * statuses, install presets, and the MCP marketplace (GitHub MCP Registry)
 * entries/details plus the install-form draft builder. Consumed by
 * ./mcp.ts.
 */

import { iso } from "./shared";

export const MCP_STORE = {
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
      enabled: {
        "claude-code": true,
        codex: true,
        gemini: false,
        grok: false,
        opencode: false,
        zcode: false,
      },
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
      enabled: {
        "claude-code": true,
        codex: false,
        gemini: false,
        grok: false,
        opencode: false,
        zcode: false,
      },
      sortIndex: 1,
    },
  ],
};

export const MCP_TOOL_STATUSES = [
  {
    toolId: "claude-code",
    label: "Claude Code",
    configPath: "~/.claude.json",
    installed: true,
    serverCount: 2,
  },
  {
    toolId: "codex",
    label: "Codex",
    configPath: "~/.codex/config.toml",
    installed: true,
    serverCount: 1,
  },
  {
    toolId: "gemini",
    label: "Gemini CLI",
    configPath: "~/.gemini/settings.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "grok",
    label: "Grok",
    configPath: "~/.grok/config.toml",
    installed: true,
    serverCount: 0,
  },
  {
    toolId: "opencode",
    label: "OpenCode",
    configPath: "~/.config/opencode/opencode.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "zcode",
    label: "ZCode",
    configPath: "~/.zcode/cli/config.json",
    installed: true,
    serverCount: 0,
  },
  {
    toolId: "kiro",
    label: "Kiro",
    configPath: "~/.kiro/settings/mcp.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "cursor",
    label: "Cursor",
    configPath: "~/.cursor/mcp.json",
    installed: true,
    serverCount: 0,
  },
];

export const MCP_PRESETS = [
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
export const MCP_MARKET = [
  {
    id: "adspower-local-api",
    name: "adspower-local-api",
    namespace: "adspower-local-api",
    description: "AdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化",
    repoUrl: "https://github.com/AdsPower/adspower-browser",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "skillstar-curated",
  },
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

export const MCP_MARKET_DETAILS: Record<string, Record<string, unknown>> = {
  "adspower-local-api": {
    readme: "# adspower-local-api\n\nAdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化。",
    packages: [
      {
        runtime: "npx",
        identifier: "local-api-mcp-typescript",
        version: null,
        requiredEnv: ["API_KEY"],
      },
    ],
    remotes: [],
  },
  "mkt-filesystem": {
    readme: "# server-filesystem\n\nGives the agent scoped read/write access to a local directory.",
    packages: [
      {
        runtime: "npx",
        identifier: "@modelcontextprotocol/server-filesystem",
        version: "1.2.0",
        requiredEnv: [],
      },
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
    packages: [
      {
        runtime: "uvx",
        identifier: "markitdown-mcp",
        version: "0.0.1a4",
        requiredEnv: [],
      },
    ],
    remotes: [],
  },
};

/** Build a prefilled McpServerEntry draft for the install form (dev mock). */
export function mcpMarketDraft(id: string): Record<string, unknown> {
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
    const env =
      id === "adspower-local-api"
        ? { PORT: "50325", API_KEY: "" }
        : Object.fromEntries(((pkg.requiredEnv as string[] | undefined) ?? []).map((key) => [key, ""]));
    return {
      ...base,
      transport: "stdio",
      command: pkg.runtime,
      env,
      args: [pkg.runtime === "uvx" ? `${pkg.identifier}@${pkg.version}` : "-y", `${pkg.identifier}`].filter(Boolean),
    };
  }
  if (remote) {
    return {
      ...base,
      transport: remote.transport,
      url: remote.url,
      headers: { Authorization: "Bearer {TOKEN}" },
    };
  }
  return base;
}
