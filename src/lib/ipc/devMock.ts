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
  REMOTE_SKILLS_SAMPLE,
  CLOUD_MANIFEST_SAMPLE,
  S3_TARGETS,
  SAMPLE_SKILLS,
  SSH_HOSTS,
  STORAGE_OVERVIEW,
  SYSTEM_SSH_HOSTS,
  USAGE_ALERTS,
  USAGE_CATALOG,
  USAGE_SUBSCRIPTIONS,
  USAGE_SUMMARY,
} from "./devMockData";

let devProviderSeq = 0;

// S3 sync targets are held in memory so browser-dev add/edit/delete persists
// across queries within a session (mirrors sshHostsStore above). Seeded from
// S3_TARGETS once on first use.
let s3TargetsStore: Record<string, unknown>[] | null = null;
function s3Targets(): Record<string, unknown>[] {
  if (s3TargetsStore === null) {
    s3TargetsStore = S3_TARGETS.map((t) => ({ ...t }));
  }
  return s3TargetsStore;
}

// SSH hosts are held in memory so browser-dev add/edit/delete persists across
// queries within a session (mirrors the real Tauri TOML store). Seeded from
// SSH_HOSTS once on first use.
let sshHostsStore: Record<string, unknown>[] | null = null;
function sshHosts(): Record<string, unknown>[] {
  if (sshHostsStore === null) {
    sshHostsStore = SSH_HOSTS.map((h) => ({ ...h }));
  }
  return sshHostsStore;
}

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
  get_skill_deploy_status: (args) => {
    const name = String((args?.skillName as string) ?? "pdf-tools");
    // Mixed kinds so the degraded-deploy badges are visible in browser dev:
    // healthy link (no badge), copy fallback, and a dangling link.
    return [
      {
        agent_id: "claude",
        agent_name: "Claude Code",
        target_path: `/Users/dev/claude/skills/${name}`,
        kind: "link",
        link_alive: true,
      },
      {
        agent_id: "codex",
        agent_name: "Codex CLI",
        target_path: `/Users/dev/codex/skills/${name}`,
        kind: "copy",
        link_alive: true,
      },
      {
        agent_id: "opencode",
        agent_name: "OpenCode",
        target_path: `/Users/dev/opencode/skills/${name}`,
        kind: "link",
        link_alive: false,
      },
    ];
  },
  list_skill_groups: () => DECKS,
  list_projects: () => PROJECTS,
  get_project_skills: () => ({ agents: { claude: ["pdf-tools", "xlsx"] }, updated_at: iso(1) }),
  // Disk scan of a project's agent skill folders. Returns an empty-but-well-
  // typed result so the Projects selection flow (buildSymlinkSkillIndex etc.)
  // runs end-to-end in browser dev instead of crashing on `undefined.skills`.
  scan_project_skills: () => ({ skills: [], agents_found: [] }),
  rebuild_project_skills_from_disk: () => ({ agents: { claude: ["pdf-tools", "xlsx"] }, updated_at: iso(1) }),
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
  // The flat store is intentionally STATEFUL in dev: write commands mutate
  // FLAT_PROVIDERS in place so the full create → edit → activate → delete flow
  // can be exercised in the browser without the Tauri backend.
  get_providers_flat: () => FLAT_PROVIDERS,
  get_tool_activations: () => FLAT_PROVIDERS.tool_activations,
  create_provider_flat: (args) => {
    const entry = (args?.entry ?? {}) as Record<string, unknown>;
    const created = {
      ...entry,
      id: `p-dev-${++devProviderSeq}`,
      sort_index: FLAT_PROVIDERS.providers.length,
      created_at: Date.now(),
    };
    FLAT_PROVIDERS.providers.push(created as never);
    return created;
  },
  update_provider_flat: (args) => {
    const id = String(args?.id ?? "");
    const patch = (args?.patch ?? {}) as Record<string, unknown>;
    const index = FLAT_PROVIDERS.providers.findIndex((p) => p.id === id);
    if (index >= 0) {
      FLAT_PROVIDERS.providers[index] = { ...FLAT_PROVIDERS.providers[index], ...patch } as never;
    }
    return { provider: FLAT_PROVIDERS.providers[index] ?? null, tool_sync_results: [] };
  },
  delete_provider_flat: (args) => {
    const id = String(args?.id ?? "");
    FLAT_PROVIDERS.providers = FLAT_PROVIDERS.providers.filter((p) => p.id !== id);
    for (const [toolId, activation] of Object.entries(FLAT_PROVIDERS.tool_activations)) {
      if ((activation as { provider_id?: string } | null)?.provider_id === id) {
        (FLAT_PROVIDERS.tool_activations as Record<string, unknown>)[toolId] = null;
      }
    }
    return undefined;
  },
  reorder_providers: (args) => {
    const orderedIds = (args?.orderedIds ?? []) as string[];
    FLAT_PROVIDERS.providers = FLAT_PROVIDERS.providers
      .map((p) => ({ ...p, sort_index: orderedIds.indexOf(p.id) === -1 ? p.sort_index : orderedIds.indexOf(p.id) }))
      .sort((a, b) => a.sort_index - b.sort_index) as never;
    return undefined;
  },
  activate_tool: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const provider = FLAT_PROVIDERS.providers.find((p) => p.id === providerId);
    (FLAT_PROVIDERS.tool_activations as Record<string, unknown>)[toolId] = {
      provider_id: providerId,
      model: (args?.model as string) || provider?.default_model || "",
      settings: args?.settings ?? null,
      last_sync_at: Math.floor(Date.now() / 1000),
    };
    return { tool_id: toolId, success: true, config_path: `~/.${toolId}/settings.json` };
  },
  deactivate_tool: (args) => {
    (FLAT_PROVIDERS.tool_activations as Record<string, unknown>)[String(args?.toolId ?? "")] = null;
    return undefined;
  },
  update_tool_settings: (args) => {
    const toolId = String(args?.toolId ?? "");
    const existing = (FLAT_PROVIDERS.tool_activations as Record<string, { settings?: unknown } | null>)[toolId];
    if (existing) existing.settings = args?.settings ?? null;
    return { tool_id: toolId, success: true };
  },
  push_provider_to_tool_config: (args) => ({ tool_id: String(args?.toolId ?? ""), success: true }),
  set_app_ai_provider_ref: () => undefined,
  clear_app_ai_provider_ref: () => undefined,
  test_provider_connection: () => ({ status: "ok", latency_ms: 180 + Math.floor(Math.random() * 240) }),
  test_endpoints_latency: (args) =>
    ((args?.urls ?? []) as string[]).map((url, i) => ({ url, latency_ms: 160 + i * 70, status: 200, error: null })),
  query_provider_balance: () => ({ balance: "12.50", currency: "USD" }),
  fetch_provider_model_catalog: () => ({
    models: ["dev-model-pro", "dev-model-mini"],
    catalog: [
      { id: "dev-model-pro", display_name: "Dev Model Pro", context_length: 200000, max_completion_tokens: 8192 },
      { id: "dev-model-mini", display_name: "Dev Model Mini", context_length: 128000, max_completion_tokens: 4096 },
    ],
    metadata_sources: ["mock"],
    missing_cost_count: 2,
  }),
  fetch_provider_models: () => ["dev-model-pro", "dev-model-mini"],
  write_tool_config_file: () => ({ success: true }),
  format_tool_config_file: () => '{\n  "// demo": "formatted sample (browser dev mock)"\n}',
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
  refresh_all_subscriptions: () => USAGE_SUBSCRIPTIONS.map((s) => s.usage).filter(Boolean),
  refresh_subscription_usage: (args) => USAGE_SUBSCRIPTIONS.find((s) => s.id === args?.id)?.usage ?? null,
  get_subscription_api_key: () => "sk-demo-********",

  // ── SSH remote hosts (in-memory persisted for the dev session) ──
  list_ssh_hosts: () => {
    const managed = sshHosts().map((h) => ({ ...h, source: "managed" }));
    // De-dup system hosts already present in the managed store (by host).
    const managedHosts = new Set(sshHosts().map((h) => String(h.host)));
    const system = SYSTEM_SSH_HOSTS.filter((s) => !managedHosts.has(s.host)).map((s) => ({
      ...s,
      source: "system",
    }));
    return [...managed, ...system];
  },
  add_ssh_host: (args) => {
    const def = (args?.def ?? {}) as Record<string, unknown>;
    const created = { ...def, id: def.id ? String(def.id) : `ssh_${Date.now()}` };
    sshHosts().push(created);
    return created;
  },
  update_ssh_host: (args) => {
    const { id, def } = (args ?? {}) as { id?: string; def?: Record<string, unknown> };
    const idx = sshHosts().findIndex((h) => h.id === id);
    if (idx >= 0 && def) sshHosts()[idx] = { ...def, id };
    return undefined;
  },
  delete_ssh_host: (args) => {
    const { id } = (args ?? {}) as { id?: string };
    const store = sshHosts();
    const idx = store.findIndex((h) => h.id === id);
    if (idx >= 0) store.splice(idx, 1);
    return undefined;
  },
  import_system_host: (args) => {
    const { alias } = (args ?? {}) as { alias?: string };
    const sys = SYSTEM_SSH_HOSTS.find((s) => s.alias === alias);
    if (!sys) throw new Error(`system host '${alias}' not found`);
    const created = {
      id: `ssh_${Date.now()}`,
      display_name: sys.alias,
      host: sys.host,
      port: sys.port,
      username: sys.username,
      auth_method: sys.identity_file ? { kind: "key", key_path: sys.identity_file } : { kind: "password" },
      default_remote_dir: "",
    };
    sshHosts().push(created);
    return created;
  },
  test_ssh_connection: () => ({
    result: { latency_ms: 42, remote_user: "ubuntu", system: "Linux 6.5 x86_64" },
    host_key_state: "verified",
  }),
  accept_ssh_host_key: () => undefined,
  discover_remote_skills: () => ({
    agents: [
      { agent: "claude", path: "/root/.claude/skills", count: 2 },
      { agent: "codex", path: "/root/.codex/skills", count: 1 },
      { agent: "grok", path: "/root/.grok/skills", count: 1 },
    ],
    skills: [
      {
        name: "code-review",
        path: "/root/.claude/skills/code-review",
        agent: "claude",
        size: 8192,
        layout: "hub_managed",
      },
      {
        name: "brandkit",
        path: "/root/.claude/skills/brandkit",
        agent: "claude",
        size: 6144,
        layout: "standalone",
      },
      {
        name: "imagine",
        path: "/root/.codex/skills/imagine",
        agent: "codex",
        size: 4096,
        layout: "standalone",
      },
      {
        name: "find-skills",
        path: "/root/.grok/skills/find-skills",
        agent: "grok",
        size: 3072,
        layout: "standalone",
      },
    ],
    needs_migration_count: 3,
  }),
  migrate_remote_skill_to_hub: () => ({
    remote_path: "/root/.grok/skills/imagine",
    hub_content_path: "~/.skillstar/hub/content/imagine",
  }),
  list_remote_skills: () => REMOTE_SKILLS_SAMPLE,
  push_skill_to_remote: (args) => ({
    files_uploaded: 3,
    bytes: 8192,
    remote_path: `~/.claude/skills/${args?.skillName ?? "skill"}`,
  }),
  delete_remote_skill: () => undefined,
  push_skills_to_remote: (args) => {
    const names: string[] = args?.skillNames ?? [];
    const pushed = names.map((skillName) => ({
      files_uploaded: 3,
      bytes: 8192,
      remote_path: `~/.claude/skills/${skillName}`,
    }));
    return {
      pushed,
      failed: [],
      total: names.length,
      succeeded: names.length,
    };
  },
  read_remote_skill_content: (args) => ({
    name: args?.skillName ?? "skill",
    content: "---\nname: skill\ndescription: Mocked remote SKILL.md\n---\n\n# Mocked remote skill body.\n",
    modified: "2025-01-01",
  }),
  write_remote_skill_content: () => undefined,
  pull_remote_skill: () => undefined,
  toggle_remote_agent_link: () => undefined,
  install_remote_skill: () => undefined,
  check_remote_skill_updates: () => [
    { name: "code-review", update_available: false },
    { name: "brandkit", update_available: true },
  ],

  // ── S3 cloud sync ──
  list_s3_targets: () => s3Targets().map((t) => ({ ...t })),
  add_s3_target: (args) => {
    const def = (args?.def ?? {}) as Record<string, unknown>;
    const created = { ...def, id: def.id ? String(def.id) : `s3_${Date.now()}` };
    s3Targets().push(created);
    return created;
  },
  update_s3_target: (args) => {
    const { id, def } = (args ?? {}) as { id?: string; def?: Record<string, unknown> };
    const idx = s3Targets().findIndex((t) => t.id === id);
    if (idx >= 0 && def) s3Targets()[idx] = { ...def, id };
    return undefined;
  },
  delete_s3_target: (args) => {
    const { id } = (args ?? {}) as { id?: string };
    const store = s3Targets();
    const idx = store.findIndex((t) => t.id === id);
    if (idx >= 0) store.splice(idx, 1);
    return undefined;
  },
  test_s3_connection: () => ({ latency_ms: 38 }),
  push_skills_to_cloud: () => ({
    hubCount: 2,
    localCount: 1,
    tarballsUploaded: 1,
    tarballsSkipped: 0,
    manifestUploaded: true,
  }),
  pull_cloud_manifest: () => CLOUD_MANIFEST_SAMPLE.map((e) => ({ ...e })),
  install_from_cloud_manifest: (args) => {
    const entries = (args?.entries ?? []) as { name?: string }[];
    const names = entries.map((e) => String(e.name ?? "")).filter(Boolean);
    return {
      requested_count: names.length,
      installed_names: names,
      existing_names: [] as string[],
      restored_names: [] as string[],
      skipped_names: [] as string[],
      outcomes: names.map((name) => ({ status: "installed" as const, name })),
    };
  },
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
