import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FlatProvidersResponse } from "../../../../types";
import { modelsKeys } from "../keys";
import { useProviderMutations } from "../providers";

const mockInvoke = vi.mocked(invoke);

const BASE: FlatProvidersResponse = {
  version: 2,
  providers: [
    {
      id: "p1",
      name: "DeepSeek",
      api_key: "sk",
      base_url_openai: "https://api.deepseek.com/v1",
      base_url_anthropic: "",
      models: ["m1"],
      default_model: "m1",
      sort_index: 0,
    } as FlatProvidersResponse["providers"][number],
  ],
  tool_activations: { "claude-code": null },
};

function makeWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } },
  });
  client.setQueryData(modelsKeys.providersFlat(), structuredClone(BASE));
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

describe("useProviderMutations", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("create seeds the cache with the returned entity (no fake-id insert)", async () => {
    const created = { ...BASE.providers[0], id: "p-new", name: "Kimi", sort_index: 1 };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "create_provider_flat") return created;
      if (cmd === "get_providers_flat") return structuredClone(BASE);
      return undefined;
    });
    const { client, wrapper } = makeWrapper();
    const { result } = renderHook(() => useProviderMutations(), { wrapper });

    const entity = await result.current.createProvider({ name: "Kimi" });
    expect(entity.id).toBe("p-new");
    const cached = client.getQueryData<FlatProvidersResponse>(modelsKeys.providersFlat());
    expect(cached?.providers.map((p) => p.id)).toContain("p-new");
  });

  it("update applies optimistically and rolls back on error", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "update_provider_flat") throw new Error("boom");
      if (cmd === "get_providers_flat") return structuredClone(BASE);
      return undefined;
    });
    const { client, wrapper } = makeWrapper();
    const { result } = renderHook(() => useProviderMutations(), { wrapper });

    await expect(result.current.updateProvider("p1", { name: "Renamed" })).rejects.toThrow("boom");
    await waitFor(() => {
      const cached = client.getQueryData<FlatProvidersResponse>(modelsKeys.providersFlat());
      expect(cached?.providers[0].name).toBe("DeepSeek");
    });
  });

  it("delete optimistically clears activations bound to the provider", async () => {
    const withActivation = structuredClone(BASE);
    withActivation.tool_activations = { "claude-code": { provider_id: "p1", model: "m1" } };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_provider_flat") return undefined;
      if (cmd === "get_providers_flat") return structuredClone(withActivation);
      return undefined;
    });
    const { client, wrapper } = makeWrapper();
    client.setQueryData(modelsKeys.providersFlat(), withActivation);
    const { result } = renderHook(() => useProviderMutations(), { wrapper });

    await result.current.deleteProvider("p1");
    const cached = client.getQueryData<FlatProvidersResponse>(modelsKeys.providersFlat());
    expect(cached?.providers).toHaveLength(0);
    expect(cached?.tool_activations["claude-code"]).toBeNull();
  });
});
