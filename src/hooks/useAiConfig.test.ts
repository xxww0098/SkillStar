import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AiConfig } from "../types";
import { getAiConfigCached, invalidateAiConfigCache, useAiConfig } from "./useAiConfig";

const mockedInvoke = vi.mocked(invoke);

const MOCK_CONFIG: AiConfig = {
  enabled: true,
  api_format: "openai",
  base_url: "https://api.example.com",
  api_key: "sk-test-key",
  model: "gpt-5.4",
  target_language: "zh-CN",
  short_text_priority: "ai_first",
  context_window_k: 128,
  max_concurrent_requests: 4,
  chunk_char_limit: 0,
  scan_max_response_tokens: 0,
  security_scan_telemetry_enabled: false,
  openai_preset: { base_url: "", api_key: "", model: "" },
  anthropic_preset: { base_url: "", api_key: "", model: "" },
  local_preset: { base_url: "http://127.0.0.1:11434/v1", api_key: "", model: "llama3.1:8b" },
};

describe("useAiConfig", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateAiConfigCache();
  });

  it("should load config from backend on mount", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_CONFIG);

    const { result } = renderHook(() => useAiConfig());
    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.config.enabled).toBe(true);
    expect(result.current.config.model).toBe("gpt-5.4");
    expect(mockedInvoke).toHaveBeenCalledWith("get_ai_config");
  });

  it("should use default config when backend call fails", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("Backend error"));

    const { result } = renderHook(() => useAiConfig());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    // Falls back to DEFAULT_CONFIG
    expect(result.current.config.enabled).toBe(false);
    expect(result.current.config.model).toBe("gpt-5.4");
  });

  it("saveConfig should invoke backend and update local state", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_CONFIG); // initial load
    mockedInvoke.mockResolvedValueOnce(undefined); // save call

    const { result } = renderHook(() => useAiConfig());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const updated = { ...MOCK_CONFIG, model: "gpt-6" };
    await act(async () => {
      await result.current.saveConfig(updated);
    });

    expect(mockedInvoke).toHaveBeenCalledWith("save_ai_config", { config: updated });
    expect(result.current.config.model).toBe("gpt-6");
  });
});

describe("getAiConfigCached", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateAiConfigCache();
  });

  it("should deduplicate concurrent calls", async () => {
    mockedInvoke.mockResolvedValue(MOCK_CONFIG);

    // Fire two concurrent calls
    const [a, b] = await Promise.all([getAiConfigCached(), getAiConfigCached()]);

    expect(a).toEqual(b);
    // Should only have made one actual invoke call
    expect(mockedInvoke).toHaveBeenCalledTimes(1);
  });

  it("should return cached value within TTL", async () => {
    mockedInvoke.mockResolvedValue(MOCK_CONFIG);

    await getAiConfigCached();
    await getAiConfigCached();

    expect(mockedInvoke).toHaveBeenCalledTimes(1);
  });

  it("should refresh after invalidation", async () => {
    mockedInvoke.mockResolvedValue(MOCK_CONFIG);

    await getAiConfigCached();
    invalidateAiConfigCache();

    mockedInvoke.mockResolvedValue({ ...MOCK_CONFIG, model: "updated" });
    const result = await getAiConfigCached();

    expect(result.model).toBe("updated");
    expect(mockedInvoke).toHaveBeenCalledTimes(2);
  });
});
