import type { GitHubMirrorConfig, GitHubMirrorPreset, ProxyConfig } from "../../../types";

interface PatrolStatus {
  enabled: boolean;
  running: boolean;
  interval_secs: number;
  last_check: string | null;
}

interface AcpConfig {
  enabled: boolean;
  agent_command: string;
  agent_label: string;
}

interface UpdateCheckResult {
  available: boolean;
  version: string | null;
  date: string | null;
  body: string | null;
}

/** Result of an idempotent `export` write into `~/.zshrc` for Codex third_party auth. */
export interface ShellRcWriteResult {
  /** Absolute path of the rc file touched. */
  path: string;
  /** `false` when the file already had the exact target line (true no-op). */
  written: boolean;
  /** Timestamped backup path, when one was made. */
  backupPath: string | null;
  /** `"added"` | `"updated"` | `"noop"`. */
  action: "added" | "updated" | "noop";
}

/**
 * OS / shell adapter commands: file system, external open, tray language,
 * patrol, proxy, GitHub mirror, updater, ACP, and platform checks.
 */
export interface SystemCommands {
  // Files / shell
  write_text_file: { args: { path: string; content: string }; result: void };
  read_text_file: { args: { path: string }; result: string };
  open_folder: { args: { path: string }; result: void };
  open_external_url: { args: { url: string }; result: void };

  // App shell
  app_quit: { args: Record<string, never>; result: void };
  set_dock_visible: { args: { visible: boolean }; result: void };
  update_tray_language: { args: { lang: string }; result: void };
  check_developer_mode: { args: Record<string, never>; result: boolean };

  // Patrol
  start_patrol: { args: { intervalSecs: number }; result: void };
  stop_patrol: { args: Record<string, never>; result: void };
  get_patrol_status: { args: Record<string, never>; result: PatrolStatus };
  set_patrol_enabled: { args: { enabled: boolean }; result: void };

  // Proxy
  get_proxy_config: { args: Record<string, never>; result: ProxyConfig };
  save_proxy_config: { args: { config: ProxyConfig }; result: void };

  // GitHub mirror
  get_github_mirror_config: { args: Record<string, never>; result: GitHubMirrorConfig };
  save_github_mirror_config: { args: { config: GitHubMirrorConfig }; result: void };
  get_github_mirror_presets: { args: Record<string, never>; result: GitHubMirrorPreset[] };
  test_github_mirror: { args: { url: string }; result: number };

  // Shell rc (Codex third_party auth env export)
  write_codex_env_to_zshrc: { args: { envKey: string; value: string }; result: ShellRcWriteResult };
  read_codex_env_from_zshrc: { args: { envKey: string }; result: string | null };

  // Updater
  check_app_update: { args: Record<string, never>; result: UpdateCheckResult };
  download_and_install_update: { args: Record<string, never>; result: void };
  restart_after_update: { args: Record<string, never>; result: void };

  // ACP
  get_acp_config: { args: Record<string, never>; result: AcpConfig };
  save_acp_config: { args: { config: AcpConfig }; result: void };
}

export type { AcpConfig, PatrolStatus, UpdateCheckResult };
