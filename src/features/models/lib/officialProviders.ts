/**
 * Claude / Codex Official (原生登录) helpers for the D1 matrix.
 *
 * Official is NOT a cross-referenced Provider row. Seeds stay in the store
 * (`ensure_official_providers`) but the matrix hides them; column headers
 * switch the matching Agent back to Official.
 */

import type { ProviderEntryFlat, ToolActivationsMap } from "../../../types";
import type { ProviderToolId } from "./agentRegistry";
import { activeEntry } from "./toolBinding";

export const CLAUDE_OFFICIAL_ID = "claude-official";
export const CODEX_OFFICIAL_ID = "codex-official";

export const OFFICIAL_PROVIDER_IDS = [CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID] as const;

export type OfficialProviderId = (typeof OFFICIAL_PROVIDER_IDS)[number];

const OFFICIAL_META: Record<
  OfficialProviderId,
  { name: string; iconColor: string; bindToolId: ProviderToolId; defaultModel: string }
> = {
  [CLAUDE_OFFICIAL_ID]: {
    name: "Claude Official",
    iconColor: "#D97757",
    bindToolId: "claude-code",
    defaultModel: "",
  },
  [CODEX_OFFICIAL_ID]: {
    name: "Codex Official",
    iconColor: "#10A37F",
    bindToolId: "codex",
    defaultModel: "",
  },
};

/** Tools that can switch to Native Official via the column header. */
export function toolSupportsOfficial(toolId: string): toolId is "claude-code" | "claude-desktop" | "codex" {
  return toolId === "claude-code" || toolId === "claude-desktop" || toolId === "codex";
}

export function officialProviderIdForTool(toolId: string): OfficialProviderId | null {
  if (toolId === "claude-code" || toolId === "claude-desktop") return CLAUDE_OFFICIAL_ID;
  if (toolId === "codex") return CODEX_OFFICIAL_ID;
  return null;
}

export function isNativeOfficialProvider(provider: { id?: string | null; preset_id?: string | null }): boolean {
  const id = provider.id?.trim() ?? "";
  const preset = provider.preset_id?.trim() ?? "";
  return (
    OFFICIAL_PROVIDER_IDS.includes(id as OfficialProviderId) ||
    OFFICIAL_PROVIDER_IDS.includes(preset as OfficialProviderId)
  );
}

/** Which agent an Official seed binds; null if not Official. */
export function officialBindToolId(provider: { id?: string | null; preset_id?: string | null }): ProviderToolId | null {
  const key = (provider.preset_id?.trim() || provider.id?.trim() || "") as OfficialProviderId;
  return OFFICIAL_META[key]?.bindToolId ?? null;
}

/** Third-party (and non-native) rows shown in the Provider × Agent matrix. */
export function matrixProviders(providers: ProviderEntryFlat[]): ProviderEntryFlat[] {
  return providers.filter((p) => !isNativeOfficialProvider(p));
}

export function findOfficialProvider(
  providers: readonly ProviderEntryFlat[],
  toolId: string,
): ProviderEntryFlat | null {
  const id = officialProviderIdForTool(toolId);
  if (!id) return null;
  return (
    providers.find((p) => p.id === id || p.preset_id === id) ??
    // Fallback seed shape if store/ensure lag (should be rare after get_providers_flat).
    makeOfficialProviderSeed(id, -1)
  );
}

export function isToolOnOfficial(
  providers: readonly ProviderEntryFlat[],
  activations: ToolActivationsMap,
  toolId: string,
): boolean {
  const entry = activeEntry(activations[toolId]);
  if (!entry) return false;
  const provider = providers.find((p) => p.id === entry.provider_id);
  return Boolean(provider && isNativeOfficialProvider(provider));
}

export function makeOfficialProviderSeed(id: OfficialProviderId, sortIndex: number): ProviderEntryFlat {
  const meta = OFFICIAL_META[id];
  return {
    id,
    name: meta.name,
    base_url_openai: "",
    base_url_anthropic: "",
    models_url: "",
    api_key: "",
    models: [],
    default_model: meta.defaultModel,
    sort_index: sortIndex,
    preset_id: id,
    icon_color: meta.iconColor,
    // Persisted onto the provider record, so it must not depend on the active
    // locale — a translated value would be frozen into the user's store.
    notes: "Native login · Official",
    codex_auth_mode: id === CODEX_OFFICIAL_ID ? "oauth" : undefined,
  };
}

/**
 * Prefer store Official rows; inject missing seeds only as fallback.
 * Does not pin Official into the matrix — callers use `matrixProviders`.
 */
export function withEnsuredOfficialProviders(providers: ProviderEntryFlat[]): ProviderEntryFlat[] {
  const byId = new Map(providers.map((p) => [p.id, p]));
  const byPreset = new Map(providers.filter((p) => p.preset_id).map((p) => [p.preset_id!, p] as const));

  const injected: ProviderEntryFlat[] = [];
  for (const id of OFFICIAL_PROVIDER_IDS) {
    if (byId.has(id) || byPreset.has(id)) continue;
    injected.push(makeOfficialProviderSeed(id, -1 - injected.length));
  }

  return [...injected, ...providers];
}
