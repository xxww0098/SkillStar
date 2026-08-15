import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat, ToolBinding } from "../../../../../../types";
import { ompAssignableProviders, ompAssignedCount, OmpRolePanel } from "./OmpRolePanel";

const updateToolBindingSettings = vi.fn(() => Promise.resolve({ tool_id: "omp", success: true }));

vi.mock("../../../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({ updateToolBindingSettings }),
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
      smol: { provider_id: "alpha123", model: "   " },
      slow: { provider_id: "", model: "m-two" },
    };
    expect(ompAssignedCount(roles, ["default", "smol", "slow", "plan"])).toBe(1);
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
          smol: { provider_id: "alpha123", model: "m-two" },
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
      smol: { provider_id: "alpha123", model: "m-one" },
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
});
