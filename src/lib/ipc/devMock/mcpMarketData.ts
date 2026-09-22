/**
 * Dev-mock sample data: MCP marketplace — catalog entries/details plus the
 * runtime-candidate, install-plan, preview and outcome builders. Consumed by
 * ./mcp.ts. The curated rows mirror `seeds::catalog` (shelves via `source`).
 */

import { iso } from "./shared";

type MockRecord = Record<string, unknown>;

// ---------------------------------------------------------------------------
// Marketplace catalog
// ---------------------------------------------------------------------------

// MCP marketplace sample data for browser dev mode. The curated rows mirror
// the built-in shortlist (`seeds::catalog`): source is the shelf — core /
// context / browser — and every curated row is recommended.
export const MCP_MARKET = [
  {
    id: "filesystem",
    name: "filesystem",
    namespace: "filesystem",
    description: "官方文件系统 MCP — 读写本地文件与目录，需在参数末尾追加允许访问的目录。",
    repoUrl: "https://github.com/modelcontextprotocol/servers",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "core",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "git",
    name: "git",
    namespace: "git",
    description: "官方 Git MCP — status / diff / log / commit 等本地仓库操作。",
    repoUrl: "https://github.com/modelcontextprotocol/servers",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["uvx"],
    updatedAt: iso(0),
    recommended: true,
    source: "core",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "github",
    name: "github",
    namespace: "github",
    description: "GitHub 官方远程 MCP — 仓库、issue、PR 与代码搜索，需要 Bearer PAT。",
    repoUrl: "https://github.com/github/github-mcp-server",
    stars: 0,
    license: null,
    version: null,
    kind: "remote",
    runtimes: [],
    updatedAt: iso(0),
    recommended: true,
    source: "core",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "context7",
    name: "context7",
    namespace: "context7",
    description: "Context7 MCP — 按版本注入库与框架的真实文档。",
    repoUrl: "https://github.com/upstash/context7",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "context",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "deepwiki",
    name: "deepwiki",
    namespace: "deepwiki",
    description: "DeepWiki MCP — 读取任意公开 GitHub 仓库的 AI 文档并回答仓库内问题。",
    repoUrl: "https://docs.devin.ai/work-with-devin/deepwiki-mcp",
    stars: 0,
    license: null,
    version: null,
    kind: "remote",
    runtimes: [],
    updatedAt: iso(0),
    recommended: true,
    source: "context",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "codegraph",
    name: "codegraph",
    namespace: "codegraph",
    description: "CodeGraph MCP — 本地代码知识图谱，返回符号源码、调用链与影响范围。",
    repoUrl: "https://github.com/colbymchenry/codegraph",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "context",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "serena",
    name: "serena",
    namespace: "serena",
    description: "Serena — 基于 LSP 的语义代码工具：符号查找、引用分析与精准编辑。",
    repoUrl: "https://github.com/oraios/serena",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["uvx"],
    updatedAt: iso(0),
    recommended: true,
    source: "context",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "playwright",
    name: "playwright",
    namespace: "playwright",
    description: "微软官方 Playwright MCP — 浏览器自动化与端到端验证。",
    repoUrl: "https://github.com/microsoft/playwright-mcp",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "browser",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "chrome-devtools",
    name: "chrome-devtools",
    namespace: "chrome-devtools",
    description: "Chrome 官方 DevTools MCP — 性能轨迹、网络请求与 DOM/控制台检查。",
    repoUrl: "https://github.com/ChromeDevTools/chrome-devtools-mcp",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "browser",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "figma",
    name: "figma",
    namespace: "figma",
    description: "Figma 官方 MCP — 设计稿结构、组件与变量交给 agent，实现设计到代码。",
    repoUrl: "https://www.figma.com/",
    stars: 0,
    license: null,
    version: null,
    kind: "remote",
    runtimes: [],
    updatedAt: iso(0),
    recommended: true,
    source: "creative",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "blender",
    name: "blender",
    namespace: "blender",
    description: "Blender MCP — 建模、材质、渲染与场景脚本驱动（需 Blender 运行 + 配套插件）。",
    repoUrl: "https://github.com/ahujasid/blender-mcp",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["uvx"],
    updatedAt: iso(0),
    recommended: true,
    source: "creative",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "photoshop",
    name: "photoshop",
    namespace: "photoshop",
    description: "Photoshop MCP — 文档操作、生成式填充与批处理配方。",
    repoUrl: "https://github.com/alisaitteke/photoshop-mcp",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: iso(0),
    recommended: true,
    source: "creative",
    status: "active",
    isLatest: true,
    registrySource: null,
  },
  {
    id: "after-effects",
    name: "after-effects",
    namespace: "after-effects",
    description: "After Effects 连接器（Oneprism）— 读取与改动真实合成：图层、关键帧、表达式。",
    repoUrl: "https://oneprism.io",
    stars: 0,
    license: null,
    version: null,
    kind: "remote",
    runtimes: [],
    updatedAt: iso(0),
    recommended: true,
    source: "creative",
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

const npmPackage = (identifier: string, extra: MockRecord = {}) => ({
  runtime: "npx",
  identifier,
  version: null,
  requiredEnv: [],
  registryType: "npm",
  environmentVariables: [],
  ...extra,
});

/** A positional `McpArgument`, as `server.json` `2025-12-11` shapes it. */
const positionalArg = (value: string) => ({
  kind: "positional",
  isRepeated: false,
  isRequired: false,
  isSecret: false,
  format: "string",
  value,
});

/** A named `McpArgument` with a pinned value — e.g. uvx's `--from <source>`. */
const namedArg = (name: string, value: string) => ({
  kind: "named",
  name,
  isRepeated: false,
  isRequired: false,
  isSecret: false,
  format: "string",
  value,
});

const githubRemote = () => ({
  transport: "http",
  transportType: "streamable-http",
  url: "https://api.githubcopilot.com/mcp/",
  requiredHeaders: ["Authorization"],
  headers: [{ name: "Authorization", value: "Bearer {TOKEN}", ...secretInput("GitHub PAT") }],
  variables: [],
});

export const MCP_MARKET_DETAILS: Record<string, Record<string, unknown>> = {
  filesystem: {
    readme: "# filesystem\n\nScoped read/write access to local directories.",
    packages: [npmPackage("@modelcontextprotocol/server-filesystem")],
    remotes: [],
  },
  git: {
    readme: "# git\n\nRead, search and edit local repositories.",
    packages: [{ ...npmPackage("mcp-server-git"), runtime: "uvx", registryType: "pypi" }],
    remotes: [],
  },
  github: {
    readme: "# github\n\nRemote MCP server hosted by GitHub.",
    packages: [],
    remotes: [githubRemote()],
  },
  context7: {
    readme: "# context7\n\nVersion-pinned library and framework docs.",
    packages: [npmPackage("@upstash/context7-mcp")],
    remotes: [],
  },
  deepwiki: {
    readme: "# deepwiki\n\nAsk questions about any public GitHub repository.",
    packages: [],
    remotes: [
      {
        transport: "http",
        transportType: "streamable-http",
        url: "https://mcp.deepwiki.com/mcp",
        requiredHeaders: [],
        headers: [],
        variables: [],
      },
    ],
  },
  codegraph: {
    readme: "# codegraph\n\nLocal code knowledge graph: symbols, call chains, blast radius.",
    packages: [
      npmPackage("@colbymchenry/codegraph", {
        packageArguments: [positionalArg("serve"), positionalArg("--mcp")],
      }),
    ],
    remotes: [],
  },
  serena: {
    readme: "# serena\n\nSemantic code toolkit over LSP: find symbols, trace references, edit precisely.",
    packages: [
      {
        ...npmPackage("serena"),
        runtime: "uvx",
        registryType: "pypi",
        runtimeArguments: [namedArg("--from", "git+https://github.com/oraios/serena")],
        packageArguments: [positionalArg("start-mcp-server"), positionalArg("--project-from-cwd")],
      },
    ],
    remotes: [],
  },
  playwright: {
    readme: "# playwright\n\nDrive a real Chromium: open pages, click, fill forms, screenshot.",
    packages: [npmPackage("@playwright/mcp")],
    remotes: [],
  },
  "chrome-devtools": {
    readme: "# chrome-devtools\n\nPerformance traces, network and console inspection in Chrome.",
    packages: [npmPackage("chrome-devtools-mcp")],
    remotes: [],
  },
  figma: {
    readme: "# figma\n\nFigma's official MCP: design structure, components and variables for your agent.",
    packages: [],
    remotes: [
      {
        transport: "http",
        transportType: "streamable-http",
        url: "https://mcp.figma.com/mcp",
        requiredHeaders: [],
        headers: [],
        variables: [],
      },
    ],
  },
  blender: {
    readme: "# blender-mcp\n\nScene modeling, materials and rendering through a Blender socket addon.",
    packages: [{ ...npmPackage("blender-mcp"), runtime: "uvx", registryType: "pypi" }],
    remotes: [],
  },
  photoshop: {
    readme: "# photoshop-mcp\n\n118 tools for Adobe Photoshop: documents, generative fills and recipes.",
    packages: [npmPackage("@alisaitteke/photoshop-mcp")],
    remotes: [],
  },
  "after-effects": {
    readme: "# after-effects\n\nOneprism connector for After Effects: real comps, layers, keyframes, expressions.",
    packages: [],
    remotes: [
      {
        transport: "http",
        transportType: "streamable-http",
        url: "https://live.oneprism.io/after-effects",
        requiredHeaders: [],
        headers: [],
        variables: [],
      },
    ],
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
    const env = Object.fromEntries(((pkg.requiredEnv as string[] | undefined) ?? []).map((key) => [key, ""]));
    const identifier = String(pkg.identifier ?? "");
    const versioned =
      identifier && pkg.version ? `${identifier}${pkg.runtime === "docker" ? ":" : "@"}${pkg.version}` : identifier;
    const argTokens = (list: unknown) =>
      ((list as MockRecord[] | undefined) ?? []).flatMap((arg) => {
        if (arg.kind === "named" && arg.name) {
          return arg.value ? [String(arg.name), String(arg.value)] : [String(arg.name)];
        }
        return arg.value ? [String(arg.value)] : [];
      });
    const launcher =
      pkg.runtime === "npx" || pkg.runtime === "bunx"
        ? ["-y", versioned]
        : pkg.runtime === "docker"
          ? ["run", "-i", "--rm", versioned]
          : [versioned];
    return {
      ...base,
      transport: "stdio",
      command: pkg.runtime,
      env,
      args: [...argTokens(pkg.runtimeArguments), ...launcher, ...argTokens(pkg.packageArguments)],
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

  const entry = { ...draft, env, headers };
  return {
    entry,
    commandPreview: plan.commandPreview,
    approvalTarget: mockApprovalTarget(entry, plan.commandPreview as string | null),
    missing,
  };
}

/**
 * `McpInstallPreview.approvalTarget` — everything the confirmation step showed,
 * in one comparable string. Mirrors `approval_target` in
 * `skillstar_app::mcp::install`: command line (or url) plus env, headers and the
 * config key, JSON-encoded so a value carrying a newline cannot forge a row.
 */
function mockApprovalTarget(entry: MockRecord, commandPreview: string | null): string {
  const sorted = (values: unknown) =>
    Object.fromEntries(Object.entries((values as Record<string, string>) ?? {}).sort(([a], [b]) => a.localeCompare(b)));
  return JSON.stringify({
    env: sorted(entry.env),
    headers: sorted(entry.headers),
    name: String(entry.name ?? ""),
    target: String(commandPreview ?? entry.url ?? "").trim(),
  });
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
  approvedTarget: string,
): MockRecord {
  const preview = mcpInstallPreview(id, runtimeId, answers);
  const entry = preview.entry as MockRecord;
  if (String(preview.approvalTarget) !== approvedTarget.trim()) {
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

/** `McpServerPage` — filtered / sorted / paginated cards with a total. */
export function mcpMarketPage(query: MockRecord): MockRecord {
  const search = String(query.search ?? "").toLowerCase();
  const publisherId = String(query.publisherId ?? "").toLowerCase();
  const runtimes = (query.runtimes as string[] | undefined) ?? [];
  const statuses = (query.statuses as string[] | undefined) ?? [];

  const registryOnly = Boolean(query.registryOnly);
  const curatedOnly = Boolean(query.curatedOnly);

  let items = MCP_MARKET.filter((m) => {
    if (search && !`${m.name} ${m.namespace} ${m.description}`.toLowerCase().includes(search)) return false;
    if (publisherId && publisherId !== "github" && (m.source ?? "").toLowerCase() !== publisherId) return false;
    if ((publisherId === "github" || registryOnly) && m.source) return false;
    if (curatedOnly && !publisherId && !m.source) return false;
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
