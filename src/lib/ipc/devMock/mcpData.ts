/**
 * Dev-mock sample data: MCP — the managed server store, per-tool sync
 * statuses, install presets, the catalog sources and their per-source sync
 * state, and the MCP marketplace entries/details plus the runtime-candidate,
 * install-plan, probe and draft builders. Consumed by ./mcp.ts.
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
    toolId: "zed",
    label: "Zed",
    configPath: "~/.config/zed/settings.json",
    installed: false,
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
  // Curated-derived: id *is* the catalog row id, and `catalogId` routes the
  // chip to the install wizard instead of the create form. The other two are
  // built-ins, which have no catalog row and keep the form path.
  {
    id: "mkt-github",
    catalogId: "mkt-github",
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

// ---------------------------------------------------------------------------
// Marketplace catalog
// ---------------------------------------------------------------------------

// MCP marketplace sample data for browser dev mode.
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
    status: "active",
    isLatest: true,
    registrySource: null,
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
    status: "active",
    isLatest: true,
    registrySource: "official",
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
    status: "active",
    isLatest: true,
    registrySource: "official",
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
    status: "deprecated",
    isLatest: false,
    registrySource: "github",
  },
];

/** A secret `Input`, as `server.json` `2025-12-11` shapes it. */
const secretInput = (description: string) => ({
  description,
  isRequired: true,
  isSecret: true,
  format: "string",
});

export const MCP_MARKET_DETAILS: Record<string, Record<string, unknown>> = {
  "adspower-local-api": {
    readme: "# adspower-local-api\n\nAdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化。",
    packages: [
      {
        runtime: "npx",
        identifier: "local-api-mcp-typescript",
        version: null,
        requiredEnv: ["API_KEY"],
        registryType: "npm",
        environmentVariables: [
          { name: "PORT", description: "Local API port", format: "number", default: "50325" },
          { name: "API_KEY", ...secretInput("AdsPower Local API key") },
        ],
      },
    ],
    remotes: [],
  },
  "mkt-filesystem": {
    readme: "# server-filesystem\n\nGives the agent scoped read/write access to a local directory.",
    packages: [
      {
        runtime: "docker",
        identifier: "mcp/filesystem",
        version: "1.2.0",
        requiredEnv: [],
        registryType: "oci",
        environmentVariables: [],
      },
      {
        runtime: "npx",
        identifier: "@modelcontextprotocol/server-filesystem",
        version: "1.2.0",
        requiredEnv: [],
        registryType: "npm",
        environmentVariables: [
          {
            name: "ROOT",
            description: "Directory the agent may read and write",
            format: "filepath",
            default: "/Users/dev",
          },
        ],
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
        transportType: "streamable-http",
        url: "https://api.githubcopilot.com/mcp/",
        requiredHeaders: ["Authorization"],
        headers: [{ name: "Authorization", value: "Bearer {TOKEN}", ...secretInput("GitHub token") }],
        variables: [],
      },
      {
        transport: "sse",
        transportType: "sse",
        url: "https://api.githubcopilot.com/mcp/sse",
        requiredHeaders: ["Authorization"],
        headers: [{ name: "Authorization", value: "Bearer {TOKEN}", ...secretInput("GitHub token") }],
        variables: [],
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
        registryType: "pypi",
        environmentVariables: [],
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

// ---------------------------------------------------------------------------
// Runtime candidates / install plan / probe
// ---------------------------------------------------------------------------

const SHAPE_BY_REGISTRY_TYPE: Record<string, { shape: string; rank: number }> = {
  oci: { shape: "packageOci", rank: 2 },
  mcpb: { shape: "packageMcpb", rank: 3 },
};

type MockRecord = Record<string, unknown>;

const details = (id: string) => MCP_MARKET_DETAILS[id] ?? {};
const packagesOf = (id: string) => (details(id).packages as MockRecord[] | undefined) ?? [];
const remotesOf = (id: string) => (details(id).remotes as MockRecord[] | undefined) ?? [];

/**
 * `McpRuntimeSelection` — remotes ranked above packages, streamable-http above
 * sse, oci above plain packages. The mock pretends every launcher is installed;
 * the real selector checks `PATH`.
 */
export function mcpRuntimeSelection(id: string): MockRecord {
  const candidates: MockRecord[] = [];
  remotesOf(id).forEach((remote, index) => {
    const sse = remote.transport === "sse";
    candidates.push({
      id: `remote:${index}`,
      shape: sse ? "remoteSse" : "remoteStreamableHttp",
      transport: sse ? "sse" : "http",
      url: remote.url,
      rank: sse ? 1 : 0,
      installable: true,
      warnings: sse
        ? ["The SSE transport is deprecated. Prefer a streamable-http endpoint when the publisher offers one."]
        : [],
    });
  });
  packagesOf(id).forEach((pkg, index) => {
    const registryType = String(pkg.registryType ?? "");
    const known = SHAPE_BY_REGISTRY_TYPE[registryType] ?? { shape: "packagePlain", rank: 4 };
    candidates.push({
      id: `package:${index}`,
      shape: known.shape,
      transport: "stdio",
      registryType,
      identifier: pkg.identifier,
      version: pkg.version,
      runtimeCommand: pkg.runtime,
      runtimeAvailable: true,
      rank: known.rank,
      installable: known.shape !== "packageMcpb",
      blockedReason:
        known.shape === "packageMcpb"
          ? "MCPB bundles must be downloaded and checked against fileSha256 before they can run; SkillStar has no bundle installer yet."
          : null,
      warnings: [],
    });
  });
  candidates.sort(
    (a, b) => Number(a.installable === false) - Number(b.installable === false) || Number(a.rank) - Number(b.rank),
  );
  return {
    serverId: id,
    candidates,
    recommendedId: candidates.find((c) => c.installable)?.id ?? null,
  };
}

const TEMPLATE_TOKEN = /\{([A-Za-z0-9_.-]+)\}/g;

/**
 * Seed the `{curly_brace}` sub-form the real backend ships with each templated
 * input, so the browser dev path renders the same variable fields the app does.
 */
function templateVariables(declared: MockRecord): MockRecord[] {
  const value = typeof declared.value === "string" ? declared.value : "";
  const map = (declared.variables as Record<string, MockRecord> | undefined) ?? {};
  const out: MockRecord[] = [];
  for (const [, name] of value.matchAll(TEMPLATE_TOKEN)) {
    if (out.some((seen) => seen.name === name)) continue;
    const variable = map[name] ?? { isRequired: true, isSecret: false, format: "string" };
    out.push({
      name,
      variable,
      prefilled: variable.isRequired || variable.isSecret ? "" : String(variable.default ?? ""),
    });
  }
  return out;
}

/** `McpInstallPlan` — the pre-install confirmation payload. */
export function mcpInstallPlan(id: string, runtimeId?: string): MockRecord {
  const selection = mcpRuntimeSelection(id);
  const candidates = selection.candidates as MockRecord[];
  const selected = candidates.find((c) => c.id === runtimeId) ?? candidates.find((c) => c.installable) ?? null;
  const entry = MCP_MARKET.find((m) => m.id === id);
  const draft = mcpMarketDraft(id);

  const packageIndex = typeof selected?.id === "string" ? Number(String(selected.id).split(":")[1]) : 0;
  const isPackage = typeof selected?.id === "string" && String(selected.id).startsWith("package:");
  const remote = isPackage ? undefined : remotesOf(id)[packageIndex];
  const pkg = isPackage ? packagesOf(id)[packageIndex] : undefined;

  const inputs: MockRecord[] = [];
  for (const [index, env] of ((pkg?.environmentVariables as MockRecord[] | undefined) ?? []).entries()) {
    inputs.push({
      key: env.name,
      scope: "environment",
      index,
      input: env,
      prefilled: env.isSecret || env.isRequired ? "" : String(env.default ?? ""),
      mustAsk: Boolean(env.isSecret || env.isRequired),
      variables: templateVariables(env),
    });
  }
  for (const [index, header] of ((remote?.headers as MockRecord[] | undefined) ?? []).entries()) {
    inputs.push({
      key: header.name,
      scope: "header",
      index,
      input: header,
      prefilled: String(header.value ?? ""),
      mustAsk: Boolean((header.isSecret || header.isRequired) && header.value === undefined),
      variables: templateVariables(header),
    });
  }

  const command = draft.command as string | undefined;
  const args = (draft.args as string[] | undefined) ?? [];
  const secretKeys = inputs.filter((i) => (i.input as MockRecord).isSecret).map((i) => String(i.key));

  return {
    serverId: id,
    serverName: entry?.name ?? "mcp-server",
    namespace: entry?.namespace ?? "",
    selection,
    selectedRuntimeId: selected?.id ?? null,
    transport: draft.transport,
    command: command ?? null,
    args,
    resolvedCommandPath: command ? `/usr/local/bin/${command}` : null,
    commandPreview: command ? [command, ...args].join(" ") : null,
    usesShell: false,
    url: draft.url ?? null,
    inputs,
    secretPolicy: {
      storage: "userLevelConfig",
      secretKeys,
      writesProjectScopedConfig: false,
      note: secretKeys.length
        ? "Secret values are stored in SkillStar's user-level MCP store and written into each enabled tool's user-level config file (under your home directory). SkillStar writes no project-scoped MCP config, so no secret reaches a version-controlled file."
        : "This server declares no secret inputs.",
    },
    warnings: [
      ...((selected?.warnings as string[] | undefined) ?? []),
      ...(entry?.status && entry.status !== "active" ? [`The registry marks this server '${entry.status}'.`] : []),
      ...(entry && entry.isLatest === false ? ["The registry knows of a newer version of this server."] : []),
    ],
    draft,
  };
}

/**
 * `McpInstallPreview` — the entry one set of answers produces.
 *
 * The real derivation lives in Rust (`preview_install`), which substitutes into
 * the structured argument list. The mock has no structured arguments to work
 * from, so it folds answers into `env` / `headers` only and reuses the plan's
 * command line: enough to exercise the wizard in a browser, never the authority
 * on what gets installed.
 */
export function mcpInstallPreview(id: string, runtimeId: string | undefined, answers: MockRecord[]): MockRecord {
  const plan = mcpInstallPlan(id, runtimeId);
  const inputs = (plan.inputs as MockRecord[]) ?? [];
  const draft = plan.draft as MockRecord;
  const answerFor = (scope: unknown, index: unknown) =>
    answers.find((a) => a.scope === scope && a.index === index && a.variable == null);

  const env: Record<string, string> = {};
  const headers: Record<string, string> = {};
  const missing: MockRecord[] = [];
  for (const input of inputs) {
    const value = String(answerFor(input.scope, input.index)?.value ?? input.prefilled ?? "");
    if (value) {
      if (input.scope === "environment") env[String(input.key)] = value;
      if (input.scope === "header") headers[String(input.key)] = value;
    } else if (input.mustAsk) {
      missing.push({ key: input.key, scope: input.scope, index: input.index, variable: null });
    }
  }

  return {
    entry: { ...draft, env, headers },
    commandPreview: plan.commandPreview,
    missing,
  };
}

/**
 * `McpInstallOutcome` — what committing one install produces.
 *
 * The two refusals are the point of the mock: it re-derives the preview and
 * refuses unless it still renders the approved string, so the browser dev path
 * exercises the same two branches the Rust seam does. What it *cannot* fake is
 * the reason those branches exist — a catalog row rewritten mid-wizard.
 */
export function mcpInstallOutcome(
  id: string,
  runtimeId: string | undefined,
  answers: MockRecord[],
  enabled: Record<string, boolean>,
  approvedPreview: string,
): MockRecord {
  const preview = mcpInstallPreview(id, runtimeId, answers);
  const entry = preview.entry as MockRecord;
  const approvalTarget = String(preview.commandPreview ?? entry.url ?? "").trim();
  if (approvalTarget !== approvedPreview.trim()) {
    return { status: "rejected", rejection: { reason: "commandChanged" } };
  }
  const missing = (preview.missing as MockRecord[]) ?? [];
  if (missing.length > 0) {
    return { status: "rejected", rejection: { reason: "missingInputs", missing } };
  }
  return {
    status: "installed",
    installed: {
      server: { ...entry, id: `mcp-installed-${id}`, enabled },
      syncResults: Object.entries(enabled)
        .filter(([, on]) => on)
        .map(([toolId]) => ({
          toolId,
          serverId: `mcp-installed-${id}`,
          success: true,
          skipped: false,
          configPath: `/Users/dev/.config/${toolId}/mcp.json`,
          backupPath: null,
          error: null,
          rolledBack: false,
          rollbackError: null,
        })),
    },
  };
}

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
    error: null,
    checkedAt: Date.now(),
  };
}

/** `McpServerPage` — filtered / sorted / paginated cards with a total. */
export function mcpMarketPage(query: MockRecord): MockRecord {
  const search = String(query.search ?? "").toLowerCase();
  const publisherId = String(query.publisherId ?? "").toLowerCase();
  const runtimes = (query.runtimes as string[] | undefined) ?? [];
  const statuses = (query.statuses as string[] | undefined) ?? [];

  let items = MCP_MARKET.filter((m) => {
    if (search && !`${m.name} ${m.namespace} ${m.description}`.toLowerCase().includes(search)) return false;
    if (publisherId && publisherId !== "github" && (m.source ?? "").toLowerCase() !== publisherId) return false;
    if (runtimes.length && !m.runtimes.some((r) => runtimes.includes(r))) return false;
    if (statuses.length && !statuses.includes(m.status)) return false;
    if (query.recommendedOnly && !("recommended" in m && m.recommended)) return false;
    if (query.latestOnly && m.isLatest === false) return false;
    if (typeof query.minStars === "number" && m.stars < query.minStars) return false;
    if (typeof query.maxStars === "number" && m.stars > query.maxStars) return false;
    return true;
  });

  if (query.sort === "stars") items = [...items].sort((a, b) => b.stars - a.stars);
  if (query.sort === "name") items = [...items].sort((a, b) => a.name.localeCompare(b.name));

  const total = items.length;
  const offset = Number(query.offset ?? 0);
  const limit = typeof query.limit === "number" ? query.limit : null;
  return {
    items: limit === null ? items.slice(offset) : items.slice(offset, offset + limit),
    total,
    offset,
    limit,
  };
}
