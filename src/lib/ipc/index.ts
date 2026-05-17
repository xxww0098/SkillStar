/**
 * Typed Tauri IPC layer. All frontend calls to the Rust backend flow through
 * `tauriInvoke` or the React Query wrappers exported here. Commands are
 * declared per domain in `./commands/*.ts`.
 *
 * Do NOT import `@tauri-apps/api/core` directly in feature code.
 */
export { tauriInvoke, tauriInvokeDynamic, useTauriMutation, useTauriQuery, useTauriQueryWithArgs } from "./core";
export type {
  AcpConfig,
  AgentCommands,
  AiCommands,
  ConfigConflict,
  GitHubCommands,
  MarketplaceCommands,
  ModelsCommands,
  PatrolStatus,
  ProjectCommands,
  RepoCacheInfo,
  SkillCommands,
  StorageCommands,
  SystemCommands,
  TauriCommands,
  ToolInstallStatus,
  UpdateCheckResult,
} from "./commands";
