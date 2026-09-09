import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { useKeyboardShortcuts } from "../../hooks/useKeyboardShortcuts";
import { ModalShell } from "./ModalShell";

describe("ModalShell", () => {
  it.each([
    "metaKey",
    "ctrlKey",
  ])("defers %s+K to the modal and restores the command shortcut after closing", async (modifier) => {
    const onToggleCommandPalette = vi.fn();
    function Workspace({ open }: { open: boolean }) {
      useKeyboardShortcuts({ onNavigate: vi.fn(), onToggleCommandPalette });
      return (
        <>
          <input aria-label="Workspace input" />
          <ModalShell open={open} onClose={vi.fn()} ariaLabel="Edit">
            <input aria-label="Modal input" />
          </ModalShell>
        </>
      );
    }
    const { rerender } = render(<Workspace open />);
    const input = screen.getByRole("textbox", { name: "Modal input" });
    fireEvent.keyDown(input, { key: "k", [modifier]: true });
    expect(onToggleCommandPalette).not.toHaveBeenCalled();
    expect(input).toHaveFocus();

    rerender(<Workspace open={false} />);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Edit" })).not.toBeInTheDocument());
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Workspace input" }), { key: "k", [modifier]: true });
    expect(onToggleCommandPalette).toHaveBeenCalledTimes(1);
  });

  it("dismisses only the top modal when Escape is pressed", async () => {
    function Flow() {
      const [installOpen, setInstallOpen] = useState(true);
      const [accountOpen, setAccountOpen] = useState(false);
      return (
        <>
          <ModalShell open={installOpen} onClose={() => setInstallOpen(false)} ariaLabel="Install">
            <button type="button" onClick={() => setAccountOpen(true)}>
              Sign in
            </button>
          </ModalShell>
          <ModalShell open={accountOpen} onClose={() => setAccountOpen(false)} ariaLabel="Account">
            <p>Account settings</p>
          </ModalShell>
        </>
      );
    }

    render(<Flow />);
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    fireEvent.keyDown(screen.getByRole("dialog", { name: "Account" }), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Account" })).not.toBeInTheDocument());
    expect(screen.getByRole("dialog", { name: "Install" })).toBeInTheDocument();
  });

  it("closes on Escape when open and dismissable", () => {
    const onClose = vi.fn();
    render(
      <ModalShell open onClose={onClose} ariaLabel="test">
        <p>content</p>
      </ModalShell>,
    );
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("ignores Escape when dismissable is false (mutation in flight)", () => {
    const onClose = vi.fn();
    render(
      <ModalShell open onClose={onClose} ariaLabel="test" dismissable={false}>
        <p>content</p>
      </ModalShell>,
    );
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does not leak Escape to page shortcuts while dismissal is blocked", () => {
    const onClose = vi.fn();
    const onPageKey = vi.fn();
    window.addEventListener("keydown", onPageKey);
    try {
      render(
        <ModalShell open onClose={onClose} ariaLabel="Saving" dismissable={false}>
          <p>Saving</p>
        </ModalShell>,
      );
      fireEvent.keyDown(screen.getByRole("dialog", { name: "Saving" }), { key: "Escape" });
      expect(onClose).not.toHaveBeenCalled();
      expect(onPageKey).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("keydown", onPageKey);
    }
  });

  it("keeps focus inside the modal and restores the opening control", async () => {
    function Editor() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Edit
          </button>
          <ModalShell open={open} onClose={() => setOpen(false)} ariaLabel="Editor">
            <input aria-label="Name" />
          </ModalShell>
        </>
      );
    }
    render(<Editor />);
    const trigger = screen.getByRole("button", { name: "Edit" });
    trigger.focus();
    fireEvent.click(trigger);
    const input = screen.getByRole("textbox", { name: "Name" });
    expect(input).toHaveFocus();
    trigger.focus();
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("does not listen when closed", () => {
    const onClose = vi.fn();
    render(
      <ModalShell open={false} onClose={onClose} ariaLabel="test">
        <p>content</p>
      </ModalShell>,
    );
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });
});
