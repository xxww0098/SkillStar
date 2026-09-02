import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusChip } from "./StatusChip";

describe("StatusChip", () => {
  it("renders the compact status mark, not the catalog Badge chrome", () => {
    render(
      <StatusChip tone="success" size="md">
        健康
      </StatusChip>,
    );
    const chip = screen.getByText("健康");
    expect(chip).toHaveClass("h-5");
    expect(chip.className).toMatch(/ring-1/);
    expect(chip.className).not.toMatch(/rounded-xl/);
  });
});
