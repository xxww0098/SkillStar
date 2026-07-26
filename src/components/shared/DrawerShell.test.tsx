import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DrawerShell } from "./DrawerShell";

describe("DrawerShell", () => {
  it("closes when the left scrim overlay is clicked", () => {
    const onOpenChange = vi.fn();

    const { baseElement } = render(
      <DrawerShell open onOpenChange={onOpenChange} title="Test drawer">
        <p>Body</p>
      </DrawerShell>,
    );

    // Portal lands on document.body; overlay is the fixed full-viewport scrim.
    const overlay = baseElement.ownerDocument.body.querySelector<HTMLElement>(".fixed.inset-0");
    expect(overlay).toBeTruthy();
    fireEvent.pointerDown(overlay as HTMLElement);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("closes when the header close button is clicked", () => {
    const onOpenChange = vi.fn();

    render(
      <DrawerShell open onOpenChange={onOpenChange} title="Test drawer">
        <p>Body</p>
      </DrawerShell>,
    );

    // i18n setup defaults to zh-CN → models.drawer.close = "关闭"
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
