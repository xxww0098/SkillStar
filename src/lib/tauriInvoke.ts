/**
 * Type-safe wrappers for Tauri `invoke()` calls.
 *
 * Instead of raw `invoke<T>("command_name", { args })`, use:
 * - `tauriInvoke("command_name", { args })` — type-checked command + args + return type
 * - `useTauriQuery("command_name", { args })` — React Query wrapper
 * - `useTauriMutation("command_name")` — React Query mutation wrapper
 *
 * The `TauriCommands` interface maps every backend command to its arg/result types,
 * so typos and arg mismatches are caught at compile time.
 */

import { type UseQueryOptions, useMutation, useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentProfile,
  AiConfig,
  AiKeywordSearchResult,
  AiPickResponse,
  BundleManifest,
  CacheCleanResult,
  CustomProfileDef,
  GhStatus,
  ImportBundleResult,
  ImportMultiBundleResult,
  ImportResult,
  ImportTarget,
  LocalFirstResult,
  MarketplaceResult,
  MarketplaceSkillDetails,
  MultiManifest,
  OfficialPublisher,
  ProjectAgentDetection,
  ProjectDeployMode,
  ProjectEntry,
  ProjectScanResult,
  ProxyConfig,
  PublisherRepo,
  PublisherRepoSkill,
  PublishResult,
  RepoHistoryEntry,
  ScanResult,
  SecurityScanAuditDetail,
  SecurityScanAuditSummary,
  SecurityScanEstimate,
  SecurityScanLogEntry,
  SecurityScanPolicy,
  SecurityScanReportExportResult,
  SecurityScanResult,
  ShortTextTranslationResult,
  SkillTranslationResult,
  Skill,
  SkillCardDeck,
  SkillContent,
  SkillInstallTarget,
  SkillsList,
  SkillUpdateState,
  StorageOverview,
  SyncStateEntry,
  TranslationReadiness,
  TranslationSettings,
  UpdateResult,
  UserRepo,
} from "../types";

// ── PatrolStatus (not exported from types yet) ──────────────────────
interface PatrolStatus {
  enabled: boolean;
  running: boolean;
  interval_secs: number;
  last_check: string | null;
}

interface RepoCacheInfo {
  total_bytes: number;
  repo_count: number;
  unused_count: number;
  unused_bytes: number;
}

// ── Command Type Map ────────────────────────────────────────────────
// Every Tauri command is listed here with its args and return type.
// Add new commands here when adding them to the backend.

interface TauriCommands {
  // Skills
  list_skills: { args: Record<string, never>; result: Skill[] };
  refresh_skill_updates: { args: { names?: string[] }; result: SkillUpdateState[] };
  install_skill: { args: { url: string; name?: string }; result: Skill };
  uninstall_skill: { args: { name: string }; result: void };
  toggle_skill_for_agent: {
    args: { skillName: string; agentId: string; enable: boolean };
    result: void;
  };
  update_skill: { args: { name: string }; result: UpdateResult };
  read_skill_file_raw: { args: { name: string }; result: string };
  create_local_skill_from_content: { args: { name: string; content: string }; result: void };
  create_local_skill: { args: { name: string; content?: string }; result: Skill };
  delete_local_skill: { args: { name: string }; result: void };
  migrate_local_skills: { args: Record<string, never>; result: number };
  list_skill_files: { args: { name: string }; result: string[] };
  read_skill_content: { args: { name: string }; result: SkillContent };
  update_skill_content: { args: { name: string; content: string }; result: void };

  // Skill Groups
  list_skill_groups: { args: Record<string, never>; result: SkillCardDeck[] };
  create_skill_group: {
    args: {
      name: string;
      description: string;
      icon: string;
      skills: string[];
      skillSources?: Record<string, string>;
    };
    result: SkillCardDeck;
  };
  update_skill_group: {
    args: {
      id: string;
      name?: string;
      description?: string;
      icon?: string;
      skills?: string[];
      skillSources?: Record<string, string>;
    };
    result: SkillCardDeck;
  };
  delete_skill_group: { args: { id: string }; result: void };
  duplicate_skill_group: { args: { id: string }; result: SkillCardDeck };
  deploy_skill_group: {
    args: { groupId: string; projectPath: string; agentTypes: string[] };
    result: number;
  };

  // Agents
  list_agent_profiles: { args: Record<string, never>; result: AgentProfile[] };
  toggle_agent_profile: { args: { id: string }; result: boolean };
  unlink_all_skills_from_agent: { args: { agentId: string }; result: number };
  batch_link_skills_to_agent: {
    args: { skillNames: string[]; agentId: string };
    result: number;
  };
  list_linked_skills: { args: { agentId: string }; result: string[] };
  unlink_skill_from_agent: { args: { skillName: string; agentId: string }; result: void };
  batch_remove_skills_from_all_agents: { args: { skillNames: string[] }; result: void };
  add_custom_agent_profile: { args: { def: CustomProfileDef }; result: void };
  remove_custom_agent_profile: { args: { id: string }; result: void };

  // Projects
  create_project_skills: {
    args: { projectPath: string; selectedSkills: string[]; agentTypes: string[] };
    result: number;
  };
  register_project: { args: { projectPath: string }; result: ProjectEntry };
  list_projects: { args: Record<string, never>; result: ProjectEntry[] };
  get_project_skills: { args: { name: string }; result: SkillsList | null };
  save_and_sync_project: {
    args: {
      projectPath: string;
      agents: Record<string, string[]>;
      deployModes?: Record<string, ProjectDeployMode>;
    };
    result: number;
  };
  save_project_skills_list: {
    args: { projectPath: string; agents: Record<string, string[]> };
    result: SkillsList;
  };
  update_project_path: { args: { name: string; newPath: string }; result: number };
  remove_project: { args: { name: string }; result: void };
  scan_project_skills: { args: { projectPath: string }; result: ProjectScanResult };
  rebuild_project_skills_from_disk: { args: { projectPath: string }; result: SkillsList };
  import_project_skills: {
    args: { projectPath: string; projectName: string; targets: ImportTarget[] };
    result: ImportResult;
  };
  detect_project_agents: { args: { projectPath: string }; result: ProjectAgentDetection };

  // Marketplace (remote)
  search_skills_sh: { args: { query: string }; result: MarketplaceResult };
  get_skills_sh_leaderboard: { args: { category: string }; result: Skill[] };
  get_official_publishers: { args: Record<string, never>; result: OfficialPublisher[] };
  get_publisher_repos: { args: { publisherName: string }; result: PublisherRepo[] };
  get_publisher_repo_skills: {
    args: { publisherName: string; repoName: string };
    result: PublisherRepoSkill[];
  };
  get_marketplace_skill_details: {
    args: { source: string; name: string };
    result: MarketplaceSkillDetails;
  };
  resolve_skill_sources: {
    args: { names: string[]; existingSources: Record<string, string> };
    result: Record<string, string>;
  };
  ai_extract_search_keywords: { args: { query: string }; result: string[] };
  ai_search_with_keywords: { args: { keywords: string[] }; result: AiKeywordSearchResult };

  // Marketplace (local-first)
  get_leaderboard_local: { args: { category: string }; result: LocalFirstResult<Skill[]> };
  search_marketplace_local: {
    args: { query: string; limit?: number };
    result: LocalFirstResult<Skill[]>;
  };
  get_publishers_local: {
    args: Record<string, never>;
    result: LocalFirstResult<OfficialPublisher[]>;
  };
  get_publisher_repos_local: {
    args: { publisherName: string };
    result: LocalFirstResult<PublisherRepo[]>;
  };
  get_repo_skills_local: { args: { source: string }; result: LocalFirstResult<Skill[]> };
  get_skill_detail_local: {
    args: { source: string; name: string };
    result: LocalFirstResult<MarketplaceSkillDetails>;
  };
  ai_search_marketplace_local: {
    args: { keywords: string[]; limit?: number };
    result: LocalFirstResult<AiKeywordSearchResult>;
  };
  sync_marketplace_scope: { args: { scope: string }; result: void };
  get_marketplace_sync_states: { args: Record<string, never>; result: SyncStateEntry[] };

  // GitHub
  check_gh_installed: { args: Record<string, never>; result: boolean };
  check_gh_status: { args: Record<string, never>; result: GhStatus };
  publish_skill_to_github: {
    args: {
      skillName: string;
      description: string;
      isPublic: boolean;
      existingRepoUrl?: string;
      folderName: string;
      repoName: string;
    };
    result: PublishResult;
  };
  list_user_repos: { args: { limit?: number }; result: UserRepo[] };
  inspect_repo_folders: { args: { repoFullName: string }; result: string[] };
  scan_github_repo: { args: { url: string; fullDepth?: boolean }; result: ScanResult };
  install_from_scan: {
    args: { repoUrl: string; source: string; skills: SkillInstallTarget[] };
    result: string[];
  };
  list_repo_history: { args: Record<string, never>; result: RepoHistoryEntry[] };
  get_repo_cache_info: { args: Record<string, never>; result: RepoCacheInfo };
  clean_repo_cache: { args: Record<string, never>; result: number };
  get_storage_overview: { args: Record<string, never>; result: StorageOverview };
  clear_all_caches: { args: Record<string, never>; result: CacheCleanResult };
  force_delete_installed_skills: { args: Record<string, never>; result: number };
  force_delete_repo_caches: { args: Record<string, never>; result: number };
  force_delete_app_config: { args: Record<string, never>; result: number };
  clean_broken_skills: { args: Record<string, never>; result: number };

  // AI
  get_ai_config: { args: Record<string, never>; result: AiConfig };
  save_ai_config: { args: { config: AiConfig }; result: void };
  get_translation_settings: { args: Record<string, never>; result: TranslationSettings };
  save_translation_settings: { args: { settings: TranslationSettings }; result: void };
  get_translation_readiness: { args: Record<string, never>; result: TranslationReadiness };
  ai_translate_skill: { args: { content: string; force?: boolean; forceQuality?: boolean }; result: string };
  ai_translate_skill_stream: {
    args: { content: string; requestId: string; force?: boolean; forceQuality?: boolean };
    result: SkillTranslationResult;
  };

  ai_translate_short_text_stream_with_source: {
    args: { content: string; requestId: string; forceRefresh?: boolean; forceAi?: boolean };
    result: ShortTextTranslationResult;
  };
  ai_retranslate_short_text_stream_with_source: {
    args: { content: string; requestId: string };
    result: ShortTextTranslationResult;
  };
  ai_summarize_skill: { args: { name: string }; result: string };
  ai_summarize_skill_stream: { args: { name: string; requestId: string }; result: void };
  ai_test_connection: { args: Record<string, never>; result: number };
  ai_pick_skills: {
    args: { taskDescription: string; skillNames: string[] };
    result: AiPickResponse;
  };
  ai_batch_process_skills: { args: { skillNames: string[] }; result: void };
  check_pending_batch_translate: {
    args: { skillNames: string[] };
    result: string[];
  };
  ai_security_scan: {
    args: { requestId: string; skillNames: string[]; force: boolean; mode: string };
    result: SecurityScanResult[];
  };
  estimate_security_scan: {
    args: { skillNames: string[]; mode: string };
    result: SecurityScanEstimate;
  };
  cancel_security_scan: { args: Record<string, never>; result: void };
  get_cached_scan_results: {
    args: Record<string, never>;
    result: SecurityScanResult[];
  };
  clear_security_scan_cache: { args: Record<string, never>; result: void };
  list_security_scan_logs: {
    args: { limit?: number };
    result: SecurityScanLogEntry[];
  };
  list_security_scan_audits: {
    args: { limit?: number };
    result: SecurityScanAuditSummary[];
  };
  get_security_scan_audit_detail: {
    args: { fileName: string };
    result: SecurityScanAuditDetail;
  };
  get_security_scan_log_dir: { args: Record<string, never>; result: string };
  get_security_scan_policy: { args: Record<string, never>; result: SecurityScanPolicy };
  save_security_scan_policy: { args: { policy: SecurityScanPolicy }; result: void };
  export_security_scan_sarif: {
    args: { skillNames?: string[]; requestLabel?: string };
    result: SecurityScanReportExportResult;
  };
  export_security_scan_report: {
    args: { format: string; skillNames?: string[]; requestLabel?: string };
    result: SecurityScanReportExportResult;
  };

  // Bundles
  export_skill_bundle: { args: { name: string; outputPath?: string }; result: string };
  preview_skill_bundle: { args: { filePath: string }; result: BundleManifest };
  import_skill_bundle: {
    args: { filePath: string; force: boolean };
    result: ImportBundleResult;
  };
  export_multi_skill_bundle: {
    args: { names: string[]; outputPath: string };
    result: string;
  };
  preview_multi_skill_bundle: { args: { filePath: string }; result: MultiManifest };
  import_multi_skill_bundle: {
    args: { filePath: string; force: boolean };
    result: ImportMultiBundleResult;
  };

  // Files & System
  write_text_file: { args: { path: string; content: string }; result: void };
  read_text_file: { args: { path: string }; result: string };
  open_folder: { args: { path: string }; result: void };

  // Patrol
  start_patrol: { args: { intervalSecs: number }; result: void };
  stop_patrol: { args: Record<string, never>; result: void };
  get_patrol_status: { args: Record<string, never>; result: PatrolStatus };
  set_patrol_enabled: { args: { enabled: boolean }; result: void };
  app_quit: { args: Record<string, never>; result: void };
  set_dock_visible: { args: { visible: boolean }; result: void };

  // Proxy
  get_proxy_config: { args: Record<string, never>; result: ProxyConfig };
  save_proxy_config: { args: { config: ProxyConfig }; result: void };

  // Tray
  update_tray_language: { args: { lang: string }; result: void };
}

// ── Type-Safe Invoke ────────────────────────────────────────────────

/**
 * Type-safe `invoke()` wrapper. Provides compile-time validation of
 * command name, argument types, and return type.
 *
 * ```ts
 * // ✅ Type-checked
 * const skills = await tauriInvoke("list_skills");
 * const skill = await tauriInvoke("install_skill", { url: "https://..." });
 *
 * // ❌ Compile error — wrong arg name
 * await tauriInvoke("install_skill", { uri: "https://..." });
 * ```
 */
export function tauriInvoke<K extends keyof TauriCommands>(
  command: K,
  ...args: TauriCommands[K]["args"] extends Record<string, never> ? [] : [TauriCommands[K]["args"]]
): Promise<TauriCommands[K]["result"]> {
  return invoke<TauriCommands[K]["result"]>(command, args[0] as Record<string, unknown>);
}

// ── React Query Wrappers ────────────────────────────────────────────

type QueryCommands = {
  [K in keyof TauriCommands]: TauriCommands[K]["args"] extends Record<string, never> ? K : never;
}[keyof TauriCommands];

/**
 * React Query wrapper for Tauri commands with no arguments.
 *
 * ```ts
 * const { data: skills } = useTauriQuery("list_skills");
 * ```
 */
export function useTauriQuery<K extends QueryCommands>(
  command: K,
  options?: Omit<UseQueryOptions<TauriCommands[K]["result"], Error>, "queryKey" | "queryFn">,
) {
  return useQuery<TauriCommands[K]["result"], Error>({
    queryKey: [command],
    queryFn: () => invoke<TauriCommands[K]["result"]>(command),
    ...options,
  });
}

/**
 * React Query wrapper for Tauri commands that take arguments.
 *
 * ```ts
 * const { data } = useTauriQueryWithArgs("read_skill_content", { name: "foo" });
 * ```
 */
export function useTauriQueryWithArgs<K extends keyof TauriCommands>(
  command: K,
  args: TauriCommands[K]["args"],
  options?: Omit<UseQueryOptions<TauriCommands[K]["result"], Error>, "queryKey" | "queryFn">,
) {
  return useQuery<TauriCommands[K]["result"], Error>({
    queryKey: [command, args],
    queryFn: () => invoke<TauriCommands[K]["result"]>(command, args as Record<string, unknown>),
    ...options,
  });
}

/**
 * React Query mutation wrapper for Tauri commands.
 *
 * ```ts
 * const install = useTauriMutation("install_skill");
 * await install.mutateAsync({ url: "https://..." });
 * ```
 */
export function useTauriMutation<K extends keyof TauriCommands>(command: K) {
  return useMutation<TauriCommands[K]["result"], Error, TauriCommands[K]["args"]>({
    mutationFn: (args) => invoke<TauriCommands[K]["result"]>(command, args as Record<string, unknown>),
  });
}

export type { PatrolStatus, RepoCacheInfo, TauriCommands };
