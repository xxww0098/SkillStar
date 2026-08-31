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
});
