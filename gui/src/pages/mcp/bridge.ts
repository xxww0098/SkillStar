import { McpHub } from "skillstar-gui/native";
import type {
  BackupEntry,
  BackupRestoreResult,
  CheckResult,
  ConflictStrategy,
  FoundServer,
  LibraryDelta,
  LibrarySnapshot,
  McpAgentTarget,
  McpServerRecord,
  McpTransport,
  ParsedPasteResult,
  ProjectSnippet,
  StoreInputSpec,
  StoreInstallRequest,
  StoreItem,
  StoreSearchResponse,
  SyncWarning,
} from "./types";

export type MutationResult = LibrarySnapshot | LibraryDelta;

export interface McpBridge {
  snapshot(): Promise<LibrarySnapshot>;
  saveServer(
    server: McpServerRecord,
    originalName?: string | null,
    conflict?: ConflictStrategy,
  ): Promise<MutationResult>;
  saveServers?(
    servers: McpServerRecord[],
    conflict?: ConflictStrategy,
  ): Promise<MutationResult>;
  toggleServerAgent(name: string, agentId: string, enabled: boolean): Promise<MutationResult>;
  setServerAgents(name: string, agentIds: string[]): Promise<MutationResult>;
  setServerPaused(name: string, paused: boolean): Promise<MutationResult>;
  batchSetPaused?(names: string[], paused: boolean): Promise<MutationResult>;
  removeServer(name: string): Promise<MutationResult>;
  batchRemoveServers?(names: string[]): Promise<MutationResult>;
  importFoundServer(
    agentId: string,
    name: string,
    conflict?: ConflictStrategy,
  ): Promise<LibrarySnapshot>;
  removeFoundServer(agentId: string, name: string): Promise<LibrarySnapshot>;
  syncAll(): Promise<LibrarySnapshot>;
  parsePaste(raw: string): Promise<ParsedPasteResult>;
  projectSnippets(agents: string[], projectRoot?: string | null): Promise<ProjectSnippet[]>;
  checkServer(name: string): Promise<CheckResult>;
  queryStore(query: string, online: boolean, limit?: number): Promise<StoreSearchResponse>;
  installStoreServer(request: StoreInstallRequest): Promise<MutationResult>;
  listBackups?(limit?: number): Promise<BackupEntry[]>;
  restoreBackup?(backupId: string, targetPath?: string | null): Promise<BackupRestoreResult>;
}

function str(v: unknown, fallback = ""): string {
  return typeof v === "string" ? v : fallback;
}

function strArr(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}

function strRec(v: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (v !== null && typeof v === "object") {
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      if (typeof val === "string") out[k] = val;
    }
  }
  return out;
}

function normTransport(v: unknown): McpTransport {
  if (v === "http" || v === "sse" || v === "stdio") return v;
  return "stdio";
}

function parseServer(raw: unknown): McpServerRecord {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    name: str(r.name),
    transport: normTransport(r.transport),
    command: str(r.command),
    args: strArr(r.args),
    env: strRec(r.env),
    url: str(r.url),
    headers: strRec(r.headers),
    paused: r.paused === true,
    agents: strArr(r.agents),
    disabledTools: strArr(r.disabledTools),
    startupTimeoutSec: typeof r.startupTimeoutSec === "number" ? r.startupTimeoutSec : undefined,
    marketId: typeof r.marketId === "string" ? r.marketId : undefined,
    updatedAt: typeof r.updatedAt === "string" ? r.updatedAt : undefined,
  };
}

function parseAgent(raw: unknown): McpAgentTarget {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    id: str(r.id),
    name: str(r.name),
    installed: r.installed === true,
    sharedHere: r.sharedHere === true,
    configPath: str(r.configPath),
    format: str(r.format),
    noRemote: r.noRemote === true,
    noSse: r.noSse === true,
    supportsToolsOff: r.supportsToolsOff === true,
    supportsStartupTimeout: r.supportsStartupTimeout === true,
    notes: typeof r.notes === "string" ? r.notes : undefined,
  };
}

function parseFound(raw: unknown): FoundServer {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    agentId: str(r.agentId),
    agentName: str(r.agentName),
    configPath: str(r.configPath),
    format: str(r.format),
    server: parseServer(r.server),
  };
}

function parseWarning(raw: unknown): SyncWarning {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    agentId: str(r.agentId),
    path: str(r.path),
    code: str(r.code),
    message: str(r.message),
  };
}

function parseSnapshot(raw: unknown): LibrarySnapshot {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    servers: Array.isArray(r.servers) ? r.servers.map(parseServer) : [],
    agents: Array.isArray(r.agents) ? r.agents.map(parseAgent) : [],
    found: Array.isArray(r.found) ? r.found.map(parseFound) : [],
    warnings: Array.isArray(r.warnings) ? r.warnings.map(parseWarning) : [],
  };
}

function parseDelta(raw: unknown): LibraryDelta {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    servers: Array.isArray(r.servers) ? r.servers.map(parseServer) : [],
    warnings: Array.isArray(r.warnings) ? r.warnings.map(parseWarning) : [],
  };
}

function parseInputSpec(raw: unknown): StoreInputSpec {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    key: str(r.key),
    whereField: str(r.whereField, "env"),
    label: str(r.label),
    description: str(r.description),
    placeholder: str(r.placeholder),
    secret: r.secret === true,
    required: r.required === true,
    format: str(r.format),
  };
}

function parseStoreItem(raw: unknown): StoreItem {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    id: str(r.id),
    name: str(r.name),
    title: str(r.title),
    description: str(r.description),
    publisher: str(r.publisher),
    icon: str(r.icon),
    homepage: str(r.homepage),
    transport: normTransport(r.transport),
    runs: str(r.runs),
    signIn: r.signIn === true,
    featured: r.featured === true,
    matchByAddr: r.matchByAddr === true,
    source: str(r.source),
    stars: typeof r.stars === "number" ? r.stars : undefined,
    have: typeof r.have === "string" ? r.have : undefined,
    inputs: Array.isArray(r.inputs) ? r.inputs.map(parseInputSpec) : [],
    template: parseServer(r.template),
  };
}

export const realMcpBridge: McpBridge = {
  async snapshot() {
    return parseSnapshot(await McpHub.snapshot());
  },
  async saveServer(server, originalName = null, conflict = "rename") {
    return parseDelta(
      await McpHub.saveServer(server as never, originalName ?? null, conflict ?? "rename"),
    );
  },
  async saveServers(servers, conflict = "rename") {
    return parseDelta(await McpHub.saveServers(servers as never, conflict ?? "rename"));
  },
  async toggleServerAgent(name, agentId, enabled) {
    return parseDelta(await McpHub.toggleServerAgent(name, agentId, enabled));
  },
  async setServerAgents(name, agentIds) {
    return parseDelta(await McpHub.setServerAgents(name, agentIds));
  },
  async setServerPaused(name, paused) {
    return parseDelta(await McpHub.setServerPaused(name, paused));
  },
  async batchSetPaused(names, paused) {
    return parseDelta(await McpHub.batchSetPaused(names, paused));
  },
  async removeServer(name) {
    return parseDelta(await McpHub.removeServer(name));
  },
  async batchRemoveServers(names) {
    return parseDelta(await McpHub.batchRemoveServers(names));
  },
  async importFoundServer(agentId, name, conflict = "rename") {
    return parseSnapshot(await McpHub.importFoundServer(agentId, name, conflict ?? "rename"));
  },
  async removeFoundServer(agentId, name) {
    return parseSnapshot(await McpHub.removeFoundServer(agentId, name));
  },
  async syncAll() {
    return parseSnapshot(await McpHub.syncAll());
  },
  async parsePaste(raw) {
    const res = (await McpHub.parsePaste(raw)) as Record<string, unknown>;
    return {
      servers: Array.isArray(res.servers) ? res.servers.map(parseServer) : [],
      detectedKind: str(res.detectedKind),
    };
  },
  async projectSnippets(agents, projectRoot = null) {
    const res = await McpHub.projectSnippets(agents, projectRoot ?? null);
    if (!Array.isArray(res)) return [];
    return res.map((item) => {
      const r = (item ?? {}) as Record<string, unknown>;
      return {
        agentId: str(r.agentId),
        agentName: str(r.agentName),
        relPath: str(r.relPath),
        format: str(r.format),
        content: str(r.content),
      };
    });
  },
  async checkServer(name) {
    const r = (await McpHub.checkServer(name)) as Record<string, unknown>;
    return {
      ok: r.ok === true,
      serverName: str(r.serverName),
      serverVersion: str(r.serverVersion),
      tools: Array.isArray(r.tools)
        ? r.tools.map((tool) => {
            const tr = (tool ?? {}) as Record<string, unknown>;
            return { name: str(tr.name), description: str(tr.description) };
          })
        : [],
      error: typeof r.error === "string" ? r.error : undefined,
      durationMs: typeof r.durationMs === "number" ? r.durationMs : 0,
    };
  },
  async queryStore(query, online, limit = 60) {
    const r = (await McpHub.queryStore(query, online, limit)) as Record<string, unknown>;
    return {
      items: Array.isArray(r.items) ? r.items.map(parseStoreItem) : [],
      onlineUsed: r.onlineUsed === true,
      onlineError: typeof r.onlineError === "string" ? r.onlineError : undefined,
    };
  },
  async installStoreServer(request) {
    return parseDelta(await McpHub.installStoreServer(request as never));
  },
  async listBackups(limit = 40) {
    const fn = (McpHub as unknown as {
      listBackups?: (lim: number | null) => Promise<unknown>;
    }).listBackups;
    if (typeof fn !== "function") return [];
    const raw = await fn(limit ?? null);
    if (!Array.isArray(raw)) return [];
    return raw.map((item) => {
      const r = (item ?? {}) as Record<string, unknown>;
      return {
        id: str(r.id),
        fileName: str(r.fileName),
        targetPath: typeof r.targetPath === "string" ? r.targetPath : undefined,
        createdAt: str(r.createdAt),
        sizeBytes: typeof r.sizeBytes === "number" ? r.sizeBytes : 0,
        existed: r.existed === true,
      };
    });
  },
  async restoreBackup(backupId, targetPath = null) {
    const fn = (McpHub as unknown as {
      restoreBackup?: (id: string, target: string | null) => Promise<unknown>;
    }).restoreBackup;
    if (typeof fn !== "function") {
      throw new Error("restoreBackup bridge method unavailable");
    }
    const r = (await fn(backupId, targetPath ?? null)) as Record<string, unknown>;
    return {
      id: str(r.id),
      targetPath: str(r.targetPath),
      removedNewFile: r.removedNewFile === true,
      restoredBytes: typeof r.restoredBytes === "number" ? r.restoredBytes : 0,
    };
  },
};
