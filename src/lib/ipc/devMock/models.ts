/**
 * Dev-mock fragment: models — providers, tool activations/bindings, tool
 * config files, and connectivity probes. Sample data lives in
 * ./modelsData.ts.
 *
 * The flat store is intentionally STATEFUL in dev: write commands mutate
 * FLAT_PROVIDERS in place so the full create → edit → activate → delete flow
 * can be exercised in the browser without the Tauri backend.
 */

import { FLAT_PROVIDERS, PRESETS_FLAT } from "./modelsData";
import type { DevMockHandlers } from "./shared";

let devProviderSeq = 0;

const routingMemory = new Map<string, { routing: string; affinity: string }>();

const savedGroups: { id: string; members: string[] }[] = [{ id: "fast", members: ["openai/gpt-test"] }];

let listenMode = "loopback";

/** Newest-first fixture rows for the Gateway column (ledger-shaped). */
const RECENT_CALLS = [
  {
    at: "2026-09-30 12:00:02",
    agent: "codex",
    model: "openai/gpt-test",
    status: 200,
    completion_tokens: "5",
    in_tokens: "410",
    session: "sess-codex-1",
    latency: "842",
  },
  {
    at: "2026-09-30 12:00:00",
    agent: "pi",
    model: "group/fast",
    status: 502,
    completion_tokens: "",
    in_tokens: "",
    session: "sess-pi-2",
    latency: "1201",
  },
];

const modelLabels = new Map<string, string>();

const profiles: { name: string; agents: { id: string; modelRef: string }[] }[] = [
  { name: "work", agents: [{ id: "opencode", modelRef: "openai/gpt-test" }] },
  { name: "home", agents: [{ id: "pi", modelRef: "group/fast" }] },
];

function secretShaped(value: string): boolean {
  return value.includes("://") || value.includes("sk-") || value.includes("api.openai.com");
}

function remembered(owner: string, id: string): { routing: string; affinity: string } {
  return routingMemory.get(`${owner}:${id}`) ?? { routing: "smart", affinity: "auto" };
}

/** Minimal shape of a tool binding for the dev mock's in-memory state. */
interface MockEntry {
  provider_id: string;
  model: string;
  settings: unknown;
  last_sync_at?: number;
}
interface MockBinding {
  entries: MockEntry[];
  active_index: number;
  /** Binding-level bag (OMP model roles). */
  settings?: unknown;
}
/** Tools whose config natively holds several providers (mirrors agentRegistry). */
const MULTI_PROVIDER_TOOLS = new Set(["codex", "opencode", "pi", "omp"]);

const NATIVE_OFFICIAL_IDS = new Set(["claude-official", "codex-official"]);

/** Dev stand-in for `Credential::summary`. The board payload keeps this, not `api_key`. */
function maskedKey(apiKey: string): string {
  const chars = Array.from(apiKey);
  if (chars.length === 0) return "";
  if (chars.length <= 8) return "••••••••";
  return `${chars.slice(0, 4).join("")}••••${chars.slice(-4).join("")}`;
}

/** Mirror backend `ensure_official_providers` so the D1 hub can prefer store rows. */
function ensureOfficialInMockStore() {
  for (const id of ["claude-official", "codex-official"] as const) {
    const exists = FLAT_PROVIDERS.providers.some((p) => p.id === id || (p as { preset_id?: string }).preset_id === id);
    if (exists) continue;
    FLAT_PROVIDERS.providers.push({
      id,
      name: id === "claude-official" ? "Claude Official" : "Codex Official",
      base_url_openai: "",
      base_url_anthropic: "",
      models_url: "",
      api_key: "",
      models: [],
      default_model: "",
      sort_index: FLAT_PROVIDERS.providers.length,
      preset_id: id,
      icon_color: id === "claude-official" ? "#D97757" : "#10A37F",
      notes: "原生登录 · Official",
      ...(id === "codex-official" ? { codex_auth_mode: "oauth" } : {}),
    } as never);
  }
}

export const MODELS_HANDLERS: DevMockHandlers = {
  get_providers_flat: () => {
    ensureOfficialInMockStore();
    return FLAT_PROVIDERS;
  },
  get_model_choices: (args) => {
    const agentId = typeof args?.agentId === "string" ? args.agentId : "";
    const choices = [
      { id: "relay/m1", label: "relay/m1" },
      { id: "openai/gpt-test", label: "openai/gpt-test" },
      { id: "group/fast", label: "group/fast" },
    ].map((choice) => ({ ...choice, label: modelLabels.get(choice.id) ?? choice.label }));
    if (agentId === "opencode") {
      return choices.filter((choice) => choice.id !== "openai/gpt-test");
    }
    return choices;
  },
  save_model_name: (args) => {
    const id = typeof args?.id === "string" ? args.id.trim() : "";
    const name = typeof args?.name === "string" ? args.name.trim() : "";
    if (!id.includes("/") || id.startsWith("group/") || !name || name.includes("\n") || name.includes("\r")) {
      throw new Error("model_name");
    }
    if (name.includes("://") || name.includes("sk-") || Array.from(name).length > 80) throw new Error("model_name");
    modelLabels.set(id, name);
    return null;
  },
  get_recent_calls: () => [...RECENT_CALLS],
  get_ledger_page: (args) => {
    const agent = typeof args?.agent === "string" && args.agent ? args.agent : null;
    const session = typeof args?.session === "string" && args.session ? args.session : null;
    const offset = typeof args?.offset === "number" && args.offset > 0 ? args.offset : 0;
    const rows = RECENT_CALLS.filter((row) => (!agent || row.agent === agent) && (!session || row.session === session));
    return rows.slice(offset);
  },
  get_routing_page: (args) => {
    const providerId = typeof args?.providerId === "string" ? args.providerId : "";
    return {
      provider: providerId ? remembered("provider", providerId) : null,
      groups: savedGroups.map((group) => ({ id: group.id, ...remembered("group", group.id) })),
    };
  },
  model_efforts: (args) => {
    const id = typeof args?.id === "string" ? args.id : "";
    const bare = id.replace(/:(none|minimal|low|medium|high|xhigh|max)$/, "");
    if (!bare.includes("/") || bare.startsWith("group/")) return [];
    if (bare === "openai/gpt-test") return ["high"];
    return ["low", "high"];
  },
  get_saved_groups: () => savedGroups.map((group) => ({ id: group.id, members: [...group.members] })),
  save_group_members: (args) => {
    const raw = typeof args?.id === "string" ? args.id : "";
    const id = raw.startsWith("group/") ? raw.slice("group/".length) : raw;
    const members = Array.isArray(args?.members)
      ? args.members.filter((item): item is string => typeof item === "string")
      : [];
    if (members.some((member) => member === id || member === `group/${id}`)) {
      throw new Error("group_cycle");
    }
    const existing = savedGroups.find((group) => group.id === id);
    if (existing) existing.members = members;
    else if (id) savedGroups.push({ id, members });
    return null;
  },
  get_profile_names: () => profiles.map((profile) => profile.name),
  save_profile: (args) => {
    const name = typeof args?.name === "string" ? args.name.trim() : "";
    const agents = Array.isArray(args?.agents) ? args.agents : [];
    const cleaned = agents
      .map((agent) => {
        const row = agent as { id?: unknown; modelRef?: unknown };
        return {
          id: typeof row.id === "string" ? row.id.trim() : "",
          modelRef: typeof row.modelRef === "string" ? row.modelRef.trim() : "",
        };
      })
      .filter((agent) => agent.id && agent.modelRef);
    const seen = new Set<string>();
    const unique = cleaned.filter((agent) => {
      if (seen.has(agent.id)) return false;
      seen.add(agent.id);
      return true;
    });
    if (!name || Array.from(name).length > 64) throw new Error("profile_name");
    if (secretShaped(name) || unique.some((agent) => secretShaped(agent.id) || secretShaped(agent.modelRef))) {
      throw new Error("profile_store");
    }
    if (unique.length === 0) throw new Error("profile_store");
    const existing = profiles.find((profile) => profile.name === name);
    if (existing) existing.agents = unique;
    else profiles.push({ name, agents: unique });
    return null;
  },
  apply_profile: (args) => {
    const name = typeof args?.name === "string" ? args.name.trim() : "";
    if (!profiles.some((profile) => profile.name === name)) throw new Error("profile_missing");
    return { applied: [], skipped: [] };
  },
  get_listen_mode: () => listenMode,
  get_loopback_origin: () => "http://127.0.0.1:21847",
  save_listen_mode: (args) => {
    const mode = typeof args?.mode === "string" ? args.mode : "";
    if (mode !== "loopback" && mode !== "lan") throw new Error("listen_store");
    listenMode = mode;
    return null;
  },
  save_routing: (args) => {
    const owner = typeof args?.owner === "string" ? args.owner : "";
    const id = typeof args?.id === "string" ? args.id : "";
    const routing = typeof args?.routing === "string" ? args.routing : "";
    const affinity = typeof args?.affinity === "string" ? args.affinity : "";
    routingMemory.set(`${owner}:${id}`, { routing, affinity });
    return null;
  },
  save_agent_model: () => null,
  get_models_board: () => ({
    agents: [
      { id: "claude-code", name: "Claude Code", loopback_label: "", model_label: "" },
      { id: "claude-desktop", name: "Claude Desktop", loopback_label: "", model_label: "" },
      {
        id: "codex",
        name: "Codex",
        loopback_label: "127.0.0.1:21847",
        model_label: "group/fast",
      },
      {
        id: "opencode",
        name: "OpenCode",
        loopback_label: "",
        model_label: "deepseek/deepseek-chat",
      },
      { id: "pi", name: "Pi", loopback_label: "", model_label: "" },
      { id: "omp", name: "Oh My Pi", loopback_label: "", model_label: "" },
    ],
    providers: FLAT_PROVIDERS.providers.map((provider) => ({
      id: provider.id,
      name: provider.name,
      credential_summary: maskedKey(provider.api_key),
      loopback_label: "",
      model_label: "",
    })),
    gateway: [],
  }),
  create_provider_flat: (args) => {
    const entry = (args?.entry ?? {}) as Record<string, unknown>;
    const requestedId = typeof entry.id === "string" ? entry.id : "";
    const keepOfficialId = NATIVE_OFFICIAL_IDS.has(requestedId);
    const created = {
      ...entry,
      id: keepOfficialId ? requestedId : `p-dev-${++devProviderSeq}`,
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
      FLAT_PROVIDERS.providers[index] = {
        ...FLAT_PROVIDERS.providers[index],
        ...patch,
      } as never;
    }
    return {
      provider: FLAT_PROVIDERS.providers[index] ?? null,
      tool_sync_results: [],
    };
  },
  delete_provider_flat: (args) => {
    const id = String(args?.id ?? "");
    FLAT_PROVIDERS.providers = FLAT_PROVIDERS.providers.filter((p) => p.id !== id);
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    for (const [toolId, binding] of Object.entries(acts)) {
      if (!binding) continue;
      const pos = binding.entries.findIndex((e) => e.provider_id === id);
      if (pos >= 0) {
        binding.entries.splice(pos, 1);
        if (binding.active_index >= pos && binding.active_index > 0) binding.active_index -= 1;
        acts[toolId] = binding;
      }
    }
    return undefined;
  },
  reorder_providers: (args) => {
    const orderedIds = (args?.orderedIds ?? []) as string[];
    FLAT_PROVIDERS.providers = FLAT_PROVIDERS.providers
      .map((p) => ({
        ...p,
        sort_index: orderedIds.indexOf(p.id) === -1 ? p.sort_index : orderedIds.indexOf(p.id),
      }))
      .sort((a, b) => a.sort_index - b.sort_index) as never;
    return undefined;
  },
  bind_provider: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const provider = FLAT_PROVIDERS.providers.find((p) => p.id === providerId);
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const presetId = (provider as { preset_id?: string } | undefined)?.preset_id;
    const isCodexOfficial = providerId === "codex-official" || presetId === "codex-official";
    let settings = (args?.settings ?? null) as unknown;
    if (isCodexOfficial) {
      const prev = settings && typeof settings === "object" ? (settings as Record<string, unknown>) : {};
      settings = { ...prev, auth_mode: "oauth" };
    }
    const entry = {
      provider_id: providerId,
      model: (args?.model as string) || provider?.default_model || "",
      settings,
      last_sync_at: Math.floor(Date.now() / 1000),
    };
    const prev = acts[toolId];
    if (MULTI_PROVIDER_TOOLS.has(toolId) && prev) {
      const pos = prev.entries.findIndex((e) => e.provider_id === providerId);
      if (pos >= 0) {
        prev.entries[pos] = entry;
        prev.active_index = pos;
      } else {
        prev.entries.push(entry);
        prev.active_index = prev.entries.length - 1;
      }
      acts[toolId] = prev;
    } else {
      acts[toolId] = { entries: [entry], active_index: 0 };
    }
    return {
      tool_id: toolId,
      success: true,
      config_path: `~/.${toolId}/settings.json`,
    };
  },
  unbind_agent: (args) => {
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding>;
    acts[String(args?.toolId ?? "")] = { entries: [], active_index: 0 };
    return undefined;
  },
  update_binding_entry_settings: (args) => {
    const toolId = String(args?.toolId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const binding = acts[toolId];
    const active = binding?.entries[Math.min(binding.active_index, binding.entries.length - 1)];
    if (active) active.settings = args?.settings ?? null;
    return { tool_id: toolId, success: true };
  },
  update_agent_settings: (args) => {
    const toolId = String(args?.toolId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const binding = acts[toolId];
    // Binding-level, not per-entry: an OMP role may target a provider other
    // than the active one, so it hangs off the binding itself.
    if (binding) binding.settings = args?.settings ?? null;
    return { tool_id: toolId, success: true };
  },
  update_binding_entry: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const entry = acts[toolId]?.entries.find((e) => e.provider_id === providerId);
    if (entry) {
      // `undefined` means "leave it alone" — the point of splitting this out
      // of bind_provider is that changing the model does not require the
      // caller to hand the settings back unchanged.
      if (args?.model != null) entry.model = String(args.model);
      if (args?.settings !== undefined) entry.settings = args.settings ?? null;
    }
    return { tool_id: toolId, success: true };
  },
  set_active_binding: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const binding = acts[toolId];
    if (binding) {
      const pos = binding.entries.findIndex((e) => e.provider_id === providerId);
      if (pos >= 0) binding.active_index = pos;
    }
    return { tool_id: toolId, success: true };
  },
  unbind_provider: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const binding = acts[toolId];
    if (binding) {
      const pos = binding.entries.findIndex((e) => e.provider_id === providerId);
      if (pos >= 0) {
        binding.entries.splice(pos, 1);
        if (binding.active_index >= pos && binding.active_index > 0) binding.active_index -= 1;
      }
    }
    return { tool_id: toolId, success: true };
  },
  push_provider_to_tool_config: (args) => ({
    tool_id: String(args?.toolId ?? ""),
    success: true,
  }),
  set_app_ai_provider_ref: () => undefined,
  clear_app_ai_provider_ref: () => undefined,
  test_provider_connection: () => ({
    status: "ok",
    latency_ms: 180 + Math.floor(Math.random() * 240),
  }),
  test_endpoints_latency: (args) =>
    ((args?.urls ?? []) as string[]).map((url, i) => ({
      url,
      latency_ms: 160 + i * 70,
      status: 200,
      error: null,
    })),
  query_provider_balance: () => ({ balance: "12.50", currency: "USD" }),
  fetch_provider_model_catalog: () => ({
    models: ["dev-model-pro", "dev-model-mini"],
    catalog: [
      {
        id: "dev-model-pro",
        display_name: "Dev Model Pro",
        context_length: 200000,
        max_completion_tokens: 8192,
      },
      {
        id: "dev-model-mini",
        display_name: "Dev Model Mini",
        context_length: 128000,
        max_completion_tokens: 4096,
      },
    ],
    metadata_sources: ["mock"],
    missing_cost_count: 2,
  }),
  fetch_provider_models: () => ["dev-model-pro", "dev-model-mini"],
  write_tool_config_file: () => ({ success: true }),
  format_tool_config_file: () => '{\n  "// demo": "formatted sample (browser dev mock)"\n}',
  get_provider_presets_flat: () => PRESETS_FLAT,
  detect_provider_conflicts: () => [],
  get_tool_config_targets: () => [],
  // Mirrors `tool_sync::agent_specs()` closely enough for the browser dev shell
  // to render the three role tiers: none, tier aliases, full map.
  list_agent_descriptors: () => [
    {
      id: "claude-code",
      display_name: "Claude Code",
      kind: "single",
      required_wire: "anthropic_messages",
      roles: [
        { id: "default", agent_key: "ANTHROPIC_MODEL", primary: true, inherits: null, requires: "any" },
        { id: "fast", agent_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL", primary: true, inherits: null, requires: "any" },
        { id: "sonnet", agent_key: "ANTHROPIC_DEFAULT_SONNET_MODEL", primary: false, inherits: null, requires: "any" },
        { id: "opus", agent_key: "ANTHROPIC_DEFAULT_OPUS_MODEL", primary: false, inherits: null, requires: "any" },
        {
          id: "subagent",
          agent_key: "CLAUDE_CODE_SUBAGENT_MODEL",
          primary: false,
          inherits: "default",
          requires: "any",
        },
      ],
      config_files: [{ file_id: "settings", label: "settings.json", format: "json" }],
    },
    {
      id: "omp",
      display_name: "Oh My Pi",
      kind: "multi",
      required_wire: "openai_chat",
      roles: [
        { id: "default", agent_key: "default", primary: true, inherits: null, requires: "any" },
        { id: "fast", agent_key: "smol", primary: true, inherits: "default", requires: "any" },
        { id: "slow", agent_key: "slow", primary: true, inherits: "default", requires: "any" },
        { id: "plan", agent_key: "plan", primary: true, inherits: null, requires: "any" },
        { id: "vision", agent_key: "vision", primary: false, inherits: null, requires: "vision" },
        { id: "designer", agent_key: "designer", primary: false, inherits: "default", requires: "any" },
        { id: "commit", agent_key: "commit", primary: false, inherits: null, requires: "any" },
        { id: "tiny", agent_key: "tiny", primary: false, inherits: null, requires: "any" },
        { id: "subagent", agent_key: "task", primary: false, inherits: null, requires: "any" },
        { id: "advisor", agent_key: "advisor", primary: false, inherits: null, requires: "any" },
      ],
      config_files: [
        { file_id: "models", label: "models.yml", format: "yaml" },
        { file_id: "config", label: "config.yml", format: "yaml" },
      ],
    },
    {
      id: "pi",
      display_name: "Pi",
      kind: "multi",
      required_wire: "openai_chat",
      roles: [],
      config_files: [
        { file_id: "models", label: "models.json", format: "json" },
        { file_id: "settings", label: "settings.json", format: "json" },
      ],
    },
  ],
  detect_tool_installation: () => ({
    installed: true,
    binary_found: true,
    config_dir_found: true,
  }),
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
};
