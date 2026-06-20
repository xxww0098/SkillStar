//! config domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export type ProxyType = "http" | "https" | "socks5";

export interface ProxyConfig {
  enabled: boolean;
  proxy_type: ProxyType;
  host: string;
  port: number;
  username: string | null;
  password: string | null;
  bypass: string | null;
}

export interface GitHubMirrorPreset {
  id: string;
  name: string;
  url: string;
  supports_clone: boolean;
}

export interface GitHubMirrorConfig {
  enabled: boolean;
  preset_id: string | null;
  custom_url: string | null;
}

export interface StorageOverview {
  data_root_path: string;
  hub_root_path: string;
  is_hub_under_data: boolean;
  config_bytes: number;
  config_path: string;
  hub_bytes: number;
  hub_path: string;
  hub_count: number;
  broken_count: number;
  local_count: number;
  local_bytes: number;
  local_path: string;
  cache_bytes: number;
  cache_path: string;
  cache_count: number;
  cache_unused_count: number;
  cache_unused_bytes: number;
  history_count: number;
}

export interface CacheCleanResult {
  repos_removed: number;
  history_cleared: number;
  translation_cleared: number;
}

// ── GitHub Publish ──────────────────────────────────────────────────
