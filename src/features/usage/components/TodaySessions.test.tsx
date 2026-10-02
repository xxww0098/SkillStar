import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TodayConsumption } from "../types";
import { TodaySessions } from "./TodaySessions";

// The suite runs on the real zh-CN resources (see src/test/setup.ts), so the
// assertions double as locale-presence checks for the chip copy.

function today(overrides: Partial<TodayConsumption> = {}): TodayConsumption {
  return {
    totals: {
      calls: 2,
      errors: 0,
      input: 210,
      output: 42,
      cache_read: 0,
      cache_write: 0,
      reasoning: 0,
      cost_usd: 0.0184,
      unpriced: 0,
      mean_latency_ms: 1_200,
    },
    by_agent: [],
    chips: [
      {
        agent: "claude-code",
        session: "s-1",
        title: "deepseek-chat",
        last_active: 1_790_000_600_000,
        tokens: { input: 180, output: 36, cache_read: 9, cache_write: 0, reasoning: 0 },
        cost_usd: 0.0162,
        via_gateway: true,
      },
      {
        agent: "opencode",
        session: "s-2",
        title: null,
        last_active: 1_790_000_000_000,
        tokens: { input: 30, output: 7, cache_read: 0, cache_write: 0, reasoning: 0 },
        cost_usd: null,
        via_gateway: false,
      },
    ],
    ...overrides,
  };
}

describe("TodaySessions", () => {
  it("renders one chip per session, newest first, with the estimate and the gateway mark", () => {
    render(<TodaySessions today={today()} onFocusAgent={vi.fn()} />);

    const strip = screen.getByTestId("today-sessions");
    const chips = within(strip).getAllByRole("button");
    expect(chips).toHaveLength(2);
    // Newest activity first: the claude-code session (later last_active).
    expect(chips[0].textContent).toContain("claude-code");
    expect(chips[0].textContent).toContain("deepseek-chat");
    expect(chips[0].textContent).toContain("$0.0162");
    expect(within(chips[0]).getByRole("img", { name: "经网关" })).toBeTruthy();
    // A session without a title falls back to its id; an unpriced session
    // shows no cost figure rather than a confident zero.
    expect(chips[1].textContent).toContain("opencode");
    expect(chips[1].textContent).toContain("s-2");
    expect(chips[1].textContent).not.toContain("$");
  });

  it("opens the agent in Models when a chip is clicked (the agent entry)", () => {
    const onFocusAgent = vi.fn();
    render(<TodaySessions today={today()} onFocusAgent={onFocusAgent} />);

    fireEvent.click(screen.getByRole("button", { name: /claude-code/ }));
    expect(onFocusAgent).toHaveBeenCalledWith("claude-code");
  });

  it("keeps the fixed empty sentence when the read landed with nothing", () => {
    render(<TodaySessions today={{ ...today(), chips: [] }} onFocusAgent={vi.fn()} />);

    expect(screen.getByText("还没有调用")).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("stays hidden until the read lands", () => {
    const { container } = render(<TodaySessions today={null} onFocusAgent={vi.fn()} />);
    expect(container.firstChild).not.toBeNull();

    const hidden = render(<TodaySessions onFocusAgent={vi.fn()} />);
    expect(hidden.container.firstChild).toBeNull();
  });
});
