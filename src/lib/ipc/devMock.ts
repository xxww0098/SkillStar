/**
 * DEV-ONLY browser fallback for the Tauri IPC layer.
 *
 * When the frontend runs OUTSIDE the Tauri shell (e.g. plain `vite` opened in a
 * browser for UI iteration), production rejects every `invoke()`. That makes the
 * whole app unusable in the browser. In DEV builds we instead serve realistic
 * sample data here so every screen renders populated — enabling fast visual
 * design work without a full `tauri dev` rebuild.
 *
 * This module is imported dynamically and ONLY from the `import.meta.env.DEV`
 * branch in `core.ts`, so it is dead-code-eliminated from production bundles and
 * is NEVER reachable inside the real Tauri shell. It must not be used in tests
 * (tests mock at their own layer).
 *
 * The sample-data consts live in `./devMockData` to keep each module small.
 */

import {
  AGENTS,
  AI_CONFIG,
  DECKS,
  FLAT_PROVIDERS,
  iso,
  MARKET_SKILLS,
  mcpMarketDraft,
  MCP_MARKET,
  MCP_MARKET_DETAILS,
  MCP_PRESETS,
  MCP_STORE,
  MCP_TOOL_STATUSES,
  PRESETS_FLAT,
  PROJECTS,
  SAMPLE_SKILLS,
  STORAGE_OVERVIEW,
  USAGE_ALERTS,
  USAGE_CATALOG,
  USAGE_SUBSCRIPTIONS,
  USAGE_SUMMARY,
} from "./devMockData";

const HANDLERS: Record<string, (args: Record<string, unknown>) => unknown> = {
  // ── Global / app shell ──
  list_agent_profiles: () => AGENTS,
  toggle_agent_profile: () => true,
  get_patrol_status: () => ({ enabled: true, running: true, interval_secs: 3600, last_check: iso(0) }),
  set_patrol_enabled: () => undefined,
  check_developer_mode: () => true,
  check_app_update: () => ({ available: false, version: null, date: null, body: null }),

  // ── Skills mode ──
  list_skills: () => SAMPLE_SKILLS,
  refresh_skill_updates: () =>
    SAMPLE_SKILLS.filter((s) => s.update_available).map((s) => ({ name: s.name, update_available: true })),
  check_new_repo_skills: () => [],
  get_dismissed_new_skills: () => [],
  read_skill_content: (args) => ({
    name: String((args?.name as string) ?? "pdf-tools"),
    description: "Read, merge, split, and OCR PDF files with a single command.",
    triggers: ["pdf", "ocr", "merge pdf"],
    scopes: ["files"],
    "allowed-tools": ["Bash", "Read"],
    content:
      "# PDF Tools\n\nA skill for working with **PDF** files — read, merge, split and OCR.\n\n## Features\n\n- Merge & split documents\n- OCR scanned pages\n- Fill interactive forms\n\n```bash\nskillstar run pdf-tools merge a.pdf b.pdf -o out.pdf\n```\n\n> Tip: pair with the `xlsx` skill for spreadsheet exports. See the [docs](https://example.com).",
  }),
  read_skill_file_raw: (args) =>
    `---\nname: ${String((args?.name as string) ?? "pdf-tools")}\ndescription: Read, merge, split, and OCR PDF files.\n---\n\n# PDF Tools\n\nA skill for working with **PDF** files.`,
  list_skill_files: () => ["SKILL.md", "scripts/merge.py", "README.md"],
  list_skill_groups: () => DECKS,
  list_projects: () => PROJECTS,
  get_project_skills: () => ({ agents: { claude: ["pdf-tools", "xlsx"] }, updated_at: iso(1) }),
  detect_project_agents: () => ({
    detected: AGENTS.filter((a) => a.project_skills_rel).map((a) => ({
      agent_id: a.id,
      display_name: a.display_name,
      icon: a.icon,
      project_skills_rel: a.project_skills_rel,
      exists: a.enabled,
    })),
    ambiguous_groups: [],
    auto_enable: ["claude"],
  }),

  // ── Marketplace ──
  list_marketplace_skills_local: () => ({ data: MARKET_SKILLS, snapshot_status: "fresh", snapshot_updated_at: iso(0) }),
  get_leaderboard_local: () => ({ data: MARKET_SKILLS, snapshot_status: "fresh", snapshot_updated_at: iso(0) }),
  get_publishers_local: () => ({
    data: [
      {
        name: "anthropics",
        repo: "anthropics/skills",
        repo_count: 3,
        skill_count: 24,
        url: "https://github.com/anthropics/skills",
      },
      {
        name: "community",
        repo: "community/skills",
        repo_count: 5,
        skill_count: 41,
        url: "https://github.com/community/skills",
      },
    ],
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  get_marketplace_sync_states: () => [],
  search_marketplace_local: (args) => {
    const q = String((args?.query as string) ?? "").toLowerCase();
    return {
      data: MARKET_SKILLS.filter((s) => s.name.includes(q) || s.description.toLowerCase().includes(q)),
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },

  // ── MCP ──
  list_mcp_servers: () => MCP_STORE,
  mcp_tool_statuses: () => MCP_TOOL_STATUSES,
  get_mcp_presets: () => MCP_PRESETS,

  // MCP marketplace (GitHub MCP Registry)
  list_mcp_market_servers_local: () => ({
    data: MCP_MARKET,
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  search_mcp_market_local: (args) => {
    const q = String((args?.query as string) ?? "").toLowerCase();
    const data = q
      ? MCP_MARKET.filter((m) => m.name.toLowerCase().includes(q) || m.description.toLowerCase().includes(q))
      : MCP_MARKET;
    return { data, snapshot_status: "fresh", snapshot_updated_at: iso(0) };
  },
  get_mcp_market_server_detail_local: (args) => {
    const id = String((args?.id as string) ?? "");
    const entry = MCP_MARKET.find((m) => m.id === id);
    const detail = MCP_MARKET_DETAILS[id];
    return {
      data: entry && detail ? { ...entry, ...detail } : null,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  sync_mcp_market_scope: () => undefined,
  get_mcp_market_sync_states: () => [
    {
      scope: "mcp_registry",
      last_success_at: iso(0),
      last_attempt_at: iso(0),
      last_error: null,
      next_refresh_at: iso(-0.5),
      schema_version: 8,
    },
  ],
  mcp_market_entry_to_draft: (args) => mcpMarketDraft(String((args?.id as string) ?? "")),

  // ── Models ──
  get_providers_flat: () => FLAT_PROVIDERS,
  get_tool_activations: () => FLAT_PROVIDERS.tool_activations,
  get_provider_presets_flat: () => PRESETS_FLAT,
  get_provider_presets: () =>
    PRESETS_FLAT.map((p) => ({
      id: p.id,
      name: p.name,
      base_url: p.base_url_openai,
      api_key_url: p.api_key_url ?? "",
      icon_color: p.icon_color,
      models: p.models,
    })),
  detect_env_conflicts: () => [],
  detect_provider_conflicts: () => [],
  get_tool_config_targets: () => [],
  detect_tool_installation: () => ({ installed: true, binary_found: true, config_dir_found: true }),
  list_tool_config_files: (args) => {
    const tool = String((args?.toolId as string) ?? "claude-code");
    const isCodex = tool === "codex";
    return [
      {
        file_id: "main",
        label: isCodex ? "config.toml" : "settings.json",
        path: isCodex ? "~/.codex/config.toml" : `~/.${tool}/settings.json`,
        format: isCodex ? "toml" : "json",
        exists: true,
        managed_by_skillstar: true,
      },
    ];
  },
  read_tool_config_file: () => '{\n  "// demo": "sample tool config (browser dev mock)"\n}',

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
    bypass: null,
  }),
  get_github_mirror_config: () => ({ enabled: false, preset_id: "ghproxy_vip", custom_url: null }),
  get_github_mirror_presets: () => [
    { id: "ghproxy_vip", name: "ghproxy.link", url: "https://ghproxy.link/", supports_clone: true },
    { id: "gh_proxy", name: "gh-proxy.com", url: "https://gh-proxy.com/", supports_clone: true },
  ],
  get_acp_config: () => ({ enabled: false, agent_command: "", agent_label: "" }),
  get_storage_overview: () => STORAGE_OVERVIEW,
  get_repo_cache_info: () => ({ total_bytes: 64_200_000, repo_count: 8, unused_count: 2, unused_bytes: 9_800_000 }),

  // ── GitHub ──
  check_gh_installed: () => true,
  check_gh_status: () => ({ status: "Ready", username: "dev-user" }),
  check_git_status: () => ({ status: "Installed", version: "2.45.0" }),
  list_repo_history: () => [],
  list_user_repos: () => [],

  // ── Usage mode (subscriptions / quota) ──
  list_usage_catalog: () => USAGE_CATALOG,
  list_subscriptions: () => USAGE_SUBSCRIPTIONS,
  get_active_subscriptions: () => ({
    cursor: "sub-cursor",
    codex: "sub-codex",
    deepseek: "sub-deepseek",
    glm: "sub-glm",
  }),
  get_subscription_alerts: () => USAGE_ALERTS,
  get_usage_summary: () => USAGE_SUMMARY,
  get_cookie_bridge_binding_status: (args) => ({
    provider: String((args?.provider as string) ?? ""),
    bound: false,
    subscription_id: null,
    updated_at: null,
  }),
  refresh_all_subscriptions: () => USAGE_SUBSCRIPTIONS.map((s) => s.usage).filter(Boolean),
  refresh_subscription_usage: (args) => USAGE_SUBSCRIPTIONS.find((s) => s.id === args?.id)?.usage ?? null,
  get_subscription_api_key: () => "sk-demo-********",
};

/**
 * Resolve a mocked command. Known commands return realistic sample data; unknown
 * commands resolve `undefined` (rather than rejecting) so unmocked reads degrade
 * to empty state and void mutations no-op, without flooding the console.
 */
export async function devInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Small delay so loading skeletons are exercised during UI iteration.
  await new Promise((r) => setTimeout(r, 90));
  const handler = HANDLERS[command];
  if (handler) {
    return handler(args ?? {}) as T;
  }
  // Unmocked commands resolve to an empty array — safe for the dominant
  // "list" read pattern (`.length` / `.map` / `for..of`) so unmocked screens
  // degrade to empty state instead of crashing. Object-returning commands that
  // need a richer shape are mocked explicitly above.
  if (import.meta.env.DEV) {
    // eslint-disable-next-line no-console
    console.debug(`[devMock] unmocked command "${command}" → []`, args ?? {});
  }
  return [] as unknown as T;
}
