/**
 * Dev-mock fragment: settings & system config — AI summarize
 * config, proxy, GitHub mirror, ACP tutorial agent, and storage overview.
 * Small data consts are colocated; the ACP config store lives in ./shared.ts
 * because the skills fragment reads it for tutorial metadata.
 */

import { type AcpConfigState, type DevMockHandlers, getAcpConfigState, setAcpConfigState } from "./shared";

export const AI_CONFIG = {
  enabled: true,
  api_format: "openai" as const,
  provider_ref: null,
  base_url: "https://api.deepseek.com/v1",
  api_key: "sk-demo-ai",
  model: "deepseek-chat",
  context_window_k: 128,
  max_concurrent_requests: 4,
  openai_preset: {
    base_url: "https://api.deepseek.com/v1",
    api_key: "sk-demo",
    model: "deepseek-chat",
  },
  anthropic_preset: {
    base_url: "https://api.anthropic.com",
    api_key: "",
    model: "claude-sonnet-4-6",
  },
  local_preset: {
    base_url: "http://localhost:11434/v1",
    api_key: "",
    model: "qwen2.5",
  },
};

export const STORAGE_OVERVIEW = {
  data_root_path: "/Users/dev/.skillstar",
  hub_root_path: "/Users/dev/.skillstar/hub",
  is_hub_under_data: true,
  config_bytes: 245_000,
  config_path: "/Users/dev/.skillstar/config",
  hub_bytes: 18_400_000,
  hub_path: "/Users/dev/.skillstar/hub",
  hub_count: 5,
  broken_count: 0,
  local_count: 1,
  local_bytes: 120_000,
  local_path: "/Users/dev/.skillstar/local",
  cache_bytes: 64_200_000,
  cache_path: "/Users/dev/.skillstar/cache",
  cache_count: 8,
  cache_unused_count: 2,
  cache_unused_bytes: 9_800_000,
  history_count: 14,
};

export const SETTINGS_HANDLERS: DevMockHandlers = {
  // ── AI config ──
  get_ai_config: () => AI_CONFIG,
  ai_test_connection: () => 220,

  // ── Settings / system ──
  get_proxy_config: () => ({
    enabled: false,
    proxy_type: "http",
    host: "",
    port: 7890,
    username: null,
    password: null,
    bypass:
      "localhost,127.0.0.1,::1,.local,.deepseek.com,.zhipuai.cn,.bigmodel.cn,.moonshot.cn,.minimax.io,.volces.com,.aliyuncs.com",
  }),
  get_github_mirror_config: () => ({
    enabled: false,
    preset_id: "ghproxy_vip",
    custom_url: null,
  }),
  get_github_mirror_presets: () => [
    {
      id: "ghproxy_vip",
      name: "ghproxy.link",
      url: "https://ghproxy.link/",
      supports_clone: true,
    },
    {
      id: "gh_proxy",
      name: "gh-proxy.com",
      url: "https://gh-proxy.com/",
      supports_clone: true,
    },
  ],
  get_marketplace_mirror_config: () => ({
    enabled: false,
    hosts: [],
  }),
  save_marketplace_mirror_config: (args) => {
    const config = args.config as { enabled: boolean; hosts: string[] };
    void config;
    return undefined;
  },
  diagnose_network: () => ({
    proxy_enabled: false,
    proxy_type: null,
    checks: [
      {
        id: "github",
        label: "GitHub",
        url: "https://github.com/",
        status: "ok",
        latency_ms: 120,
        detail: "HTTP 200",
      },
    ],
    recommendations: [],
  }),
  get_acp_config: () => ({ ...getAcpConfigState() }),
  save_acp_config: (args) => {
    const config = args.config as AcpConfigState;
    setAcpConfigState(config);
    return undefined;
  },
  get_storage_overview: () => STORAGE_OVERVIEW,
  get_repo_cache_info: () => ({
    total_bytes: 64_200_000,
    repo_count: 8,
    unused_count: 2,
    unused_bytes: 9_800_000,
  }),
};
