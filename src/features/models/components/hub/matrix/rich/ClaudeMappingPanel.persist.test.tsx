/**
 * The link that was missing (00 §1.3).
 *
 * The backend has read Claude's tier models and written
 * `ANTHROPIC_DEFAULT_{HAIKU,SONNET,OPUS}_MODEL` for three schema versions. The
 * panel held its value in a `useState` nothing ever read, so the user filled in
 * a form, saw it accepted, and nothing reached disk — a form that lies.
 *
 * These tests assert the renderer's half of the chain: an edit becomes an
 * `update_agent_settings` call carrying a role map. The backend's half —
 * role map → store → env block — is
 * `tool_sync::tests::roles::claude_role_mapping_lands_in_the_env_block`, and the
 * wire shim between them is `roles_round_trip_through_the_v3_settings_bag`.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat, ToolBinding } from "../../../../../../types";
import { ClaudeMappingPanel } from "./ClaudeMappingPanel";

const updateToolBindingSettings = vi.fn(() => Promise.resolve({ tool_id: "claude-code", success: true }));

vi.mock("../../../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({ updateToolBindingSettings, updateProvider: vi.fn() }),
}));

vi.mock("../../../../api/modelCatalog", () => ({
  useModelFetch: () => ({ fetchModelCatalog: vi.fn(), isLoading: false }),
}));

const descriptor = {
  id: "claude-code",
  display_name: "Claude Code",
  kind: "single" as const,
  required_wire: "anthropic_messages" as const,
  roles: [
    { id: "default", agent_key: "ANTHROPIC_MODEL", primary: true, inherits: null, requires: "any" as const },
    {
      id: "fast",
      agent_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      primary: true,
      inherits: null,
      requires: "any" as const,
    },
    {
      id: "subagent",
      agent_key: "CLAUDE_CODE_SUBAGENT_MODEL",
      primary: false,
      inherits: "default",
      requires: "any" as const,
    },
  ],
  config_files: [],
};
const roleDrops = vi.fn(() => [] as { role: string; reason: string }[]);

vi.mock("../../../../api/agents", () => ({
  useAgentDescriptor: () => descriptor,
  useRoleDrops: () => roleDrops(),
}));

function provider(): ProviderEntryFlat {
  return {
    id: "p1",
    name: "Relay",
    base_url_openai: "",
    base_url_anthropic: "https://relay.example.com/anthropic",
    models_url: "",
    api_key: "sk",
    models: ["big-model", "small-model"],
    default_model: "big-model",
    sort_index: 0,
  };
}

function binding(roles: Record<string, { provider_id: string; model: string }> = {}): ToolBinding {
  return {
    entries: [{ provider_id: "p1", model: "big-model" }],
    active_index: 0,
    settings: { roles },
  };
}

beforeEach(() => {
  updateToolBindingSettings.mockClear();
  roleDrops.mockReturnValue([]);
});

describe("ClaudeMappingPanel persistence", () => {
  it("writes an edited tier model through to the binding", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);

    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "small-model" } });
    fireEvent.blur(input);

    expect(updateToolBindingSettings).toHaveBeenCalledWith("claude-code", {
      roles: { fast: { provider_id: "p1", model: "small-model" } },
    });
  });

  it("clears the role rather than storing a blank model", () => {
    render(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "p1", model: "small-model" } })}
      />,
    );

    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.blur(input);

    // An empty string would be written as `ANTHROPIC_DEFAULT_HAIKU_MODEL: ""`,
    // which Claude Code cannot resolve. Removing the key is what restores its
    // built-in default.
    expect(updateToolBindingSettings).toHaveBeenCalledWith("claude-code", { roles: {} });
  });

  it("renders only the roles the registry declares", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);

    expect(screen.getByLabelText("Default model")).toBeTruthy();
    expect(screen.getByLabelText("Subagent model")).toBeTruthy();
    // Claude Code has no `ANTHROPIC_DEFAULT_FABLE_MODEL`, so the row that used
    // to exist for it is gone — it could never have been written.
    expect(screen.queryByLabelText("Fable model")).toBeNull();
  });

  it("names the env key each row writes", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    expect(screen.getByText(/ANTHROPIC_DEFAULT_HAIKU_MODEL/)).toBeTruthy();
    expect(screen.getByText(/CLAUDE_CODE_SUBAGENT_MODEL/)).toBeTruthy();
  });

  it("says what an unassigned row falls back to", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    // `subagent` inherits `default`; the tier keys inherit nothing SkillStar
    // models, so they say the agent decides rather than naming a fallback that
    // does not exist.
    // The suite runs under zh-CN, so the copy is asserted in that locale.
    expect(screen.getByText(/回落到 default/)).toBeTruthy();
    expect(screen.getAllByText(/由该 Agent 自行选择/).length).toBeGreaterThan(0);
  });

  it("marks a row the last write skipped, with the backend's reason", () => {
    roleDrops.mockReturnValue([{ role: "fast", reason: "provider_not_bound" }]);
    render(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "other", model: "cheap" } })}
      />,
    );
    expect(screen.getByText(/没有绑定到此 Agent/)).toBeTruthy();
  });
});
