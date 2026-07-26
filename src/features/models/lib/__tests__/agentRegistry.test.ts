import { describe, expect, it } from "vitest";
import { CONFIG_FILE_TOOLS, PROVIDER_AGENTS } from "../agentRegistry";

/**
 * Cross-language registry reconciliation.
 *
 * The Rust agent registry (`crates/skillstar-models/src/tool_sync/agents.rs`)
 * pins the same literal list in
 * `registry_covers_exactly_the_known_agents_in_order`. If either side adds,
 * removes or reorders an agent without the other, exactly one of the two
 * tests goes red.
 */
const CANONICAL_TOOL_IDS = ["claude-code", "codex", "opencode", "gemini", "pi"] as const;

describe("agentRegistry", () => {
  it("covers exactly the canonical agents, in canonical order", () => {
    expect(PROVIDER_AGENTS.map((a) => a.toolId)).toEqual([...CANONICAL_TOOL_IDS]);
  });

  it("exposes config-file editing for every agent", () => {
    expect(CONFIG_FILE_TOOLS.map((t) => t.toolId)).toEqual([...CANONICAL_TOOL_IDS]);
  });

  it("kind matches the backend store-layer decision point", () => {
    // Mirrors providers::crud::agent_supports_multiple_providers.
    const kinds = Object.fromEntries(PROVIDER_AGENTS.map((a) => [a.toolId, a.kind]));
    expect(kinds).toEqual({
      "claude-code": "single",
      codex: "multi",
      opencode: "multi",
      gemini: "single",
      pi: "multi",
    });
  });

  it("requiredUrlField matches the backend activation validation", () => {
    // Mirrors the RequiredUrl column of the Rust registry.
    for (const agent of PROVIDER_AGENTS) {
      expect(agent.requiredUrlField).toBe(agent.toolId === "claude-code" ? "anthropic" : "openai");
    }
  });
});
