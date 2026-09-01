import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Subscription } from "../../types";
import { UsageCardFooter } from "./UsageCardFooter";

const subscription = {
  id: "sub-xai",
  catalog_id: "xai",
  display_name: "Grok · account@example.com",
  currency: "USD",
} as Subscription;

function renderFooter(resetCreditsRemaining: number | null) {
  return render(
    <UsageCardFooter
      subscription={subscription}
      monthlyCost={null}
      showRenewFooter={false}
      renewDays={null}
      onRefresh={vi.fn()}
      onResetQuota={vi.fn()}
      resetCreditsRemaining={resetCreditsRemaining}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
    />,
  );
}

describe("UsageCardFooter Grok reset credits", () => {
  it("shows the authoritative remaining reset count beside the action", () => {
    renderFooter(2);

    expect(screen.getByText("剩余 2 次")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重置额度，剩余 2 次" })).toBeEnabled();
  });

  it("keeps zero visible and prevents a request when no credit remains", () => {
    renderFooter(0);

    expect(screen.getByText("剩余 0 次")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "暂无可用重置次数，剩余 0 次" })).toBeDisabled();
  });
});

describe("UsageCardFooter confirms", () => {
  it("opens a named alertdialog for delete and closes it on Escape", () => {
    const onDelete = vi.fn();
    render(
      <UsageCardFooter
        subscription={subscription}
        monthlyCost={null}
        showRenewFooter={false}
        renewDays={null}
        onRefresh={vi.fn()}
        onEdit={vi.fn()}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    expect(screen.getByRole("alertdialog")).toHaveAccessibleName(/删除/);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(onDelete).not.toHaveBeenCalled();
  });

  it("names icon-only actions for keyboard and screen readers", () => {
    renderFooter(null);
    expect(screen.getByRole("button", { name: "编辑" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /同步用量/ })).toBeInTheDocument();
  });
});
