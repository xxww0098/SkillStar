import type { CacheCleanResult, StorageOverview } from "../../../types";

interface RepoCacheInfo {
  total_bytes: number;
  repo_count: number;
  unused_count: number;
  unused_bytes: number;
}

/** Storage overview, cache maintenance, and force-delete operations. */
export interface StorageCommands {
  get_storage_overview: { args: Record<string, never>; result: StorageOverview };
  get_repo_cache_info: { args: Record<string, never>; result: RepoCacheInfo };
  clean_repo_cache: { args: Record<string, never>; result: number };
  clear_all_caches: { args: Record<string, never>; result: CacheCleanResult };
  force_delete_installed_skills: { args: Record<string, never>; result: number };
  force_delete_repo_caches: { args: Record<string, never>; result: number };
  force_delete_app_config: { args: Record<string, never>; result: number };
}

export type { RepoCacheInfo };
