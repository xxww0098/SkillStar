import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { UsageMeter } from "./UsageMeter";

describe("UsageMeter", () => {
  it("can show remaining percentage as the single header percentage", () => {
    render(<UsageMeter label="模型额度" usedPercent={46} badgePercent={54} badgeTitle="剩余 54%" footNote={null} />);

    expect(screen.getByTitle("剩余 54%")).toHaveTextContent("54%");
    expect(screen.queryByText("46%")).not.toBeInTheDocument();
  });
});
