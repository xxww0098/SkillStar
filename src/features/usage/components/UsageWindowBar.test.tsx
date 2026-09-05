import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { UsageWindowBar } from "./UsageWindowBar";
import type { UsageWindow } from "../types";

describe("UsageWindowBar", () => {
  it("renders compact breakdown category rows with pure percentages, never 'x / 100'", () => {
    const window: UsageWindow = {
      label: "Auto + Composer",
      used: 47,
      total: 100,
      percent: 47,
      reset_at: undefined,
      breakdown: [],
    };

    render(<UsageWindowBar window={window} compact />);

    expect(screen.getByText("47%")).toBeInTheDocument();
    expect(screen.queryByText(/47\s*\/\s*100/)).not.toBeInTheDocument();
    expect(screen.queryByText(/100/)).not.toBeInTheDocument();
  });

  it("renders zero-usage category rows as '0%', never '0 / 100'", () => {
    const window: UsageWindow = {
      label: "API",
      used: 0,
      total: 100,
      percent: 0,
      reset_at: undefined,
      breakdown: [],
    };

    render(<UsageWindowBar window={window} compact />);

    expect(screen.getByText("0%")).toBeInTheDocument();
    expect(screen.queryByText(/0\s*\/\s*100/)).not.toBeInTheDocument();
  });

  it("renders Antigravity breakdown rows with pure percentage", () => {
    const window: UsageWindow = {
      label: "Gemini Models · Weekly Limit",
      used: 46,
      total: 100,
      percent: 46,
      reset_at: Math.floor(Date.now() / 1000) + 86400 * 3,
      breakdown: [],
    };

    render(<UsageWindowBar window={window} compact catalogId="antigravity" />);

    expect(screen.getByText("46%")).toBeInTheDocument();
    expect(screen.queryByText(/46\s*\/\s*100/)).not.toBeInTheDocument();
  });

  it("renders nested breakdown inside monetary parent with percentage-only children", () => {
    const parent: UsageWindow = {
      label: "Total",
      used: 5916,
      total: 14287,
      percent: 41,
      reset_at: Math.floor(Date.now() / 1000) + 86400 * 7,
      breakdown: [
        { label: "Auto + Composer", used: 47, total: 100, percent: 47, reset_at: undefined, breakdown: [] },
        { label: "API", used: 3, total: 100, percent: 3, reset_at: undefined, breakdown: [] },
      ],
    };

    render(<UsageWindowBar window={parent} showCategoryReset={false} />);

    // Parent displays monetary usage
    expect(screen.getByText("$59.16")).toBeInTheDocument();
    expect(screen.getByText((c) => c.includes("$142.87"))).toBeInTheDocument();

    // Children display clean percentage
    expect(screen.getByText("47%")).toBeInTheDocument();
    expect(screen.getByText("3%")).toBeInTheDocument();
    expect(screen.queryByText(/47\s*\/\s*100/)).not.toBeInTheDocument();
    expect(screen.queryByText(/3\s*\/\s*100/)).not.toBeInTheDocument();
  });

  it("renders real absolute quota with used/total and percent", () => {
    const window: UsageWindow = {
      label: "Tokens",
      used: 1200,
      total: 5000,
      percent: 24,
      reset_at: undefined,
      breakdown: [],
    };

    render(<UsageWindowBar window={window} compact />);

    expect(screen.getByText("1.2k / 5.0k · 24%")).toBeInTheDocument();
  });

  it("lists Ollama model request counts under the weekly bar, not as quota percentages", () => {
    const window: UsageWindow = {
      label: "7d",
      used: 10,
      total: 100,
      percent: 10,
      reset_at: Math.floor(Date.now() / 1000) + 86400 * 3,
      breakdown: [
        { label: "glm-5.3-flash", used: 1294, breakdown: [] },
        { label: "web search", used: 3, breakdown: [] },
        { label: "web fetch", used: 2, breakdown: [] },
      ],
    };

    render(<UsageWindowBar window={window} />);

    expect(screen.getByText("本周用过的模型")).toBeInTheDocument();
    expect(screen.getByText("glm-5.3-flash")).toBeInTheDocument();
    expect(screen.getByText("1294 次")).toBeInTheDocument();
    expect(screen.getByText("web search")).toBeInTheDocument();
    expect(screen.getByText("3 次")).toBeInTheDocument();
    expect(screen.queryByText("1294%")).not.toBeInTheDocument();
    expect(screen.queryByText(/1294\s*\/\s*100/)).not.toBeInTheDocument();
    expect(screen.getByText(/剩余\s*90%/)).toBeInTheDocument();
    expect(screen.getByTitle(/重置倒计时/)).toBeInTheDocument();
  });

  it("places 5h and 7d reset chips on the remaining row, right-aligned", () => {
    const now = Math.floor(Date.now() / 1000);
    const hourly: UsageWindow = {
      label: "5h",
      used: 0,
      total: 100,
      percent: 0,
      reset_at: now + 2 * 3600 + 39 * 60,
      breakdown: [],
    };
    const weekly: UsageWindow = {
      label: "7d",
      used: 10,
      total: 100,
      percent: 10,
      reset_at: now + 3 * 86_400,
      breakdown: [],
    };

    const { rerender } = render(<UsageWindowBar window={hourly} />);
    const hourlyRemaining = screen.getByText(/剩余\s*100%/);
    expect(hourlyRemaining).toBeInTheDocument();
    const hourlyChip = screen.getByTitle(/重置倒计时/);
    expect(hourlyChip).toHaveTextContent("2h39m");
    expect(hourlyRemaining.parentElement).toContainElement(hourlyChip);

    rerender(<UsageWindowBar window={weekly} />);
    const weeklyRemaining = screen.getByText(/剩余\s*90%/);
    const weeklyChip = screen.getByTitle(/重置倒计时/);
    expect(weeklyRemaining.parentElement).toContainElement(weeklyChip);
  });
});
