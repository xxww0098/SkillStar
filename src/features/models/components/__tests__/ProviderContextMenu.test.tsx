import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../../types";

// Mock sonner toast
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock Radix UI ContextMenu to make items directly testable
// In jsdom, right-click context menus with portals are not easily testable,
// so we render the menu content inline.
vi.mock("radix-ui", () => ({
  ContextMenu: {
    Root: ({ children }: { children: React.ReactNode }) => <div data-testid="context-menu-root">{children}</div>,
    Trigger: ({ children }: { children: React.ReactNode; asChild?: boolean }) => (
      <div data-testid="context-menu-trigger">{children}</div>
    ),
    Portal: ({ children }: { children: React.ReactNode }) => <div data-testid="context-menu-portal">{children}</div>,
    Content: ({ children }: { children: React.ReactNode; className?: string }) => (
      <div data-testid="context-menu-content" role="menu">
        {children}
      </div>
    ),
    Item: ({
      children,
      onSelect,
      disabled,
    }: {
      children: React.ReactNode;
      onSelect?: () => void;
      disabled?: boolean;
      className?: string;
    }) => (
      <button
        type="button"
        role="menuitem"
        onClick={onSelect}
        disabled={disabled}
        data-testid={`menu-item-${children}`}
      >
        {children}
      </button>
    ),
    Separator: () => <hr data-testid="context-menu-separator" />,
  },
  AlertDialog: {
    Root: ({ children, open }: { children: React.ReactNode; open?: boolean; onOpenChange?: (v: boolean) => void }) =>
      open ? <div data-testid="alert-dialog-root">{children}</div> : null,
    Portal: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
    Overlay: ({ className }: { className?: string }) => <div data-testid="alert-dialog-overlay" />,
    Content: ({ children }: { children: React.ReactNode; className?: string }) => (
      <div data-testid="alert-dialog-content">{children}</div>
    ),
    Title: ({ children }: { children: React.ReactNode; className?: string }) => <h2>{children}</h2>,
    Description: ({ children }: { children: React.ReactNode; className?: string }) => <p>{children}</p>,
    Cancel: ({ children, className }: { children: React.ReactNode; className?: string }) => (
      <button type="button" data-testid="alert-dialog-cancel">
        {children}
      </button>
    ),
    Action: ({
      children,
      onClick,
      className,
    }: {
      children: React.ReactNode;
      onClick?: () => void;
      className?: string;
    }) => (
      <button type="button" data-testid="alert-dialog-action" onClick={onClick}>
        {children}
      </button>
    ),
  },
}));

import { DeleteConfirmDialog, ProviderContextMenu } from "../ProviderContextMenu";

afterEach(cleanup);

const mockProvider: ProviderEntryFlat = {
  id: "provider-1",
  name: "DeepSeek",
  base_url_openai: "https://api.deepseek.com/v1",
  base_url_anthropic: "https://api.deepseek.com/anthropic",
  models_url: "https://api.deepseek.com/v1/models",
  api_key: "sk-test",
  models: ["deepseek-chat"],
  default_model: "deepseek-chat",
  sort_index: 0,
  icon_color: "#4D6BFE",
};

describe("ProviderContextMenu", () => {
  const mockOnActivate = vi.fn();
  const mockOnActivateAll = vi.fn();
  const mockOnDuplicate = vi.fn();
  const mockOnDelete = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockOnActivate.mockResolvedValue(undefined);
    mockOnActivateAll.mockResolvedValue(undefined);
    mockOnDuplicate.mockResolvedValue(undefined);
  });

  const renderMenu = () =>
    render(
      <ProviderContextMenu
        provider={mockProvider}
        onActivate={mockOnActivate}
        onActivateAll={mockOnActivateAll}
        onDuplicate={mockOnDuplicate}
        onDelete={mockOnDelete}
      >
        <div>Provider Item</div>
      </ProviderContextMenu>,
    );

  // ── Menu options display (Requirement 5.1) ──

  describe("context menu options", () => {
    it("renders all context menu options", () => {
      renderMenu();

      expect(screen.getByText("应用到 Claude Code")).toBeInTheDocument();
      expect(screen.getByText("应用到 Codex")).toBeInTheDocument();
      expect(screen.getByText("应用到全部")).toBeInTheDocument();
      expect(screen.getByText("复制")).toBeInTheDocument();
      expect(screen.getByText("删除")).toBeInTheDocument();
    });

    it("renders the trigger children", () => {
      renderMenu();

      expect(screen.getByText("Provider Item")).toBeInTheDocument();
    });
  });

  // ── Activation flow ──

  describe("activation", () => {
    it("calls onActivate with 'claude-code' when '应用到 Claude Code' is clicked", async () => {
      renderMenu();

      fireEvent.click(screen.getByText("应用到 Claude Code"));

      await waitFor(() => {
        expect(mockOnActivate).toHaveBeenCalledWith("claude-code");
      });
    });

    it("calls onActivate with 'codex' when '应用到 Codex' is clicked", async () => {
      renderMenu();

      fireEvent.click(screen.getByText("应用到 Codex"));

      await waitFor(() => {
        expect(mockOnActivate).toHaveBeenCalledWith("codex");
      });
    });

    it("calls onActivateAll when '应用到全部' is clicked", async () => {
      renderMenu();

      fireEvent.click(screen.getByText("应用到全部"));

      await waitFor(() => {
        expect(mockOnActivateAll).toHaveBeenCalled();
      });
    });

    it("calls onDuplicate when '复制' is clicked", async () => {
      renderMenu();

      fireEvent.click(screen.getByText("复制"));

      await waitFor(() => {
        expect(mockOnDuplicate).toHaveBeenCalled();
      });
    });
  });

  // ── Delete flow ──

  describe("delete", () => {
    it("calls onDelete when '删除' is clicked", () => {
      renderMenu();

      fireEvent.click(screen.getByText("删除"));

      expect(mockOnDelete).toHaveBeenCalled();
    });
  });
});

// ── DeleteConfirmDialog tests ──

describe("DeleteConfirmDialog", () => {
  const mockOnOpenChange = vi.fn();
  const mockOnConfirm = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows affected tools when dialog is open", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        providerName="DeepSeek"
        affectedTools={["claude-code", "codex"]}
        onConfirm={mockOnConfirm}
      />,
    );

    expect(screen.getByText("确认删除供应商")).toBeInTheDocument();
    expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
  });

  it("does not render when open is false", () => {
    render(
      <DeleteConfirmDialog
        open={false}
        onOpenChange={mockOnOpenChange}
        providerName="DeepSeek"
        affectedTools={["claude-code"]}
        onConfirm={mockOnConfirm}
      />,
    );

    expect(screen.queryByText("确认删除供应商")).not.toBeInTheDocument();
  });

  it("shows message about restoring backup state", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        providerName="TestProvider"
        affectedTools={["claude-code"]}
        onConfirm={mockOnConfirm}
      />,
    );

    expect(screen.getByText(/删除后以下工具将恢复到启用前的状态/)).toBeInTheDocument();
  });

  it("calls onConfirm when delete button is clicked", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        providerName="DeepSeek"
        affectedTools={["claude-code"]}
        onConfirm={mockOnConfirm}
      />,
    );

    fireEvent.click(screen.getByTestId("alert-dialog-action"));

    expect(mockOnConfirm).toHaveBeenCalled();
  });

  it("shows cancel button that closes the dialog", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        providerName="DeepSeek"
        affectedTools={[]}
        onConfirm={mockOnConfirm}
      />,
    );

    expect(screen.getByText("取消")).toBeInTheDocument();
  });

  it("does not show affected tools section when no tools are affected", () => {
    render(
      <DeleteConfirmDialog
        open={true}
        onOpenChange={mockOnOpenChange}
        providerName="DeepSeek"
        affectedTools={[]}
        onConfirm={mockOnConfirm}
      />,
    );

    expect(screen.queryByText(/删除后以下工具将恢复到启用前的状态/)).not.toBeInTheDocument();
  });
});
