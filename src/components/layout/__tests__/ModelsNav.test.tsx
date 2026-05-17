import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat, ToolActivationsMap } from "../../../types";

// Linearize the Radix ContextMenu + AlertDialog so the delete flow is
// trivially testable. Each provider gets an explicit "trigger-delete-${id}"
// button that opens the dialog; the dialog exposes a single "confirm" button.
vi.mock("@/features/models/components/ProviderContextMenu", () => ({
  ProviderContextMenu: ({
    children,
    provider,
    onDelete,
  }: {
    children: React.ReactNode;
    provider: { id: string };
    onDelete: () => void;
  }) => (
    <div data-testid={`provider-row-${provider.id}`}>
      {children}
      <button type="button" data-testid={`trigger-delete-${provider.id}`} onClick={onDelete}>
        delete
      </button>
    </div>
  ),
  DeleteConfirmDialog: ({ open, onConfirm }: { open: boolean; onConfirm: () => void }) =>
    open ? (
      <button type="button" data-testid="confirm-delete" onClick={onConfirm}>
        confirm
      </button>
    ) : null,
}));

vi.mock("@/features/models/components/ProviderBrandIcon", () => ({
  ProviderBrandIcon: () => <span data-testid="brand-icon" />,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// Mock framer-motion (Reorder components)
vi.mock("framer-motion", () => ({
  Reorder: {
    Group: ({ children, className }: { children: React.ReactNode; className?: string }) => (
      <div className={className} data-testid="reorder-group">
        {children}
      </div>
    ),
    Item: ({ children, className }: { children: React.ReactNode; className?: string }) => (
      <div className={className}>{children}</div>
    ),
  },
  useDragControls: () => ({ start: vi.fn() }),
}));

// Mock data for useProvidersFlat
let mockProviders: ProviderEntryFlat[] = [];
let mockToolActivations: ToolActivationsMap = {};
const mockReorderProviders = vi.fn();
const mockActivateTool = vi.fn();
const mockDeleteProvider = vi.fn();
const mockCreateProvider = vi.fn();

vi.mock("@/features/models/hooks", () => ({
  useProvidersFlat: () => ({
    providers: mockProviders,
    toolActivations: mockToolActivations,
    reorderProviders: mockReorderProviders,
    activateTool: mockActivateTool,
    deleteProvider: mockDeleteProvider,
    createProvider: mockCreateProvider,
  }),
  getProviderToolBadges: (providerId: string, activations: ToolActivationsMap) => {
    return Object.entries(activations)
      .filter(([, activation]) => activation?.provider_id === providerId)
      .map(([toolId]) => toolId);
  },
}));

import { ModelsNav } from "../ModelsNav";

const makeProvider = (overrides: Partial<ProviderEntryFlat> = {}): ProviderEntryFlat => ({
  id: "test-id-1",
  name: "DeepSeek",
  base_url_openai: "https://api.deepseek.com/v1",
  base_url_anthropic: "https://api.deepseek.com/anthropic",
  models_url: "https://api.deepseek.com/v1/models",
  api_key: "sk-test",
  models: ["deepseek-chat"],
  default_model: "deepseek-chat",
  sort_index: 0,
  icon_color: "#4D6BFE",
  ...overrides,
});

describe("ModelsNav", () => {
  const defaultProps = {
    selectedProviderId: null,
    onSelectProvider: vi.fn(),
    onAddProvider: vi.fn(),
    collapsed: false,
  };

  beforeEach(() => {
    mockProviders = [];
    mockToolActivations = {};
    mockReorderProviders.mockClear();
    mockActivateTool.mockClear();
    mockDeleteProvider.mockClear();
    mockDeleteProvider.mockResolvedValue(undefined);
    mockCreateProvider.mockClear();
  });

  describe("empty state", () => {
    it("shows empty state message when no providers are configured", () => {
      mockProviders = [];
      render(<ModelsNav {...defaultProps} />);

      expect(screen.getByText("尚未配置任何供应商")).toBeInTheDocument();
    });

    it("shows add button in empty state", () => {
      mockProviders = [];
      render(<ModelsNav {...defaultProps} />);

      expect(screen.getByText("新增供应商")).toBeInTheDocument();
    });

    it("calls onAddProvider when add button is clicked in empty state", () => {
      mockProviders = [];
      const onAddProvider = vi.fn();
      render(<ModelsNav {...defaultProps} onAddProvider={onAddProvider} />);

      fireEvent.click(screen.getByText("新增供应商"));
      expect(onAddProvider).toHaveBeenCalledTimes(1);
    });
  });

  describe("provider list", () => {
    it("renders provider names when providers exist", () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek" }),
        makeProvider({ id: "p2", name: "Kimi", sort_index: 1 }),
      ];
      render(<ModelsNav {...defaultProps} />);

      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
      expect(screen.getByText("Kimi")).toBeInTheDocument();
    });

    it("calls onSelectProvider when a provider is clicked", () => {
      mockProviders = [makeProvider({ id: "p1", name: "DeepSeek" })];
      const onSelectProvider = vi.fn();
      render(<ModelsNav {...defaultProps} onSelectProvider={onSelectProvider} />);

      fireEvent.click(screen.getByText("DeepSeek"));
      expect(onSelectProvider).toHaveBeenCalledWith("p1");
    });
  });

  describe("search filtering", () => {
    it("filters providers by search query (case-insensitive)", () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek" }),
        makeProvider({ id: "p2", name: "Kimi", sort_index: 1 }),
        makeProvider({ id: "p3", name: "OpenRouter", sort_index: 2 }),
      ];
      render(<ModelsNav {...defaultProps} />);

      const searchInput = screen.getByPlaceholderText("搜索供应商...");
      fireEvent.change(searchInput, { target: { value: "deep" } });

      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
      expect(screen.queryByText("Kimi")).not.toBeInTheDocument();
      expect(screen.queryByText("OpenRouter")).not.toBeInTheDocument();
    });

    it("shows all providers when search is empty", () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek" }),
        makeProvider({ id: "p2", name: "Kimi", sort_index: 1 }),
      ];
      render(<ModelsNav {...defaultProps} />);

      const searchInput = screen.getByPlaceholderText("搜索供应商...");
      fireEvent.change(searchInput, { target: { value: "" } });

      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
      expect(screen.getByText("Kimi")).toBeInTheDocument();
    });

    it("shows '无匹配结果' when search matches nothing", () => {
      mockProviders = [makeProvider({ id: "p1", name: "DeepSeek" })];
      render(<ModelsNav {...defaultProps} />);

      const searchInput = screen.getByPlaceholderText("搜索供应商...");
      fireEvent.change(searchInput, { target: { value: "nonexistent" } });

      expect(screen.getByText("无匹配结果")).toBeInTheDocument();
    });
  });

  describe("delete provider", () => {
    it("clears the persisted selection when the currently selected provider is deleted", async () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek" }),
        makeProvider({ id: "p2", name: "Kimi", sort_index: 1 }),
      ];
      const onClearSelection = vi.fn();

      render(<ModelsNav {...defaultProps} selectedProviderId="p1" onClearSelection={onClearSelection} />);

      fireEvent.click(screen.getByTestId("trigger-delete-p1"));
      fireEvent.click(await screen.findByTestId("confirm-delete"));

      await waitFor(() => {
        expect(mockDeleteProvider).toHaveBeenCalledWith("p1");
      });
      expect(onClearSelection).toHaveBeenCalledTimes(1);
    });

    it("keeps the selection when a non-selected provider is deleted", async () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek" }),
        makeProvider({ id: "p2", name: "Kimi", sort_index: 1 }),
      ];
      const onClearSelection = vi.fn();

      render(<ModelsNav {...defaultProps} selectedProviderId="p1" onClearSelection={onClearSelection} />);

      fireEvent.click(screen.getByTestId("trigger-delete-p2"));
      fireEvent.click(await screen.findByTestId("confirm-delete"));

      await waitFor(() => {
        expect(mockDeleteProvider).toHaveBeenCalledWith("p2");
      });
      expect(onClearSelection).not.toHaveBeenCalled();
    });
  });

  describe("collapsed mode", () => {
    it("shows only brand dots (no text) in collapsed mode", () => {
      mockProviders = [
        makeProvider({ id: "p1", name: "DeepSeek", icon_color: "#4D6BFE" }),
        makeProvider({ id: "p2", name: "Kimi", icon_color: "#FF6600", sort_index: 1 }),
      ];
      render(<ModelsNav {...defaultProps} collapsed={true} />);

      // Provider names should not be visible
      expect(screen.queryByText("DeepSeek")).not.toBeInTheDocument();
      expect(screen.queryByText("Kimi")).not.toBeInTheDocument();

      // Search input should not be visible
      expect(screen.queryByPlaceholderText("搜索供应商...")).not.toBeInTheDocument();
    });

    it("provider buttons have title attribute for tooltip in collapsed mode", () => {
      mockProviders = [makeProvider({ id: "p1", name: "DeepSeek" })];
      render(<ModelsNav {...defaultProps} collapsed={true} />);

      const button = screen.getByTitle("DeepSeek");
      expect(button).toBeInTheDocument();
    });
  });
});
