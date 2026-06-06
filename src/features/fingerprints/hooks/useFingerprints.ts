import { useCallback, useEffect, useState } from "react";
import { fingerprintsApi } from "../api";
import type { FingerprintListDto, FingerprintRow, PresetTemplate, UpdateFingerprintInput } from "../types";

interface FingerprintsHookState {
  items: FingerprintRow[];
  activeId: string | null;
  presets: PresetTemplate[];
  loading: boolean;
  error: string | null;
}

interface FingerprintsHookActions {
  reload: () => Promise<void>;
  createFromPreset: (presetId: string, name: string) => Promise<FingerprintRow>;
  update: (id: string, input: UpdateFingerprintInput) => Promise<FingerprintRow>;
  remove: (id: string) => Promise<void>;
  setActive: (id: string) => Promise<FingerprintRow>;
}

/**
 * Local cache + mutations for the fingerprint store. Keeps a single source
 * of truth for FingerprintsPanel and FingerprintPicker so list + picker
 * stay in lockstep without React Query for now.
 */
export function useFingerprints(): FingerprintsHookState & FingerprintsHookActions {
  const [items, setItems] = useState<FingerprintRow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [presets, setPresets] = useState<PresetTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const applyList = useCallback((dto: FingerprintListDto) => {
    setItems(dto.items);
    setActiveId(dto.activeId);
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [dto, presetList] = await Promise.all([fingerprintsApi.list(), fingerprintsApi.listPresets()]);
      applyList(dto);
      setPresets(presetList);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [applyList]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const createFromPreset = useCallback(
    async (presetId: string, name: string): Promise<FingerprintRow> => {
      const row = await fingerprintsApi.createFromPreset(presetId, name);
      // Refresh the whole list so the new entry shows up in the right order.
      await reload();
      return row;
    },
    [reload],
  );

  const update = useCallback(async (id: string, input: UpdateFingerprintInput): Promise<FingerprintRow> => {
    const row = await fingerprintsApi.update(id, input);
    setItems((prev) => prev.map((it) => (it.id === id ? row : it)));
    return row;
  }, []);

  const remove = useCallback(
    async (id: string) => {
      const dto = await fingerprintsApi.delete(id);
      applyList(dto);
    },
    [applyList],
  );

  const setActive = useCallback(async (id: string): Promise<FingerprintRow> => {
    const row = await fingerprintsApi.setActive(id);
    setActiveId(id);
    setItems((prev) => prev.map((it) => ({ ...it, isActive: it.id === id })));
    return row;
  }, []);

  return {
    items,
    activeId,
    presets,
    loading,
    error,
    reload,
    createFromPreset,
    update,
    remove,
    setActive,
  };
}
