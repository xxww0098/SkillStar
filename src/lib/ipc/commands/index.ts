/**
 * Central command registry. Every Tauri `invoke` target used by the frontend
 * must be declared in one of these domain interfaces so the `tauriInvoke`
 * wrapper can type-check it.
 */
import type { AgentCommands } from "./agents";
import type { AiCommands } from "./ai";
import type { DecisionCommands } from "./decision";
import type { GitHubCommands } from "./github";
import type { MarketplaceCommands } from "./marketplace";
import type { ModelsCommands } from "./models";
import type { ProjectCommands } from "./projects";
import type { SharedChannelCommands } from "./sharedChannels";
import type { SkillCommands } from "./skills";
import type { SshCommands } from "./ssh";
import type { StorageCommands } from "./storage";
import type { InstanceCommands } from "./instances";
import type { SystemCommands } from "./system";

export type TauriCommands = SkillCommands &
  DecisionCommands &
  AgentCommands &
  ProjectCommands &
  MarketplaceCommands &
  GitHubCommands &
  StorageCommands &
  AiCommands &
  ModelsCommands &
  SshCommands &
  SharedChannelCommands &
  SystemCommands &
  InstanceCommands;

export type {
  AgentCommands,
  AiCommands,
  DecisionCommands,
  GitHubCommands,
  InstanceCommands,
  MarketplaceCommands,
  ModelsCommands,
  ProjectCommands,
  SharedChannelCommands,
  SkillCommands,
  SshCommands,
  StorageCommands,
  SystemCommands,
};
export type { PatrolStatus, UpdateCheckResult } from "./system";
export type { AgentDeployStatus, DeployKind } from "./agents";
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
