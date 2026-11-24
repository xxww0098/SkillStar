export type McpTransport = "stdio" | "http" | "sse";
export type ConflictStrategy = "rename" | "replace" | "keep";

export interface McpServerRecord {
  name: string;
  transport: McpTransport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
  paused: boolean;
  agents: string[];
  disabledTools: string[];
  startupTimeoutSec?: number;
  marketId?: string;
  updatedAt?: string;
}

export interface McpAgentTarget {
  id: string;
  name: string;
  installed: boolean;
  sharedHere: boolean;
  configPath: string;
  format: string;
  noRemote: boolean;
  noSse: boolean;
  supportsToolsOff: boolean;
  supportsStartupTimeout: boolean;
  notes?: string;
}

export interface FoundServer {
  agentId: string;
  agentName: string;
  configPath: string;
  format: string;
  server: McpServerRecord;
}

export interface SyncWarning {
  agentId: string;
  path: string;
  code: string;
  message: string;
}

export interface LibrarySnapshot {
  servers: McpServerRecord[];
  agents: McpAgentTarget[];
  found: FoundServer[];
  warnings: SyncWarning[];
}

export interface LibraryDelta {
  servers: McpServerRecord[];
  warnings: SyncWarning[];
}

export interface DiscoveredTool {
  name: string;
  description: string;
}

export interface CheckResult {
  ok: boolean;
  serverName: string;
  serverVersion: string;
  tools: DiscoveredTool[];
  error?: string;
  durationMs: number;
}

export interface ProjectSnippet {
  agentId: string;
  agentName: string;
  relPath: string;
  format: string;
  content: string;
}

export interface StoreInputSpec {
  key: string;
  whereField: string;
  label: string;
  description: string;
  placeholder: string;
  secret: boolean;
  required: boolean;
  format: string;
}

export interface StoreItem {
  id: string;
  name: string;
  title: string;
  description: string;
  publisher: string;
  icon: string;
  homepage: string;
  transport: McpTransport;
  runs: string;
  signIn: boolean;
  featured: boolean;
  matchByAddr: boolean;
  source: string;
  stars?: number;
  have?: string;
  inputs: StoreInputSpec[];
  template: McpServerRecord;
}

export interface StoreSearchResponse {
  items: StoreItem[];
  onlineUsed: boolean;
  onlineError?: string;
}

export interface StoreInstallRequest {
  item: StoreItem;
  name?: string;
  values: Record<string, string>;
  agents: string[];
}

export interface ParsedPasteResult {
  servers: McpServerRecord[];
  detectedKind: string;
}

export interface BackupEntry {
  id: string;
  fileName: string;
  targetPath?: string;
  createdAt: string;
  sizeBytes: number;
  existed: boolean;
}

export interface BackupRestoreResult {
  id: string;
  targetPath: string;
  removedNewFile: boolean;
  restoredBytes: number;
}
