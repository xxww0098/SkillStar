import type {
  GitHubMirrorConfig,
  GitHubMirrorPreset,
  MarketplaceMirrorConfig,
  NetworkDiagnosis,
  ProxyConfig,
  SkillTutorialStyle,
} from "../../../types";

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
  tutorial_style: SkillTutorialStyle;
}

interface UpdateCheckResult {
  available: boolean;
  version: string | null;
  date: string | null;
  body: string | null;
  /** GitHub release page for manual download (github-release mode). */
  release_url: string | null;
}

/**
 * OS / shell adapter commands: file system, external open, tray language,
 * patrol, proxy, GitHub mirror, updater, ACP, and platform checks.
 */
export interface SystemCommands {
  // Files / shell
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

  // Marketplace mirror (skills.sh accelerators)
  get_marketplace_mirror_config: { args: Record<string, never>; result: MarketplaceMirrorConfig };
  save_marketplace_mirror_config: { args: { config: MarketplaceMirrorConfig }; result: void };
  diagnose_network: { args: Record<string, never>; result: NetworkDiagnosis };

  // Updater
  check_app_update: { args: Record<string, never>; result: UpdateCheckResult };
  download_and_install_update: { args: Record<string, never>; result: void };
  restart_after_update: { args: Record<string, never>; result: void };

  // ACP
  get_acp_config: { args: Record<string, never>; result: AcpConfig };
  save_acp_config: { args: { config: AcpConfig }; result: void };
}

export type { AcpConfig, PatrolStatus, UpdateCheckResult };
