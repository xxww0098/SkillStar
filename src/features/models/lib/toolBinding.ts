/**
 * Pure helpers for the `ToolBinding` shape (entries + active_index).
 *
 * The backend stores each agent's providers as a binding: an ordered entry
 * list plus a pointer to the active one. Single-provider agents keep 0–1
 * entries; multi-provider agents may keep several. These helpers centralise the
 * clamp/active/upsert logic so components never index `entries[active_index]`
 * raw (a stale pointer would panic-by-undefined).
 */
import type { ToolActivation, ToolBinding } from "../../../types";
import { agentSupportsMultipleProviders } from "./agentRegistry";

export const EMPTY_BINDING: ToolBinding = { entries: [], active_index: 0 };

/** The active entry, clamping a stale `active_index`. `null` when unbound. */
export function activeEntry(binding: ToolBinding | null | undefined): ToolActivation | null {
  if (!binding || binding.entries.length === 0) return null;
  const idx = Math.min(binding.active_index, binding.entries.length - 1);
  return binding.entries[idx] ?? null;
}

/** Whether any entry binds the given provider. */
export function bindsProvider(binding: ToolBinding | null | undefined, providerId: string): boolean {
  return Boolean(binding?.entries.some((e) => e.provider_id === providerId));
}

/**
 * Apply an activation the way the backend would, for optimistic cache updates:
 * multi-provider agents upsert the entry (update model if already bound, else
 * append) and point the active pointer at it; single-provider agents replace
 * the sole entry.
 */
export function upsertBindingEntry(
  prev: ToolBinding | null | undefined,
  toolId: string,
  entry: ToolActivation,
): ToolBinding {
  const base = prev ?? EMPTY_BINDING;
  if (!agentSupportsMultipleProviders(toolId)) {
    return { entries: [entry], active_index: 0 };
  }
  const pos = base.entries.findIndex((e) => e.provider_id === entry.provider_id);
  if (pos >= 0) {
    const entries = base.entries.slice();
    entries[pos] = { ...entries[pos], ...entry };
    return { entries, active_index: pos };
  }
  const entries = [...base.entries, entry];
  return { entries, active_index: entries.length - 1 };
}

/** Remove the entry for `providerId`, re-clamping the active pointer. */
export function removeBindingEntry(prev: ToolBinding | null | undefined, providerId: string): ToolBinding {
  const base = prev ?? EMPTY_BINDING;
  const pos = base.entries.findIndex((e) => e.provider_id === providerId);
  if (pos < 0) return base;
  const entries = base.entries.filter((_, i) => i !== pos);
  let active_index = base.active_index;
  if (active_index >= pos && active_index > 0) active_index -= 1;
  if (active_index >= entries.length) active_index = Math.max(0, entries.length - 1);
  return { entries, active_index };
}

/** Set the active pointer to `providerId` if bound (else unchanged). */
export function setActiveProvider(prev: ToolBinding | null | undefined, providerId: string): ToolBinding {
  const base = prev ?? EMPTY_BINDING;
  const pos = base.entries.findIndex((e) => e.provider_id === providerId);
  if (pos < 0) return base;
  return { ...base, active_index: pos };
}
