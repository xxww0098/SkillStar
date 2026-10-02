import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RouteComparison } from "@/types/generated/RouteComparison";
import { RouteCandidates, ServingAgents } from "./RouteCandidates";

// The suite runs on the real zh-CN resources (see src/test/setup.ts).

const mockInvoke = vi.mocked(invoke);

const COMPARISON: RouteComparison = {
  model: "deepseek/deepseek-chat",
  candidates: [
    {
      catalog: "relay",
      calls: 2,
      error_rate: 0,
      p50_latency_ms: 400,
      p95_latency_ms: 500,
      tokens: { input: 200, output: 40, cache_read: 0, cache_write: 0, reasoning: 0 },
      cost_usd: null,
      resting: false,
      percent: 10,
      renews_at_ms: 1_790_003_600_000,
    },
    {
      catalog: "deepseek",
      calls: 4,
      error_rate: 0.5,
      p50_latency_ms: 100,
      p95_latency_ms: 300,
      tokens: { input: 400, output: 80, cache_read: 20, cache_write: 0, reasoning: 0 },
      cost_usd: 0.0001861,
      resting: true,
      percent: 99,
      renews_at_ms: 1_790_043_200_000,
    },
  ],
};

function renderWithClient(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("RouteCandidates", () => {
  it("lists the candidates in route_smart order with the allowance on the UI", () => {
    renderWithClient(
      <RouteCandidates
        agentName="Codex"
        modelRef="deepseek/deepseek-chat"
        comparison={COMPARISON}
        loading={false}
        onOpenPicker={vi.fn()}
      />,
    );

    const chips = screen.getAllByTestId("route-candidate-chip");
    expect(chips).toHaveLength(2);
    // Room first, used up last — the order the backend (route_smart) sent.
    expect(chips[0].textContent).toContain("relay");
    expect(chips[0].textContent).toContain("已用 10%");
    // A resting candidate says so; its allowance reads at the destructive
    // tier; the cost always carries the estimate marker.
    expect(chips[1].textContent).toContain("deepseek");
    expect(chips[1].textContent).toContain("休息中");
    expect(chips[1].textContent).toContain("已用 99%");
    expect(chips[1].textContent).toContain("$0.0002");
    // Calls and the latency vocabulary ride every chip with traffic.
    expect(chips[1].textContent).toContain("4 次调用");
    expect(chips[1].textContent).toContain("p50 100ms");
  });

  it("opens the selector when the compare button is clicked (the triangle's last leg)", () => {
    const onOpenPicker = vi.fn();
    renderWithClient(
      <RouteCandidates
        agentName="Codex"
        modelRef="m"
        comparison={COMPARISON}
        loading={false}
        onOpenPicker={onOpenPicker}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "对照并切换模型" }));
    expect(onOpenPicker).toHaveBeenCalledOnce();
  });

  it("says no candidate serves the model when the comparison is empty", () => {
    renderWithClient(
      <RouteCandidates
        agentName="Codex"
        modelRef="m"
        comparison={{ model: "m", candidates: [] }}
        loading={false}
        onOpenPicker={vi.fn()}
      />,
    );

    expect(screen.getByText("没有候选在服务这个模型。")).toBeInTheDocument();
  });
});

describe("ServingAgents", () => {
  const AGENTS = [
    { id: "codex", name: "Codex", model_label: "deepseek/deepseek-chat" },
    { id: "opencode", name: "OpenCode", model_label: "zai/glm-4.7" },
    { id: "pi", name: "Pi", model_label: "" },
  ];

  it("joins the agents whose current models route to the catalog", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "get_route_comparison") throw new Error(`unexpected ${cmd}`);
      const ref = String((args as Record<string, unknown> | undefined)?.modelRef ?? "");
      if (ref === "deepseek/deepseek-chat") return COMPARISON;
      return { model: ref, candidates: [{ ...COMPARISON.candidates[0], catalog: "zai" }] };
    });

    renderWithClient(<ServingAgents catalogId="zai" agents={AGENTS} onOpenPicker={vi.fn()} />);

    const section = await screen.findByTestId("serving-agents");
    // Only OpenCode's model resolves to a zai-attributed candidate; Codex's
    // candidates are relay/deepseek and Pi has no model at all.
    await waitFor(() => expect(within(section).getByText("OpenCode")).toBeTruthy());
    expect(within(section).queryByText("Codex")).toBeNull();
    expect(within(section).queryByText("Pi")).toBeNull();
    expect(within(section).getByText("zai/glm-4.7")).toBeTruthy();
  });

  it("keeps the empty sentence when no agent routes here", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd !== "get_route_comparison") throw new Error(`unexpected ${cmd}`);
      return { model: "", candidates: [] };
    });

    renderWithClient(<ServingAgents catalogId="zai" agents={AGENTS} onOpenPicker={vi.fn()} />);

    expect(await screen.findByText("还没有 Agent 的当前模型路由到这里。")).toBeInTheDocument();
  });

  it("opens that agent's selector on click", async () => {
    const onOpenPicker = vi.fn();
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "get_route_comparison") throw new Error(`unexpected ${cmd}`);
      const ref = String((args as Record<string, unknown> | undefined)?.modelRef ?? "");
      if (ref === "deepseek/deepseek-chat") return COMPARISON;
      return { model: ref, candidates: [] };
    });

    renderWithClient(<ServingAgents catalogId="deepseek" agents={AGENTS} onOpenPicker={onOpenPicker} />);

    fireEvent.click(await screen.findByRole("button", { name: /Codex/ }));
    await waitFor(() => expect(onOpenPicker).toHaveBeenCalledWith("codex"));
  });
});
