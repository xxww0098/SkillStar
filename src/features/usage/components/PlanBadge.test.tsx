import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PlanBadge } from "./PlanBadge";

describe("PlanBadge", () => {
  it("keeps compact plan identifiers exactly as supplied", () => {
    render(<PlanBadge plan="MAX20X" />);

    const badge = screen.getByText("MAX20X");
    expect(badge).toHaveAttribute("title", "MAX20X");
    expect(badge).not.toHaveClass("uppercase");
  });

  it("keeps the provider tier and multiplier together", () => {
    render(<PlanBadge plan="PRO20X" />);

    expect(screen.getByText("PRO20X")).toBeInTheDocument();
  });

  it("does not truncate long plan names", () => {
    render(<PlanBadge plan="ENTERPRISE20X" />);

    expect(screen.getByText("ENTERPRISE20X")).toBeInTheDocument();
  });
});
