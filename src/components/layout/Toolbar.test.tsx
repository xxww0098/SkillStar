import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Toolbar } from "./Toolbar";

describe("Toolbar attention filter and update-all action", () => {
  const defaultProps = {
    searchQuery: "",
    onSearchChange: vi.fn(),
    sortBy: "stars-desc" as const,
    onSortChange: vi.fn(),
    viewMode: "grid" as const,
    onViewModeChange: vi.fn(),
  };

  it("does not render the attention group when nothing needs attention", () => {
    render(<Toolbar {...defaultProps} pendingUpdateCount={0} attentionCount={0} onlyUpdatesFilter={false} />);
    expect(screen.queryByRole("button", { name: /需处理/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/全部更新/i)).not.toBeInTheDocument();
  });

  it("renders dual attention filter and update-all action when updates exist", () => {
    const onFilterChange = vi.fn();
    const onUpdateAll = vi.fn();

    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        attentionCount={3}
        onlyUpdatesFilter={false}
        onOnlyUpdatesFilterChange={onFilterChange}
        onUpdateAll={onUpdateAll}
      />,
    );

    const filterBtn = screen.getByRole("button", { name: /需处理 \(3\)/i });
    expect(filterBtn).toBeInTheDocument();
    expect(filterBtn).toHaveAttribute("aria-pressed", "false");

    const updateAllBtn = screen.getByRole("button", { name: /全部更新/i });
    expect(updateAllBtn).toBeInTheDocument();

    // Clicking filter toggles attention-only filter
    fireEvent.click(filterBtn);
    expect(onFilterChange).toHaveBeenCalledWith(true);

    // Clicking updateAll triggers onUpdateAll
    fireEvent.click(updateAllBtn);
    expect(onUpdateAll).toHaveBeenCalled();
  });

  it("shows the attention group without the CTA when only upstream changes exist", () => {
    // The regression this guards: the sidebar badge counts removed / renamed
    // skills, so the filter group must stay visible and promise that count
    // even when nothing is content-updatable.
    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={0}
        attentionCount={1}
        onlyUpdatesFilter={false}
        onOnlyUpdatesFilterChange={vi.fn()}
        onUpdateAll={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /需处理 \(1\)/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /全部更新/i })).not.toBeInTheDocument();
  });

  it("counts the filtered attention skills on the chip while update-all stays global", () => {
    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        attentionCount={3}
        filteredAttentionCount={0}
        onlyUpdatesFilter={false}
        onOnlyUpdatesFilterChange={vi.fn()}
        onUpdateAll={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /需处理 \(0\)/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /全部更新/i })).toBeInTheDocument();
  });

  it("shows active state on filter button when onlyUpdatesFilter is true", () => {
    const onFilterChange = vi.fn();

    render(
      <Toolbar
        {...defaultProps}
        pendingUpdateCount={3}
        attentionCount={3}
        onlyUpdatesFilter={true}
        onOnlyUpdatesFilterChange={onFilterChange}
      />,
    );

    const filterBtn = screen.getByRole("button", { name: /需处理 \(3\)/i });
    expect(filterBtn).toBeInTheDocument();
    expect(filterBtn).toHaveAttribute("aria-pressed", "true");
    expect(filterBtn.className).toContain("bg-amber-500");

    fireEvent.click(filterBtn);
    expect(onFilterChange).toHaveBeenCalledWith(false);
  });
});
