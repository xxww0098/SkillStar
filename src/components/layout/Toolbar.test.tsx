import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Toolbar } from "./Toolbar";

describe("Toolbar update filter and action", () => {
  const defaultProps = {
    searchQuery: "",
    onSearchChange: vi.fn(),
    sortBy: "stars-desc" as const,
    onSortChange: vi.fn(),
    viewMode: "grid" as const,
    onViewModeChange: vi.fn(),
  };

  it("does not render updates group when pendingUpdateCount is 0 and onlyUpdatesFilter is false", () => {
    render(<Toolbar {...defaultProps} pendingUpdateCount={0} onlyUpdatesFilter={false} />);
    expect(screen.queryByRole("button", { name: /可更新/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/全部更新/i)).not.toBeInTheDocument();
  });

  it("renders dual update filter and update-all action when pendingUpdateCount > 0", () => {
    const onFilterChange = vi.fn();
    const onUpdateAll = vi.fn();

    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        onlyUpdatesFilter={false}
        onOnlyUpdatesFilterChange={onFilterChange}
        onUpdateAll={onUpdateAll}
      />,
    );

    const filterBtn = screen.getByRole("button", { name: /可更新 \(3\)/i });
    expect(filterBtn).toBeInTheDocument();
    expect(filterBtn).toHaveAttribute("aria-pressed", "false");

    const updateAllBtn = screen.getByRole("button", { name: /全部更新/i });
    expect(updateAllBtn).toBeInTheDocument();

    // Clicking filter toggles updatesOnly filter
    fireEvent.click(filterBtn);
    expect(onFilterChange).toHaveBeenCalledWith(true);

    // Clicking updateAll triggers onUpdateAll
    fireEvent.click(updateAllBtn);
    expect(onUpdateAll).toHaveBeenCalled();
  });

  it("counts the filtered updates on the chip while update-all stays global", () => {
    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        filteredUpdateCount={0}
        onlyUpdatesFilter={false}
        onOnlyUpdatesFilterChange={vi.fn()}
        onUpdateAll={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /可更新 \(0\)/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /全部更新/i })).toBeInTheDocument();
  });

  it("shows active state on filter button when onlyUpdatesFilter is true", () => {
    const onFilterChange = vi.fn();

    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        onlyUpdatesFilter={true}
        onOnlyUpdatesFilterChange={onFilterChange}
      />,
    );

    const filterBtn = screen.getByRole("button", { name: /可更新 \(3\)/i });
    expect(filterBtn).toBeInTheDocument();
    expect(filterBtn).toHaveAttribute("aria-pressed", "true");
    expect(filterBtn.className).toContain("bg-amber-500");

    fireEvent.click(filterBtn);
    expect(onFilterChange).toHaveBeenCalledWith(false);
  });
});
