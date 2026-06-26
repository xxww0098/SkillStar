/**
 * Central command registry. Every Tauri `invoke` target used by the frontend
 * must be declared in one of these domain interfaces so the `tauriInvoke`
 * wrapper can type-check it.
 */
import type { AgentCommands } from "./agents";
import type { AiCommands } from "./ai";
import type { GitHubCommands } from "./github";
import type { MarketplaceCommands } from "./marketplace";
import type { McpCommands } from "./mcp";
import type { McpMarketplaceCommands } from "./mcpMarketplace";
import type { ModelsCommands } from "./models";
import type { ProjectCommands } from "./projects";
import type { S3Commands } from "./s3";
import type { SkillCommands } from "./skills";
import type { SshCommands } from "./ssh";
import type { StorageCommands } from "./storage";
import type { SystemCommands } from "./system";

export type TauriCommands = SkillCommands &
  AgentCommands &
  ProjectCommands &
  MarketplaceCommands &
  GitHubCommands &
  StorageCommands &
  AiCommands &
  ModelsCommands &
  McpCommands &
  McpMarketplaceCommands &
  SshCommands &
  S3Commands &
  SystemCommands;

export type {
  AgentCommands,
  AiCommands,
  GitHubCommands,
  MarketplaceCommands,
  McpCommands,
  McpMarketplaceCommands,
  ModelsCommands,
  ProjectCommands,
  S3Commands,
  SkillCommands,
  SshCommands,
  StorageCommands,
  SystemCommands,
};
export type { PatrolStatus, UpdateCheckResult, AcpConfig } from "./system";
export type { AgentDeployStatus, DeployKind } from "./agents";
export type { RepoCacheInfo } from "./storage";
export type { ConfigConflict, ToolInstallStatus } from "./models";
export type {
  AuthMethod,
  ConnectionTestResult,
  DiscoveryResult,
  HostKeyState,
  PushResult,
  RemoteAgentSkills,
  RemoteSkill,
  SshHost,
  SshHostListItem,
  SystemHost,
  TestConnectionOutput,
} from "./ssh";
export type {
  InstallOutcome as S3InstallOutcome,
  ManifestEntry,
  ManifestEntryView,
  S3ConnectionTestResult,
  S3InstallSummary,
  S3PushSummary,
  S3Target,
} from "./s3";
