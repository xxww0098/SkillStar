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
import type { SkillCommands } from "./skills";
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
  SkillCommands,
  StorageCommands,
  SystemCommands,
};
export type { PatrolStatus, UpdateCheckResult, AcpConfig } from "./system";
export type { AgentDeployStatus, DeployKind } from "./agents";
export type { RepoCacheInfo } from "./storage";
export type { ConfigConflict, ToolInstallStatus } from "./models";
