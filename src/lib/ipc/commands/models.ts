import type {
  AppId,
  ConnectionTestResult,
  FlatProvidersResponse,
  LatencyResult,
  ModelCatalogFetchResult,
  ProviderEntryFlat,
  ProviderPatchFlat,
  ProviderUpdateFlatResult,
  EndpointLatencyResult,
  ProviderPresetFlat,
  ToolBindingSettings,
  ToolConfigTarget,
  ToolConfigFileInfo,
  ToolSyncResult,
  WriteToolConfigFileResult,
} from "../../../types";
import type { AgentDescriptorDto } from "../../../types/generated/AgentDescriptorDto";

interface ConfigConflict {
  conflict_type: "EnvVarOverride" | "LegacyConfig" | "ExternalModification";
  description: string;
  file_path?: string | null;
  details?: string | null;
  tool_id?: string | null;
}

interface ToolInstallStatus {
  installed: boolean;
  binary_found: boolean;
  config_dir_found: boolean;
}

interface BalanceRawResponse {
  [key: string]: unknown;
}

/** Models mode: providers, presets, tool activations, and latency/balance checks. */
export interface ModelsCommands {
  // Flat provider store (v2)
  get_providers_flat: { args: Record<string, never>; result: FlatProvidersResponse };
  create_provider_flat: { args: { entry: Partial<ProviderEntryFlat> }; result: ProviderEntryFlat };
  update_provider_flat: {
    args: { id: string; patch: ProviderPatchFlat };
    result: ProviderUpdateFlatResult;
  };
  set_app_ai_provider_ref: { args: { appId: string; providerId: string }; result: void };
  clear_app_ai_provider_ref: { args: Record<string, never>; result: void };

  list_tool_config_files: { args: { toolId: string }; result: ToolConfigFileInfo[] };
  read_tool_config_file: { args: { toolId: string; fileId: string }; result: string };
  write_tool_config_file: {
    args: { toolId: string; fileId: string; content: string };
    result: WriteToolConfigFileResult;
  };
  format_tool_config_file: { args: { toolId: string; fileId: string }; result: string };
  push_provider_to_tool_config: {
    args: { providerId: string; toolId: string };
    result: ToolSyncResult;
  };
  delete_provider_flat: { args: { id: string }; result: void };
  reorder_providers: { args: { orderedIds: string[] }; result: void };

  // Agent bindings.
  //
  // v3's `activate_tool` did three jobs (add, point, edit) and its
  // `deactivate_tool` cleared an agent's whole list even when the caller meant
  // to drop one row. Each of those is now its own command.

  /** Add a provider to an agent and make it active. */
  bind_provider: {
    args: { providerId: string; toolId: string; model?: string | null; settings?: Record<string, unknown> | null };
    result: ToolSyncResult;
  };
  /** Drop **one** provider from an agent. */
  unbind_provider: {
    args: { toolId: string; providerId: string };
    result: ToolSyncResult;
  };
  /** Clear an agent's binding entirely. The destructive one. */
  unbind_agent: { args: { toolId: string }; result: void };
  /** Move the active pointer to an already-bound provider. */
  set_active_binding: {
    args: { toolId: string; providerId: string };
    result: ToolSyncResult;
  };
  /** Edit one bound entry without moving the pointer. */
  update_binding_entry: {
    args: { toolId: string; providerId: string; model?: string | null; settings?: Record<string, unknown> | null };
    result: ToolSyncResult;
  };
  /** Per-entry settings: how this provider behaves under this agent. */
  update_binding_entry_settings: {
    args: { toolId: string; settings: Record<string, unknown> };
    result: ToolSyncResult;
  };
  /**
   * Agent-level settings, including role → model routing.
   *
   * Typed as the bag itself rather than a loose record — a caller holding a
   * well-formed settings object should reach the command without a cast.
   */
  update_agent_settings: {
    args: { toolId: string; settings: ToolBindingSettings };
    result: ToolSyncResult;
  };

  // Tool config targets
  get_tool_config_targets: { args: { app_id: AppId }; result: ToolConfigTarget[] };
  /**
   * The agent registry itself. Compiled into the binary, so the renderer can
   * cache it forever — and, more to the point, stop keeping a second copy of
   * which roles each agent has and what its config file calls them.
   */
  list_agent_descriptors: { args: Record<string, never>; result: AgentDescriptorDto[] };

  // Presets and discovery
  get_provider_presets_flat: { args: Record<string, never>; result: ProviderPresetFlat[] };
  // Diagnostics take `providerId`, never a key: the backend resolves the
  // credential from the store it already owns, so the plaintext key never has
  // to live in the renderer or cross IPC. `urls` survives on the speed test
  // because that probe is explicitly about comparing candidate URLs the row
  // does not point at yet.
  test_endpoints_latency: {
    args: { urls: string[]; providerId?: string | null; timeoutMs?: number };
    result: EndpointLatencyResult[];
  };
  fetch_provider_models: {
    args: { providerId: string; timeoutMs?: number };
    result: string[];
  };
  fetch_provider_model_catalog: {
    args: { providerId: string; timeoutMs?: number };
    result: ModelCatalogFetchResult;
  };

  // Tests
  test_provider_connection: {
    args: { providerId: string; model: string; format: "openai" | "anthropic" };
    result: ConnectionTestResult;
  };
  test_provider_latency: {
    args: { app_id: AppId | string; provider_id: string; timeout_ms?: number };
    result: LatencyResult;
  };
  query_provider_balance: {
    args: { providerId: string };
    result: BalanceRawResponse;
  };

  // Environment / conflict detection
  detect_provider_conflicts: { args: { providerId: string }; result: ConfigConflict[] };
  resync_tool: { args: { toolId: string }; result: ToolSyncResult };
  detect_tool_installation: { args: { toolId: string }; result: ToolInstallStatus };
}

export type { ConfigConflict, ToolInstallStatus };
