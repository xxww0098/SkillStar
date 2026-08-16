import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat, ToolBinding } from "../../../../../../types";
import { ompAssignableProviders, ompAssignedCount, OmpRolePanel } from "./OmpRolePanel";

const updateToolBindingSettings = vi.fn(() => Promise.resolve({ tool_id: "omp", success: true }));

vi.mock("../../../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({ updateToolBindingSettings }),
}));

/** Registry rows and the last write's skipped roles, both normally served over
 *  IPC. Stubbed rather than wired to a QueryClient: what is under test is what
 *  the panel does with them, not how they arrive. */
const descriptor = {
  id: "omp",
  display_name: "Oh My Pi",
  kind: "multi" as const,
  required_wire: "openai_chat" as const,
  roles: [
    { id: "default", agent_key: "default", primary: true, inherits: null, requires: "any" as const },
    { id: "fast", agent_key: "smol", primary: true, inherits: "default", requires: "any" as const },
    { id: "slow", agent_key: "slow", primary: true, inherits: "default", requires: "any" as const },
    { id: "plan", agent_key: "plan", primary: true, inherits: null, requires: "any" as const },
  ],
  config_files: [],
};
const roleDrops = vi.fn(() => [] as { role: string; reason: string }[]);

vi.mock("../../../../api/agents", () => ({
  useAgentDescriptor: () => descriptor,
  useRoleDrops: () => roleDrops(),
}));

function provider(id: string, overrides: Partial<ProviderEntryFlat> = {}): ProviderEntryFlat {
  return {
    id,
    name: `P-${id}`,
    base_url_openai: "https://api.example.com/v1",
    base_url_anthropic: "",
    models_url: "",
    api_key: "k",
    models: ["m-one", "m-two"],
    default_model: "m-one",
    sort_index: 0,
    ...overrides,
  };
}

function binding(overrides: Partial<ToolBinding> = {}): ToolBinding {
  return {
    entries: [{ provider_id: "alpha123", model: "m-one" }],
    active_index: 0,
    ...overrides,
  };
}

/** Last `roles` map handed to the persistence mutation. */
function lastRoles() {
  const call = updateToolBindingSettings.mock.calls.at(-1) as unknown as [string, { roles: Record<string, unknown> }];
  return call[1].roles;
}

beforeEach(() => {
  updateToolBindingSettings.mockClear();
  roleDrops.mockReturnValue([]);
});

describe("ompAssignableProviders", () => {
  it("keeps only providers bound to OMP that have an OpenAI base URL", () => {
    const providers = [provider("alpha123"), provider("beta456", { base_url_openai: "" }), provider("gamma789")];
    const bound = binding({
      entries: [
        { provider_id: "alpha123", model: "m-one" },
        { provider_id: "beta456", model: "m-one" },
      ],
    });
    // gamma is unbound, beta has no OpenAI URL — the writer would skip both.
    expect(ompAssignableProviders(bound, providers).map((p) => p.id)).toEqual(["alpha123"]);
  });

  it("returns nothing for an unbound tool", () => {
    expect(ompAssignableProviders(null, [provider("alpha123")])).toEqual([]);
  });
});

describe("ompAssignedCount", () => {
  it("counts only roles that carry both a provider and a non-blank model", () => {
    const roles = {
      default: { provider_id: "alpha123", model: "m-one" },
      fast: { provider_id: "alpha123", model: "   " },
      slow: { provider_id: "", model: "m-two" },
    };
    expect(ompAssignedCount(roles, ["default", "fast", "slow", "plan"])).toBe(1);
  });
});

describe("OmpRolePanel", () => {
  it("shows the four flag-backed roles up front and hides the rest behind a disclosure", () => {
    render(<OmpRolePanel binding={binding()} providers={[provider("alpha123")]} />);

    for (const flag of ["--model", "--smol", "--slow", "--plan"]) {
      expect(screen.getByText(flag)).toBeInTheDocument();
    }
    expect(screen.queryByLabelText("视觉 模型")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText(/更多角色/));
    expect(screen.getByLabelText("视觉 模型")).toBeInTheDocument();
  });

  it("persists a model edit on blur and previews the managed value", () => {
    const bound = binding({ settings: { roles: { default: { provider_id: "alpha123", model: "m-one" } } } });
    render(<OmpRolePanel binding={bound} providers={[provider("alpha123")]} />);

    expect(screen.getByText("skillstar_alpha123/m-one")).toBeInTheDocument();

    const input = screen.getByLabelText("默认 模型");
    fireEvent.change(input, { target: { value: "m-two" } });
    fireEvent.blur(input);

    expect(updateToolBindingSettings).toHaveBeenCalledWith("omp", {
      roles: { default: { provider_id: "alpha123", model: "m-two" } },
    });
  });

  it("does not write when a blurred model is unchanged", () => {
    const bound = binding({ settings: { roles: { default: { provider_id: "alpha123", model: "m-one" } } } });
    render(<OmpRolePanel binding={bound} providers={[provider("alpha123")]} />);

    fireEvent.blur(screen.getByLabelText("默认 模型"));
    expect(updateToolBindingSettings).not.toHaveBeenCalled();
  });

  it("drops the role entirely when cleared", () => {
    const bound = binding({
      settings: {
        roles: {
          default: { provider_id: "alpha123", model: "m-one" },
          fast: { provider_id: "alpha123", model: "m-two" },
        },
      },
    });
    render(<OmpRolePanel binding={bound} providers={[provider("alpha123")]} />);

    fireEvent.click(screen.getByLabelText("清除 轻量"));
    expect(lastRoles()).toEqual({ default: { provider_id: "alpha123", model: "m-one" } });
  });

  it("fills every unset primary role from the active entry", () => {
    render(<OmpRolePanel binding={binding()} providers={[provider("alpha123")]} />);

    fireEvent.click(screen.getByText("填充主要角色"));
    expect(lastRoles()).toEqual({
      default: { provider_id: "alpha123", model: "m-one" },
      fast: { provider_id: "alpha123", model: "m-one" },
      slow: { provider_id: "alpha123", model: "m-one" },
      plan: { provider_id: "alpha123", model: "m-one" },
    });
  });

  it("builds the equivalent command from the flag-backed roles only", () => {
    const bound = binding({
      settings: {
        roles: {
          default: { provider_id: "alpha123", model: "m-one" },
          slow: { provider_id: "alpha123", model: "m-two", thinking: "xhigh" },
          vision: { provider_id: "alpha123", model: "m-vlm" },
        },
      },
    });
    render(<OmpRolePanel binding={bound} providers={[provider("alpha123")]} />);

    expect(
      screen.getByText('omp --model "skillstar_alpha123/m-one" --slow "skillstar_alpha123/m-two:xhigh"'),
    ).toBeInTheDocument();
  });

  it("explains the empty state instead of offering unwritable providers", () => {
    render(<OmpRolePanel binding={binding()} providers={[provider("alpha123", { base_url_openai: "" })]} />);

    expect(screen.getByText(/还没有绑定到 OMP 的 Provider/)).toBeInTheDocument();
    expect(screen.queryByText("--smol")).not.toBeInTheDocument();
  });

  /**
   * 02 §9.3 gap 3: the fallback table existed and was used only to decide
   * whether to nag. The row now says what leaving it empty will actually do,
   * and says it from the backend registry rather than a local copy.
   */
  it("tells an empty row what it falls back to", () => {
    render(<OmpRolePanel binding={binding()} providers={[provider("alpha123")]} />);

    // `fast` (OMP's `smol`) inherits `default`; `plan` does not, so it must
    // not claim to.
    // `fast` and `slow` both declare it; `plan` does not.
    expect(screen.getAllByText(/回落到 default/)).toHaveLength(2);
    expect(screen.getAllByText(/config\.yml 里的原值保持不变/).length).toBeGreaterThan(0);
  });

  /**
   * 02 §9.3 gap 1: a role the writer skipped used to leave the panel showing an
   * assignment the file did not have.
   */
  it("marks a role the last write skipped, with the reason", () => {
    roleDrops.mockReturnValue([{ role: "fast", reason: "provider_not_bound" }]);
    render(
      <OmpRolePanel
        binding={binding({ settings: { roles: { fast: { provider_id: "beta456", model: "m-two" } } } })}
        providers={[provider("alpha123")]}
      />,
    );

    expect(screen.getByText(/未写入/)).toBeInTheDocument();
    expect(screen.getByText(/没有绑定到此 Agent/)).toBeInTheDocument();
  });
});
