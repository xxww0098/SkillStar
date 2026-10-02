import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ModalShell } from "../ui/ModalShell";
import { KeepAliveOutlet } from "./KeepAliveOutlet";

const KEEP = ["a", "b", "c"] as const;

/** A form that owns its draft in local state, like the real page editors. */
function DraftForm({ onSubmit }: { onSubmit: (name: string) => void }) {
  const [name, setName] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(name);
      }}
    >
      <input placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
      <button type="submit">Save draft</button>
    </form>
  );
}

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
  it("deactivates a cached page's portal and preserves the form's own draft on return", async () => {
    const onSubmit = vi.fn();
    function Editor() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Open editor
          </button>
          <ModalShell open={open} onClose={() => setOpen(false)} ariaLabel="Page editor">
            <DraftForm onSubmit={onSubmit} />
          </ModalShell>
        </>
      );
    }
    const page = (id: string) => (id === "a" ? <Editor /> : <button type="button">Other page</button>);
    const { rerender } = render(<KeepAliveOutlet active="a" keep={KEEP} render={page} />);
    const trigger = screen.getByRole("button", { name: "Open editor" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.change(screen.getByPlaceholderText("Name"), { target: { value: "Unsaved draft" } });

    rerender(<KeepAliveOutlet active="b" keep={KEEP} render={page} />);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Page editor" })).not.toBeInTheDocument());
    const otherPage = screen.getByRole("button", { name: "Other page" });
    otherPage.focus();
    expect(otherPage).toHaveFocus();
    expect(document.body.style.pointerEvents).not.toBe("none");

    rerender(<KeepAliveOutlet active="a" keep={KEEP} render={page} />);
    await screen.findByRole("dialog", { name: "Page editor" });
    const draft = screen.getByPlaceholderText("Name");
    expect(draft).toHaveValue("Unsaved draft");
    expect(draft).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Save draft" }));
    expect(onSubmit).toHaveBeenCalledWith("Unsaved draft");
    fireEvent.keyDown(draft, { key: "Escape" });
    await waitFor(() => expect(trigger).toHaveFocus());
  });

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
