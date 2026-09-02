import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KeepAliveOutlet } from "./KeepAliveOutlet";

const KEEP = ["a", "b", "c"] as const;

function Harness({ active }: { active: string }) {
  return (
    <KeepAliveOutlet
      active={active}
      keep={KEEP}
      limit={2}
      render={(id) => <div data-testid={`page-${id}`}>{id}</div>}
    />
  );
}

describe("KeepAliveOutlet", () => {
  it("keeps the previous list page mounted and hidden when switching", () => {
    const { rerender } = render(<Harness active="a" />);
    expect(screen.getByTestId("page-a")).toBeInTheDocument();

    rerender(<Harness active="b" />);
    expect(screen.getByTestId("page-a")).toBeInTheDocument();
    expect(screen.getByTestId("page-a")).not.toBeVisible();
    expect(screen.getByTestId("page-b")).toBeVisible();
  });

  it("evicts the oldest cached page once the LRU is full", () => {
    const { rerender } = render(<Harness active="a" />);
    rerender(<Harness active="b" />);
    rerender(<Harness active="c" />);
    expect(screen.queryByTestId("page-a")).toBeNull();
    expect(screen.getByTestId("page-b")).toBeInTheDocument();
    expect(screen.getByTestId("page-c")).toBeVisible();
  });

  it("does not cache a page that is not in the keep list", () => {
    const { rerender } = render(<Harness active="a" />);
    rerender(<Harness active="settings" />);
    expect(screen.getByText("settings")).toBeVisible();
    expect(screen.getByTestId("page-a")).not.toBeVisible();
    rerender(<Harness active="b" />);
    expect(screen.queryByText("settings")).toBeNull();
  });
});
