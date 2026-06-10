import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpPreset } from "../../../types";

const PRESETS_STALE_TIME_MS = 60_000 * 60;
const QUERY_KEY = ["mcp-presets"] as const;

/**
 * Built-in / recommended MCP presets from the backend (single source of truth).
 *
 * Mirrors `useProviderPresets` — the registry lives in Rust
 * (`skillstar_models::mcp::get_mcp_presets`) and is exposed via the
 * `get_mcp_presets` command. The UI pre-fills the create form from a preset.
 */
export function useMcpPresets() {
  const { data, isLoading, error } = useQuery<McpPreset[]>({
    queryKey: QUERY_KEY,
    queryFn: () => tauriInvoke("get_mcp_presets"),
    staleTime: PRESETS_STALE_TIME_MS,
  });

  return { presets: data ?? [], isLoading, error: error ?? null };
}
