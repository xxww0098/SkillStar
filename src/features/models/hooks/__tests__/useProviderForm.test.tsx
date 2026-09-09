import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../../types";
import { useAutosave } from "../useAutosave";
import { useProviderForm } from "../useProviderForm";

const { updateProvider } = vi.hoisted(() => ({ updateProvider: vi.fn() }));
vi.mock("../../api/providers", () => ({ useProviderMutations: () => ({ updateProvider }) }));
vi.mock("../../api/presets", () => ({ useProviderPresets: () => ({ presets: [] }) }));
vi.mock("../../api/modelCatalog", () => ({ useModelFetch: () => ({ fetchModelCatalog: vi.fn() }) }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const provider = {
  id: "p1",
  name: "Original",
  preset_id: "custom",
  api_key: "",
  base_url_openai: "https://example.com/v1",
  base_url_anthropic: "",
  models_url: "",
  models: ["model"],
  default_model: "model",
  sort_index: 0,
  meta: { extra: { retained: true }, claude_main_model: "tier-not-in-models" },
} as ProviderEntryFlat;

describe("useProviderForm safe patches", () => {
  it("flushes edits made in flight relative to the successful snapshot, not the cache", async () => {
    let finishSave!: (entry: ProviderEntryFlat) => void;
    updateProvider.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishSave = resolve;
        }),
    );
    const { result } = renderHook(() => {
      const form = useProviderForm(provider);
      const autosave = useAutosave({ dirty: form.dirty, save: form.save, changeToken: form.values });
      return { ...form, flush: autosave.flush };
    });
    act(() => result.current.setField("apiKey", "sk-new"));
    let flushing!: ReturnType<typeof result.current.flush>;
    act(() => {
      flushing = result.current.flush();
    });
    act(() => result.current.setField("name", "Typed while saving"));
    await act(async () => {
      finishSave(provider);
      expect(await flushing).toBe("saved");
    });
    expect(updateProvider.mock.calls).toEqual([
      ["p1", { api_key: "sk-new" }],
      ["p1", { name: "Typed while saving" }],
    ]);
    expect(result.current.dirty).toBe(false);
  });

  it("preserves unknown metadata, including refreshed extras, only on metadata edits", async () => {
    const { result, rerender } = renderHook(({ entry }) => useProviderForm(entry), {
      initialProps: { entry: provider },
    });
    rerender({ entry: { ...provider, meta: { ...provider.meta, newer_extra: ["keep"] } } });
    expect(result.current.dirty).toBe(false);
    act(() => result.current.setField("timeout", 45));
    await act(async () => {
      expect(await result.current.save()).toBe("saved");
    });
    const patch = updateProvider.mock.calls[0][1];
    expect(Object.keys(patch)).toEqual(["meta"]);
    expect(patch.meta).toMatchObject({
      extra: { retained: true },
      newer_extra: ["keep"],
      timeout: 45,
      claude_main_model: "tier-not-in-models",
    });
    act(() => result.current.setField("notes", "Updated notes"));
    await act(async () => {
      await result.current.save();
    });
    expect(updateProvider.mock.calls[1]).toEqual(["p1", { notes: "Updated notes" }]);
  });

  it("retries the same patch after failure despite same-id optimistic cache values", async () => {
    let rejectSave!: (error: Error) => void;
    updateProvider.mockImplementationOnce(
      () =>
        new Promise((_, reject) => {
          rejectSave = reject;
        }),
    );
    const { result, rerender } = renderHook(({ entry }) => useProviderForm(entry), {
      initialProps: { entry: provider },
    });
    act(() => result.current.setField("name", "Renamed"));
    let saving!: ReturnType<typeof result.current.save>;
    act(() => {
      saving = result.current.save();
    });
    rerender({ entry: { ...provider, name: "Renamed" } });
    expect(result.current.dirty).toBe(true);
    await act(async () => {
      rejectSave(new Error("offline"));
      expect(await saving).toBe("error");
    });
    expect(result.current.dirty).toBe(true);
    await act(async () => {
      expect(await result.current.save()).toBe("saved");
    });
    expect(updateProvider.mock.calls).toEqual([
      ["p1", { name: "Renamed" }],
      ["p1", { name: "Renamed" }],
    ]);
    rerender({ entry: provider });
    expect(result.current.values.name).toBe("Renamed");
    expect(result.current.dirty).toBe(false);
  });

  it("still sends an explicit empty key when clearing a saved key", async () => {
    const { result } = renderHook(() => useProviderForm({ ...provider, api_key: "sk-existing" }));
    act(() => result.current.setField("apiKey", ""));
    await act(async () => {
      expect(await result.current.save()).toBe("saved");
    });
    expect(updateProvider).toHaveBeenCalledWith("p1", { api_key: "" });
  });

  beforeEach(() => {
    updateProvider.mockReset();
    updateProvider.mockResolvedValue(provider);
  });

  it("sends only name when an untouched credential is projected as empty", async () => {
    const { result } = renderHook(() => useProviderForm(provider));
    act(() => result.current.setField("name", "Renamed"));
    await act(async () => {
      expect(await result.current.save()).toBe("saved");
    });
    expect(updateProvider).toHaveBeenCalledWith("p1", { name: "Renamed" });
    expect(result.current.dirty).toBe(false);
  });
});
