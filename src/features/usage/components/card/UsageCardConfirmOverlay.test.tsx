import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UsageCardConfirmOverlay } from "./UsageCardConfirmOverlay";

describe("UsageCardConfirmOverlay", () => {
  it("exposes an alertdialog named by the title", () => {
    render(
      <UsageCardConfirmOverlay
        title="删除此订阅？"
        message="会移除账号凭证。"
        confirmLabel="删除"
        cancelLabel="取消"
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveAccessibleName("删除此订阅？");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByText("会移除账号凭证。")).toBeInTheDocument();
  });

  it("cancels on Escape and on backdrop click", () => {
    const onCancel = vi.fn();
    const { container } = render(
      <UsageCardConfirmOverlay
        title="确认重置额度？"
        message="会消耗 1 次重置额度。"
        confirmLabel="重置额度"
        cancelLabel="取消"
        confirmVariant="default"
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);

    fireEvent.click(container.firstChild as HTMLElement);
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("keeps confirm from dismissing the sheet when clicking inside the dialog", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <UsageCardConfirmOverlay
        title="删除此订阅？"
        message="会移除账号凭证。"
        confirmLabel="删除"
        cancelLabel="取消"
        onCancel={onCancel}
        onConfirm={onConfirm}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });
});
