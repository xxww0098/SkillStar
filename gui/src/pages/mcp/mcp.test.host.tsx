import { View, mount, type JSX } from "nsh";
import { DialogHost } from "../../chrome/dialog";
import type { McpBridge } from "./bridge";
import { McpWorkbench } from "./index";
import { createMcpState } from "./state";
import type {
  BackupEntry,
  CheckResult,
  FoundServer,
  LibrarySnapshot,
  McpAgentTarget,
  McpServerRecord,
  StoreItem,
} from "./types";

const FAKE_AGENTS: McpAgentTarget[] = [
  {
    id: "claude",
    name: "Claude Code",
    installed: true,
    sharedHere: false,
    configPath: "/tmp/.claude.json",
    format: "claude",
    noRemote: false,
    noSse: false,
    supportsToolsOff: true,
    supportsStartupTimeout: false,
  },
  {
    id: "codex",
    name: "Codex",
    installed: true,
    sharedHere: false,
    configPath: "/tmp/.codex/config.toml",
    format: "codex",
    noRemote: false,
    noSse: true,
    supportsToolsOff: true,
    supportsStartupTimeout: true,
  },
  {
    id: "antigravity",
    name: "Antigravity",
    installed: false,
    sharedHere: false,
    configPath: "/tmp/.gemini/config/mcp_config.json",
    format: "antigravity",
    noRemote: false,
    noSse: false,
    supportsToolsOff: false,
    supportsStartupTimeout: false,
  },
];

const STORE_CATALOG: StoreItem[] = [
  {
    id: "playwright",
    name: "playwright",
    title: "Playwright",
    description: "Drive a real Chromium browser via accessibility snapshots.",
    publisher: "Microsoft",
    icon: "",
    homepage: "https://github.com/microsoft/playwright-mcp",
    transport: "stdio",
    runs: "npx",
    signIn: false,
    featured: true,
    matchByAddr: false,
    source: "featured",
    inputs: [],
    template: {
      name: "playwright",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@playwright/mcp@latest"],
      env: {},
      url: "",
      headers: {},
      paused: false,
      agents: [],
      disabledTools: [],
    },
  },
  {
    id: "brave-search",
    name: "brave-search",
    title: "Brave Search",
    description: "Web and news search powered by Brave Search API.",
    publisher: "Brave",
    icon: "",
    homepage: "https://github.com/brave/brave-search-mcp-server",
    transport: "stdio",
    runs: "npx",
    signIn: false,
    featured: true,
    matchByAddr: false,
    source: "featured",
    inputs: [
      {
        key: "BRAVE_API_KEY",
        whereField: "env",
        label: "Brave API key",
        description: "From brave.com/search/api",
        placeholder: "BSA_...",
        secret: true,
        required: true,
        format: "",
      },
    ],
    template: {
      name: "brave-search",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@brave/brave-search-mcp-server"],
      env: {},
      url: "",
      headers: {},
      paused: false,
      agents: [],
      disabledTools: [],
    },
  },
  {
    id: "sentry",
    name: "sentry",
    title: "Sentry",
    description: "Inspect issues and traces via Sentry's hosted remote MCP server.",
    publisher: "Sentry",
    icon: "",
    homepage: "https://mcp.sentry.dev",
    transport: "http",
    runs: "remote",
    signIn: true,
    featured: false,
    matchByAddr: true,
    source: "builtin",
    inputs: [],
    template: {
      name: "sentry",
      transport: "http",
      command: "",
      args: [],
      env: {},
      url: "https://mcp.sentry.dev/mcp",
      headers: {},
      paused: false,
      agents: [],
      disabledTools: [],
    },
  },
  {
    id: "datadog-online",
    name: "datadog-online",
    title: "Datadog Online",
    description: "Query live metrics and logs from the online MCP registry.",
    publisher: "Datadog",
    icon: "",
    homepage: "https://mcp.datadoghq.com",
    transport: "sse",
    runs: "sse",
    signIn: false,
    featured: false,
    matchByAddr: true,
    source: "online",
    inputs: [],
    template: {
      name: "datadog-online",
      transport: "sse",
      command: "",
      args: [],
      env: {},
      url: "https://mcp.datadoghq.com/sse",
      headers: {},
      paused: false,
      agents: [],
      disabledTools: [],
    },
  },
];

function createFakeBridge(): McpBridge {
  let servers: McpServerRecord[] = [
    {
      name: "filesystem",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      env: {},
      url: "",
      headers: {},
      paused: false,
      agents: ["claude"],
      disabledTools: [],
    },
  ];
  let found: FoundServer[] = [
    {
      agentId: "codex",
      agentName: "Codex",
      configPath: "/tmp/.codex/config.toml",
      format: "codex",
      server: {
        name: "legacy-git",
        transport: "stdio",
        command: "uvx",
        args: ["mcp-server-git"],
        env: {},
        url: "",
        headers: {},
        paused: false,
        agents: [],
        disabledTools: [],
      },
    },
    {
      agentId: "codex",
      agentName: "Codex",
      configPath: "/tmp/.codex/config.toml",
      format: "codex",
      server: {
        name: "filesystem",
        transport: "stdio",
        command: "uvx",
        args: ["mcp-server-fs"],
        env: {},
        url: "",
        headers: {},
        paused: false,
        agents: [],
        disabledTools: [],
      },
    },
  ];
  const backups: BackupEntry[] = [
    {
      id: "20261120-0001-.claude.json.bak",
      fileName: "20261120-0001-.claude.json.bak",
      targetPath: "/tmp/.claude.json",
      createdAt: "2026-11-20T10:00:00Z",
      sizeBytes: 128,
      existed: true,
    },
  ];

  const makeUniqueName = (base: string): string => {
    if (!servers.some((s) => s.name === base)) return base;
    let i = 2;
    while (servers.some((s) => s.name === `${base}-${i}`)) i += 1;
    return `${base}-${i}`;
  };

  const makeSnap = (): LibrarySnapshot => ({
    servers: servers.map((s) => ({ ...s, agents: [...s.agents], disabledTools: [...s.disabledTools] })),
    agents: FAKE_AGENTS,
    found: [...found],
    warnings: [],
  });

  return {
    async snapshot() {
      return makeSnap();
    },
    async saveServer(server, originalName, conflict) {
      if (originalName) {
        servers = servers.map((s) => (s.name === originalName ? { ...server } : s));
      } else if (servers.some((s) => s.name === server.name)) {
        if (conflict === "replace") {
          servers = servers.map((s) => (s.name === server.name ? { ...server } : s));
        } else {
          const unique = makeUniqueName(server.name);
          servers = [...servers, { ...server, name: unique }];
        }
      } else {
        servers = [...servers, { ...server }];
      }
      return makeSnap();
    },
    async toggleServerAgent(name, agentId, enabled) {
      servers = servers.map((s) => {
        if (s.name !== name) return s;
        const next = enabled
          ? Array.from(new Set([...s.agents, agentId]))
          : s.agents.filter((a) => a !== agentId);
        return { ...s, agents: next };
      });
      return makeSnap();
    },
    async setServerAgents(name, agentIds) {
      servers = servers.map((s) => (s.name === name ? { ...s, agents: [...agentIds] } : s));
      return makeSnap();
    },
    async setServerPaused(name, paused) {
      servers = servers.map((s) => (s.name === name ? { ...s, paused } : s));
      return makeSnap();
    },
    async removeServer(name) {
      servers = servers.filter((s) => s.name !== name);
      return makeSnap();
    },
    async batchRemoveServers(names) {
      const drop = new Set(names);
      servers = servers.filter((s) => !drop.has(s.name));
      return makeSnap();
    },
    async importFoundServer(agentId, name, conflict = "rename") {
      const target = found.find((f) => f.agentId === agentId && f.server.name === name);
      if (target) {
        found = found.filter((f) => !(f.agentId === agentId && f.server.name === name));
        const idx = servers.findIndex((s) => s.name === name);
        if (idx === -1) {
          servers = [...servers, { ...target.server, agents: [agentId] }];
        } else if (conflict === "keep") {
          servers = servers.map((s, i) =>
            i === idx && !s.agents.includes(agentId) ? { ...s, agents: [...s.agents, agentId] } : s,
          );
        } else if (conflict === "replace") {
          const mergedAgents = Array.from(new Set([...servers[idx].agents, agentId]));
          servers = servers.map((s, i) =>
            i === idx ? { ...target.server, agents: mergedAgents } : s,
          );
        } else {
          const unique = makeUniqueName(name);
          servers = [...servers, { ...target.server, name: unique, agents: [agentId] }];
        }
      }
      return makeSnap();
    },
    async removeFoundServer(agentId, name) {
      found = found.filter((f) => !(f.agentId === agentId && f.server.name === name));
      return makeSnap();
    },
    async syncAll() {
      return makeSnap();
    },
    async parsePaste(raw) {
      if (raw.includes("mcp.linear.app")) {
        return {
          detectedKind: "cli:claude-mcp-add",
          servers: [
            {
              name: "linear",
              transport: "http",
              command: "",
              args: [],
              env: {},
              url: "https://mcp.linear.app/mcp",
              headers: {},
              paused: false,
              agents: [],
              disabledTools: [],
            },
          ],
        };
      }
      throw new Error("unrecognized format");
    },
    async projectSnippets() {
      const active = servers.filter((s) => !s.paused);
      if (active.length === 0) return [];
      const claudeEntries: Record<string, unknown> = {};
      for (const s of active) {
        claudeEntries[s.name] = s.transport === "stdio" ? { command: s.command } : { url: s.url };
      }
      const codexBody = active
        .map((s) =>
          s.transport === "stdio"
            ? `[mcp_servers.${s.name}]\ncommand = "${s.command}"\n`
            : `[mcp_servers.${s.name}]\nurl = "${s.url}"\n`,
        )
        .join("\n");
      return [
        {
          agentId: "claude",
          agentName: "Claude Code",
          relPath: ".mcp.json",
          format: "claude",
          content: JSON.stringify({ mcpServers: claudeEntries }, null, 2),
        },
        {
          agentId: "codex",
          agentName: "Codex",
          relPath: ".codex/config.toml",
          format: "codex",
          content: codexBody,
        },
      ];
    },
    async checkServer(name): Promise<CheckResult> {
      return {
        ok: true,
        serverName: `${name}-mock`,
        serverVersion: "1.0.0",
        durationMs: 14,
        tools: [
          { name: "read_file", description: "Read file contents" },
          { name: "write_file", description: "Write file contents" },
        ],
      };
    },
    async queryStore(query, online) {
      const q = query.trim().toLowerCase();
      const items = STORE_CATALOG.filter(
        (it) =>
          (online || it.source !== "online") &&
          (q.length === 0 || it.id.includes(q) || it.title.toLowerCase().includes(q)),
      ).map((it) => ({
        ...it,
        have: servers.find((s) => s.name === it.name)?.name,
      }));
      return { items, onlineUsed: online };
    },
    async installStoreServer(request) {
      const env = { ...request.item.template.env };
      for (const spec of request.item.inputs) {
        const val = (request.values[spec.key] ?? "").trim();
        if (!val && spec.required) {
          throw new Error(`missing required store input: ${spec.label}`);
        }
        if (val && spec.whereField === "env") env[spec.key] = val;
      }
      const name = makeUniqueName(request.name || request.item.name);
      servers = [
        ...servers,
        {
          ...request.item.template,
          marketId: request.item.id,
          name,
          env,
          agents: [...request.agents],
        },
      ];
      return makeSnap();
    },
    async listBackups() {
      return [...backups];
    },
    async restoreBackup(backupId) {
      const foundBak = backups.find((b) => b.id === backupId);
      return {
        id: backupId,
        targetPath: foundBak?.targetPath ?? "/tmp/.claude.json",
        removedNewFile: foundBak ? !foundBak.existed : false,
        restoredBytes: foundBak?.sizeBytes ?? 128,
      };
    },
  };
}

export function App(): JSX.Element {
  const state = createMcpState(createFakeBridge());
  return (
    <View style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column" }}>
      <McpWorkbench state={state} />
      <DialogHost />
    </View>
  );
}

export default App;
mount(App);
