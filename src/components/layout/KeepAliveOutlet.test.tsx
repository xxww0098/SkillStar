import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { McpServerForm } from "../../features/mcp/components/McpServerForm";
import i18n from "../../i18n";
import { ModalShell } from "../ui/ModalShell";
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
  it("deactivates a cached page's portal and preserves the MCP form's own draft on return", async () => {
    const onSubmit = vi.fn();
    function Editor() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Open editor
          </button>
          <ModalShell open={open} onClose={() => setOpen(false)} ariaLabel="Page editor">
            <McpServerForm onSubmit={onSubmit} submitLabel="Save draft" />
          </ModalShell>
        </>
      );
    }
    const page = (id: string) => (id === "a" ? <Editor /> : <button type="button">Other page</button>);
    const { rerender } = render(<KeepAliveOutlet active="a" keep={KEEP} render={page} />);
    const trigger = screen.getByRole("button", { name: "Open editor" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.change(screen.getByPlaceholderText(i18n.t("mcp.fieldNamePlaceholder")), {
      target: { value: "Unsaved draft" },
    });
    fireEvent.change(screen.getByPlaceholderText("npx"), { target: { value: "my-mcp-server" } });
    fireEvent.change(screen.getByPlaceholderText("API_KEY=sk-xxx"), { target: { value: "MODE=draft" } });

    rerender(<KeepAliveOutlet active="b" keep={KEEP} render={page} />);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Page editor" })).not.toBeInTheDocument());
    const otherPage = screen.getByRole("button", { name: "Other page" });
    otherPage.focus();
    expect(otherPage).toHaveFocus();
    expect(document.body.style.pointerEvents).not.toBe("none");

    rerender(<KeepAliveOutlet active="a" keep={KEEP} render={page} />);
    await screen.findByRole("dialog", { name: "Page editor" });
    const draft = screen.getByPlaceholderText(i18n.t("mcp.fieldNamePlaceholder"));
    expect(draft).toHaveValue("Unsaved draft");
    expect(draft).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Save draft" }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Unsaved draft",
        command: "my-mcp-server",
        env: { MODE: "draft" },
      }),
    );
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
