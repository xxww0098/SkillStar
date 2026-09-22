/**
 * Dev-mock sample data: MCP — the managed server store, per-tool sync
 * statuses, install presets, the catalog sources and their per-source sync
 * state, plus the health-probe and paste-parse stand-ins. Marketplace catalog
 * data and the install-plan builders live in ./mcpMarketData.ts. Consumed by
 * ./mcp.ts.
 */

import { iso } from "./shared";

type MockRecord = Record<string, unknown>;

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
    toolId: "claude-desktop-chat",
    label: "Claude Desktop",
    configPath: "~/Library/Application Support/Claude/claude_desktop_config.json",
    installed: true,
    serverCount: 1,
  },
  {
    toolId: "codex",
    label: "Codex",
    configPath: "~/.codex/config.toml",
    installed: true,
    serverCount: 1,
  },
  {
    toolId: "grok",
    label: "Grok",
    configPath: "~/.grok/config.toml",
    installed: true,
    serverCount: 0,
  },
  {
    toolId: "deepseek",
    label: "DeepSeek Harness",
    configPath: "~/.dsh/cordis.patch.yml",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "hermes",
    label: "Hermes Agent",
    configPath: "~/.hermes/config.yaml",
    installed: false,
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
  {
    toolId: "vscode",
    label: "VS Code",
    configPath: "~/.copilot/mcp-config.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "windsurf",
    label: "Windsurf",
    configPath: "~/.codeium/windsurf/mcp_config.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "cline",
    label: "Cline",
    configPath: "~/.cline/mcp.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "gemini-cli",
    label: "Gemini CLI",
    configPath: "~/.gemini/settings.json",
    installed: true,
    serverCount: 0,
  },
  {
    toolId: "antigravity",
    label: "Antigravity",
    configPath: "~/.gemini/config/mcp_config.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "zed",
    label: "Zed",
    configPath: "~/.config/zed/settings.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "workbuddy",
    label: "WorkBuddy",
    configPath: "~/.workbuddy/mcp.json",
    installed: false,
    serverCount: 0,
  },
  {
    toolId: "devin",
    label: "Devin",
    configPath: "~/.config/devin/mcp_config.json",
    installed: false,
    serverCount: 0,
  },
];

// Curated-derived presets: id *is* the catalog row id, and `catalogId` routes
// the chip to the install wizard instead of the create form. That is what a
// fresh snapshot produces for every curated row; the built-in floor (no
// catalogId, form path) only shows when the snapshot is unreadable.
const curatedPreset = (id: string, description: string, homepage: string, launch: Record<string, unknown>) => ({
  id,
  catalogId: id,
  name: id,
  description,
  homepage,
  tags: ["recommended"],
  requiredEnv: [],
  ...launch,
});

export const MCP_PRESETS = [
  curatedPreset(
    "filesystem",
    "官方文件系统 MCP — 读写本地文件与目录。",
    "https://github.com/modelcontextprotocol/servers",
    { transport: "stdio", command: "npx", args: ["-y", "@modelcontextprotocol/server-filesystem"] },
  ),
  curatedPreset("git", "官方 Git MCP — 本地仓库操作（uvx 运行）。", "https://github.com/modelcontextprotocol/servers", {
    transport: "stdio",
    command: "uvx",
    args: ["mcp-server-git"],
  }),
  curatedPreset(
    "github",
    "GitHub 官方远程 MCP — 仓库、issue、PR 与代码搜索。",
    "https://github.com/github/github-mcp-server",
    { transport: "http", url: "https://api.githubcopilot.com/mcp/" },
  ),
  curatedPreset("context7", "Context7 — 为 AI 提供最新版库/框架文档上下文。", "https://github.com/upstash/context7", {
    transport: "stdio",
    command: "npx",
    args: ["-y", "@upstash/context7-mcp"],
  }),
  curatedPreset(
    "deepwiki",
    "DeepWiki — 读取公开 GitHub 仓库的 AI 文档并回答仓库内问题。",
    "https://docs.devin.ai/work-with-devin/deepwiki-mcp",
    { transport: "http", url: "https://mcp.deepwiki.com/mcp" },
  ),
  curatedPreset("codegraph", "CodeGraph — 本地代码知识图谱。", "https://github.com/colbymchenry/codegraph", {
    transport: "stdio",
    command: "npx",
    args: ["-y", "@colbymchenry/codegraph", "serve", "--mcp"],
  }),
  curatedPreset("serena", "Serena — 基于 LSP 的语义代码工具。", "https://github.com/oraios/serena", {
    transport: "stdio",
    command: "uvx",
    args: ["--from", "git+https://github.com/oraios/serena", "serena", "start-mcp-server", "--project-from-cwd"],
  }),
  curatedPreset(
    "playwright",
    "Playwright — 浏览器自动化与端到端验证。",
    "https://github.com/microsoft/playwright-mcp",
    { transport: "stdio", command: "npx", args: ["-y", "@playwright/mcp@latest"] },
  ),
  curatedPreset(
    "chrome-devtools",
    "Chrome DevTools — 调试、性能/网络抓取与 DOM/控制台检查。",
    "https://github.com/ChromeDevTools/chrome-devtools-mcp",
    { transport: "stdio", command: "npx", args: ["-y", "chrome-devtools-mcp@latest"] },
  ),
  curatedPreset("figma", "Figma 官方 MCP — 设计到代码。", "https://www.figma.com/", {
    transport: "http",
    url: "https://mcp.figma.com/mcp",
  }),
  curatedPreset("blender", "Blender MCP — 建模、渲染与场景脚本驱动。", "https://github.com/ahujasid/blender-mcp", {
    transport: "stdio",
    command: "uvx",
    args: ["blender-mcp"],
  }),
  curatedPreset("photoshop", "Photoshop MCP — 文档操作与生成式填充。", "https://github.com/alisaitteke/photoshop-mcp", {
    transport: "stdio",
    command: "npx",
    args: ["-y", "@alisaitteke/photoshop-mcp"],
  }),
  curatedPreset("after-effects", "After Effects 连接器 — 合成、图层与关键帧。", "https://oneprism.io", {
    transport: "http",
    url: "https://live.oneprism.io/after-effects",
  }),
];

// ---------------------------------------------------------------------------
// Catalog sources
// ---------------------------------------------------------------------------

/** `McpSourceDescriptor[]` — built-ins plus one user-added source. */
export const MCP_SOURCES = [
  {
    id: "official",
    displayName: "Official MCP Registry",
    baseUrl: "https://registry.modelcontextprotocol.io/v0.1/servers",
    kind: "registry",
    cursorStyle: "camel",
    listQuery: "version=latest",
    requiresKey: false,
    license: "cc0",
    mirrorable: true,
    enabled: true,
    builtin: true,
    priority: 0,
    maxPages: 400,
  },
  {
    id: "github",
    displayName: "GitHub MCP Registry",
    baseUrl: "https://api.mcp.github.com/v0.1/servers",
    kind: "registry",
    cursorStyle: "snake",
    listQuery: null,
    requiresKey: false,
    license: "unspecified",
    mirrorable: false,
    enabled: true,
    builtin: true,
    priority: 10,
    maxPages: 50,
  },
  {
    id: "custom:acme",
    displayName: "Acme internal registry",
    baseUrl: "https://mcp.acme.internal/v0.1/servers",
    kind: "registry",
    cursorStyle: "camel",
    listQuery: null,
    requiresKey: false,
    license: "userProvided",
    mirrorable: true,
    enabled: true,
    builtin: false,
    priority: 50,
    maxPages: 50,
  },
];

/**
 * `SyncStateEntry[]`, one per source. `custom:acme` is deliberately failing and
 * `github` deliberately degraded, so the "this sync was incomplete, because X"
 * UI has something to render in browser dev.
 */
export const MCP_SOURCE_SYNC_STATES = [
  {
    scope: "mcp_registry:official",
    last_success_at: iso(0),
    last_attempt_at: iso(0),
    last_error: null,
    next_refresh_at: iso(-0.5),
    schema_version: 13,
    source_host: "registry.modelcontextprotocol.io",
    payload_sha256: "0f1e2d3c",
    etag: 'W/"official-1"',
    degraded_reason: null,
  },
  {
    scope: "mcp_registry:github",
    last_success_at: iso(0.2),
    last_attempt_at: iso(0),
    last_error: null,
    next_refresh_at: iso(-0.5),
    schema_version: 13,
    source_host: "api.mcp.github.com",
    payload_sha256: "9a8b7c6d",
    etag: 'W/"github-1"',
    degraded_reason: "github stopped after 50 pages (rate limit); the mirror's contribution is partial",
  },
  {
    scope: "mcp_registry:custom:acme",
    last_success_at: iso(3),
    last_attempt_at: iso(0),
    last_error: "connect ECONNREFUSED mcp.acme.internal:443",
    next_refresh_at: iso(2.5),
    schema_version: 13,
    source_host: "mcp.acme.internal",
    payload_sha256: null,
    etag: null,
    degraded_reason: null,
  },
];

/** `McpProbeReport` for an installed server. */
export function mcpProbeReport(id: string): MockRecord {
  const server = MCP_STORE.servers.find((s) => s.id === id);
  const remote = server?.transport !== "stdio";
  return {
    serverId: id,
    serverName: server?.name ?? id,
    // `McpProbeStatus` is kebab-case on the wire (`#[serde(rename_all =
    // "kebab-case")]`); the camelCase spelling renders as an unknown status.
    status: remote ? "authorization-required" : "healthy",
    epoch: remote ? null : "modern",
    protocolVersion: remote ? null : "2026-07-28",
    tools: remote ? [] : ["read_file", "write_file", "list_directory"],
    instructions: remote ? null : "Read and write files under the configured root.",
    cacheTtlMs: remote ? null : 60_000,
    cachePrivate: false,
    authChallenge: remote
      ? 'Bearer resource_metadata="https://api.githubcopilot.com/.well-known/oauth-protected-resource"'
      : null,
    schemaBytes: remote ? null : 78,
    schemaTokens: remote ? null : 20,
    error: null,
    checkedAt: Date.now(),
  };
}

/** Browser-dev stand-in for `parse_mcp_paste`. Mirrors the Rust dialects enough for the fleet page. */
export function parseMcpPaste(text: string): MockRecord {
  const trimmed = text.trim();
  if (!trimmed) return { kind: "empty", drafts: [], warnings: [] };
  const catalogMatch = /(?:^|[?&])catalog=([^&]+)/i.exec(trimmed);
  if (catalogMatch) {
    return { kind: "catalog", drafts: [], catalogId: decodeURIComponent(catalogMatch[1]), warnings: [] };
  }
  const urlMatch = /(?:^|[?&])url=([^&]+)/i.exec(trimmed);
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://") || urlMatch) {
    const url = trimmed.startsWith("http") ? trimmed : decodeURIComponent(urlMatch?.[1] ?? "");
    return {
      kind: trimmed.includes("skillstar://") ? "deep-link" : "url",
      drafts: [
        { id: "", name: "imported", transport: "http", url, args: [], env: {}, headers: {}, tags: [], enabled: {} },
      ],
      warnings: [],
    };
  }
  if (trimmed.includes("mcpServers") || trimmed.includes('"servers"')) {
    return {
      kind: "json-servers",
      drafts: [
        {
          id: "",
          name: "github",
          transport: "stdio",
          command: "npx",
          args: ["-y", "demo"],
          env: {},
          headers: {},
          tags: [],
          enabled: {},
        },
      ],
      warnings: [],
    };
  }
  if (/^(npx|uvx|docker|bunx)\b/.test(trimmed)) {
    const parts = trimmed.split(/\s+/);
    return {
      kind: "command",
      drafts: [
        {
          id: "",
          name: parts[parts.length - 1]?.replace(/^@.*\//, "") ?? "imported",
          transport: "stdio",
          command: parts[0],
          args: parts.slice(1),
          env: {},
          headers: {},
          tags: [],
          enabled: {},
        },
      ],
      warnings: [],
    };
  }
  return {
    kind: "unknown",
    drafts: [],
    warnings: [],
    error: "could not parse as MCP JSON, URL, command, or skillstar://mcp link",
  };
}
