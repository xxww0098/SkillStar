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
}
/** Tools whose config natively holds several providers (mirrors agentRegistry). */
const MULTI_PROVIDER_TOOLS = new Set(["codex", "opencode"]);

export const MODELS_HANDLERS: DevMockHandlers = {
  get_providers_flat: () => FLAT_PROVIDERS,
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
  activate_tool: (args) => {
    const toolId = String(args?.toolId ?? "");
    const providerId = String(args?.providerId ?? "");
    const provider = FLAT_PROVIDERS.providers.find((p) => p.id === providerId);
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const entry = {
      provider_id: providerId,
      model: (args?.model as string) || provider?.default_model || "",
      settings: (args?.settings ?? null) as unknown,
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
  deactivate_tool: (args) => {
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding>;
    acts[String(args?.toolId ?? "")] = { entries: [], active_index: 0 };
    return undefined;
  },
  update_tool_settings: (args) => {
    const toolId = String(args?.toolId ?? "");
    const acts = FLAT_PROVIDERS.tool_activations as Record<string, MockBinding | null>;
    const binding = acts[toolId];
    const active = binding?.entries[Math.min(binding.active_index, binding.entries.length - 1)];
    if (active) active.settings = args?.settings ?? null;
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
  remove_binding_entry: (args) => {
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
