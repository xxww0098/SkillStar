import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { UsageMeter } from "./UsageMeter";

describe("UsageMeter", () => {
  it("can show remaining percentage as the single header percentage", () => {
    render(<UsageMeter label="模型额度" usedPercent={46} badgePercent={54} badgeTitle="剩余 54%" footNote={null} />);

    const badge = screen.getByTitle("剩余 54%");
    expect(badge).toHaveTextContent("剩余 54%");
    expect(screen.queryByText("已用 46%")).not.toBeInTheDocument();
  });

  it("labels the default badge as used share so color is not the only signal", () => {
    render(<UsageMeter label="5 小时窗口" usedPercent={40} footNote={null} />);

    expect(screen.getByText("已用 40%")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "5 小时窗口" })).toHaveAttribute("aria-valuenow", "60");
  });

  it("adds a warning glyph when consumed share is in the warn/critical band", () => {
    const { container } = render(<UsageMeter label="7 天窗口" usedPercent={92} footNote={null} />);
    const badge = container.querySelector("[data-urgency='critical']");
    expect(badge).not.toBeNull();
    expect(badge).toHaveTextContent("已用 92%");
    expect(badge?.querySelector("svg")).not.toBeNull();
  });
});
