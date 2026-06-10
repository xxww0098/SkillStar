import { describe, expect, it } from "vitest";
import type { ProviderEntryFlat, ToolActivation } from "../../../../types";
import { PROVIDER_AGENTS } from "../agentRegistry";
import {
  type AgentStatusInput,
  computeAgentStatus,
  isConnected,
  isProblem,
  summarizeAgentStatuses,
} from "../agentStatus";

const claude = PROVIDER_AGENTS.find((a) => a.toolId === "claude-code");
const codex = PROVIDER_AGENTS.find((a) => a.toolId === "codex");
if (!claude || !codex) throw new Error("registry missing agents");

const provider = {
  id: "p1",
  name: "X",
  api_key: "sk",
  base_url_openai: "https://api.x.com/v1",
  base_url_anthropic: "https://api.x.com/anthropic",
  models: [],
  default_model: "m",
  sort_index: 0,
} as unknown as ProviderEntryFlat;

const activation: ToolActivation = { provider_id: "p1", model: "m" };

function input(overrides: Partial<AgentStatusInput> = {}): AgentStatusInput {
  return {
    agent: claude!,
    activation,
    boundProvider: provider,
    installed: true,
    installLoading: false,
    ...overrides,
  };
}

describe("computeAgentStatus", () => {
  it("reports not_installed only after detection finished", () => {
    expect(computeAgentStatus(input({ installed: false })).kind).toBe("not_installed");
    // While detection is loading we stay optimistic.
    expect(computeAgentStatus(input({ installed: false, installLoading: true })).kind).not.toBe("not_installed");
  });

  it("is inactive without an activation or resolvable provider", () => {
    expect(computeAgentStatus(input({ activation: null })).kind).toBe("inactive");
    expect(computeAgentStatus(input({ boundProvider: null })).kind).toBe("inactive");
  });

  it("flags missing required endpoint as misconfigured", () => {
    const noAnthropic = { ...provider, base_url_anthropic: "" } as ProviderEntryFlat;
    const status = computeAgentStatus(input({ boundProvider: noAnthropic }));
    expect(status).toEqual({ kind: "misconfigured", requiredUrlField: "anthropic" });
    // codex needs the openai URL instead — same provider is fine for codex? no: openai URL present
    expect(computeAgentStatus(input({ agent: codex!, boundProvider: noAnthropic })).kind).toBe("unverified");
  });

  it("syncing wins over probe results; unverified until a probe lands", () => {
    expect(computeAgentStatus(input({ isSyncing: true, probe: { status: "ok", latency_ms: 10 } })).kind).toBe(
      "syncing",
    );
    expect(computeAgentStatus(input({ probing: true })).kind).toBe("unverified");
    expect(computeAgentStatus(input({})).kind).toBe("unverified");
  });

  it("maps probe outcomes onto the canonical statuses", () => {
    expect(computeAgentStatus(input({ probe: { status: "ok", latency_ms: 142 } }))).toEqual({
      kind: "healthy",
      latencyMs: 142,
    });
    expect(computeAgentStatus(input({ probe: { status: "auth_failed" } })).kind).toBe("auth_failed");
    expect(computeAgentStatus(input({ probe: { status: "timeout" } })).kind).toBe("timeout");
    expect(computeAgentStatus(input({ probe: { status: "network_error" } }))).toEqual({
      kind: "error",
      detail: "network_error",
    });
    expect(computeAgentStatus(input({ probe: { status: "model_unavailable" } }))).toEqual({
      kind: "error",
      detail: "model_unavailable",
    });
  });
});

describe("summary helpers", () => {
  it("counts connected and problem agents", () => {
    const statuses = [
      computeAgentStatus(input({ probe: { status: "ok", latency_ms: 100 } })),
      computeAgentStatus(input({ activation: null })),
      computeAgentStatus(input({ probe: { status: "auth_failed" } })),
      computeAgentStatus(input({ installed: false })),
    ];
    expect(statuses.map(isConnected)).toEqual([true, false, false, false]);
    expect(statuses.map(isProblem)).toEqual([false, false, true, false]);
    expect(summarizeAgentStatuses(statuses)).toEqual({ connected: 1, problems: 1, total: 4 });
  });
});
