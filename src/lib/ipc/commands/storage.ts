import type { CacheCleanResult, StorageOverview } from "../../../types";

/** Storage overview, cache maintenance, and force-delete operations. */
export interface StorageCommands {
  get_storage_overview: { args: Record<string, never>; result: StorageOverview };
  clear_all_caches: { args: Record<string, never>; result: CacheCleanResult };
  force_delete_installed_skills: { args: Record<string, never>; result: number };
  force_delete_repo_caches: { args: Record<string, never>; result: number };
  force_delete_app_config: { args: Record<string, never>; result: number };
}
