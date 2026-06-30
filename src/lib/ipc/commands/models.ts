import type {
  AppId,
  AppProviders,
  ConnectionTestResult,
  FlatProvidersResponse,
  LatencyResult,
  ModelCatalogFetchResult,
  ProviderEntry,
  ProviderEntryFlat,
  ProviderPatchFlat,
  ProviderUpdateFlatResult,
  EndpointLatencyResult,
  ProviderPreset,
  ProviderPresetFlat,
  SwitchResult,
  ToolActivationsMap,
  ToolConfigTarget,
  ToolConfigFileInfo,
  ToolSyncResult,
  WriteToolConfigFileResult,
} from "../../../types";

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
  // Per-app provider store (v1)
  get_app_providers: { args: { appId: AppId }; result: AppProviders };
  create_provider: {
    args: { appId: AppId; entry: Omit<ProviderEntry, "id" | "created_at"> };
    result: ProviderEntry;
  };
  update_provider: {
    args: { appId: AppId; id: string; patch: Partial<ProviderEntry> };
    result: ProviderEntry;
  };
  delete_provider: { args: { appId: AppId; id: string }; result: void };
  switch_active_provider: {
    args: { appId: AppId; providerId: string; syncTools: string[] };
    result: SwitchResult;
  };

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

  // Tool activations (v2)
  get_tool_activations: { args: Record<string, never>; result: ToolActivationsMap };
  activate_tool: {
    args: { providerId: string; toolId: string; model?: string | null; settings?: Record<string, unknown> | null };
    result: ToolSyncResult;
  };
  deactivate_tool: { args: { toolId: string }; result: void };
  update_tool_settings: {
    args: { toolId: string; settings: Record<string, unknown> };
    result: ToolSyncResult;
  };
  set_active_binding: {
    args: { toolId: string; providerId: string };
    result: ToolSyncResult;
  };
  remove_binding_entry: {
    args: { toolId: string; providerId: string };
    result: ToolSyncResult;
  };

  // Tool config targets (v1)
  get_tool_config_targets: { args: { app_id: AppId }; result: ToolConfigTarget[] };
  sync_provider_to_tool: {
    args: { app_id: AppId; provider_id: string; tool_id: string };
    result: ToolSyncResult;
  };
  sync_provider_to_all_tools: {
    args: { app_id: AppId; provider_id: string; tool_ids: string[] };
    result: ToolSyncResult[];
  };

  // Presets and discovery
  get_provider_presets: { args: Record<string, never>; result: ProviderPreset[] };
  get_provider_presets_flat: { args: Record<string, never>; result: ProviderPresetFlat[] };
  test_endpoints_latency: {
    args: { urls: string[]; apiKey?: string | null; timeoutMs?: number };
    result: EndpointLatencyResult[];
  };
  fetch_provider_models: {
    args: { url: string; apiKey: string; timeoutMs?: number };
    result: string[];
  };
  fetch_provider_model_catalog: {
    args: { url: string; apiKey: string; timeoutMs?: number };
    result: ModelCatalogFetchResult;
  };

  // Tests
  test_provider_connection: {
    args: { baseUrl: string; apiKey: string; model: string; format: "openai" | "anthropic" };
    result: ConnectionTestResult;
  };
  test_provider_latency: {
    args: {
      app_id: AppId | string;
      provider_id: string;
      base_url: string;
      api_key: string;
      timeout_ms?: number;
    };
    result: LatencyResult;
  };
  test_all_providers_latency: { args: { app_id: AppId }; result: LatencyResult[] };
  query_provider_balance: {
    args: { presetId: string; apiKey: string; baseUrl: string };
    result: BalanceRawResponse;
  };

  // Environment / conflict detection
  detect_env_conflicts: { args: Record<string, never>; result: ConfigConflict[] };
  detect_provider_conflicts: { args: { providerId: string }; result: ConfigConflict[] };
  resync_tool: { args: { toolId: string }; result: ToolSyncResult };
  detect_tool_installation: { args: { toolId: string }; result: ToolInstallStatus };
}

export type { ConfigConflict, ToolInstallStatus };
