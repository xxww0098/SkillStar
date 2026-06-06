/**
 * Typed Tauri IPC for fingerprint commands (Phase 4 backend).
 *
 * Matches the 7 commands registered in `src-tauri/src/lib.rs`:
 * - list_fingerprints / get_fingerprint
 * - list_fingerprint_presets / create_fingerprint_from_preset
 * - update_fingerprint / delete_fingerprint / set_active_fingerprint
 */
import { invoke } from "@tauri-apps/api/core";
import type { FingerprintListDto, FingerprintRow, PresetTemplate, SupportedIde, UpdateFingerprintInput } from "./types";

export const fingerprintsApi = {
  list: () => invoke<FingerprintListDto>("list_fingerprints"),
  get: (id: string) => invoke<FingerprintRow>("get_fingerprint", { id }),
  listPresets: () => invoke<PresetTemplate[]>("list_fingerprint_presets"),
  createFromPreset: (presetId: string, name: string) =>
    invoke<FingerprintRow>("create_fingerprint_from_preset", { presetId, name }),
  update: (id: string, input: UpdateFingerprintInput) => invoke<FingerprintRow>("update_fingerprint", { id, input }),
  delete: (id: string) => invoke<FingerprintListDto>("delete_fingerprint", { id }),
  setActive: (id: string) => invoke<FingerprintRow>("set_active_fingerprint", { id }),
  // ── IDE projectors (Phase 6) ──────────────────────────────────────
  listIdes: () => invoke<SupportedIde[]>("list_supported_ides"),
  applyToIde: (agentId: string, fingerprintId?: string) =>
    invoke<SupportedIde>("apply_fingerprint_to_ide", { agentId, fingerprintId }),
  restoreIde: (agentId: string) => invoke<SupportedIde>("restore_ide_baseline", { agentId }),
};
