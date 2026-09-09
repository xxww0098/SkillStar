import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import type { ProviderEntryFlat, ToolActivationsMap } from "@/types";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { ModelsHub } from "./ModelsHub";

const store = vi.hoisted(() => ({
  providers: [] as ProviderEntryFlat[],
  toolActivations: {} as ToolActivationsMap,
  isLoading: false,
  error: null as Error | null,
  refresh: vi.fn(),
  activateTool: vi.fn(),
  deactivateTool: vi.fn(),
  removeBindingEntry: vi.fn(),
  createProvider: vi.fn(),
  deleteProvider: vi.fn(),
}));
vi.mock("../../hooks/useProvidersFlat", async (original) => ({
  ...(await original<typeof import("../../hooks/useProvidersFlat")>()),
  useProvidersFlat: () => store,
}));
vi.mock("./claude/ClaudeMappingPanel", () => ({
  ClaudeMappingPanel: ({
    provider,
    onBusyChange,
  }: {
    provider: ProviderEntryFlat;
    onBusyChange: (busy: boolean) => void;
  }) => (
    <div data-testid="mapping">
      Mapping {provider.name}
      <button onClick={() => onBusyChange(true)}>Start role write</button>
      <button onClick={() => onBusyChange(false)}>Finish role write</button>
    </div>
  ),
}));
vi.mock("./claude/ClaudeProviderCreate", () => ({
  ClaudeProviderCreate: ({
    onCreated,
    onClose,
  }: {
    onCreated: (p: ProviderEntryFlat) => void;
    onClose: () => void;
  }) => (
    <div>
      <button
        onClick={() => {
          const p = { ...store.providers[0], id: "created", name: "Created" };
          store.providers = [...store.providers, p];
          onCreated(p);
        }}
      >
        Complete creation
      </button>
      <button onClick={onClose}>Cancel creation</button>
    </div>
  ),
}));
vi.mock("../provider/ProviderEditorDrawer", () => ({
  ProviderEditorDrawer: ({
    provider,
    onDelete,
    onDuplicate,
    onClose,
  }: {
    provider: ProviderEntryFlat;
    onDelete: (p: ProviderEntryFlat) => void;
    onDuplicate: (p: ProviderEntryFlat) => void;
    onClose: () => void;
  }) => (
    <div role="dialog" aria-label={provider.name}>
      <button onClick={() => onDelete(provider)}>Delete provider</button>
      <button onClick={() => onDuplicate(provider)}>Duplicate provider</button>
      <button onClick={onClose}>Close editor</button>
    </div>
  ),
}));
const text = (key: string) => i18n.t("models.claudeWorkbench." + key);
function navigation(): ModelsNavBridge {
  return {
    selectedProviderId: null,
    setSelectedProviderId: vi.fn(),
    modelsDrawerRequest: null,
    clearModelsDrawerRequest: vi.fn(),
  };
}
function chooseApi() {
  fireEvent.click(screen.getByRole("radio", { name: text("api") }));
}
function switchDesktop() {
  fireEvent.mouseDown(screen.getByRole("tab", { name: "Claude Code Desktop" }), { button: 0, ctrlKey: false });
}
beforeEach(() => {
  vi.clearAllMocks();
  store.error = null;
  store.isLoading = false;
  store.providers = [
    {
      id: "relay",
      name: "Relay",
      base_url_openai: "https://relay.example/v1",
      base_url_anthropic: "https://relay.example/anthropic",
      api_key: "test-secret",
      models_url: "",
      models: ["test-model"],
      default_model: "test-model",
      sort_index: 0,
    },
    {
      id: "legacy",
      name: "Legacy",
      base_url_openai: "https://legacy.example",
      base_url_anthropic: "",
      api_key: "",
      models_url: "",
      models: [],
      default_model: "",
      sort_index: 1,
    },
  ];
  store.toolActivations = {
    codex: { entries: [{ provider_id: "relay", model: "test-model" }], active_index: 0 },
    omp: { entries: [{ provider_id: "relay", model: "test-model" }], active_index: 0 },
  };
  store.activateTool.mockImplementation(async (providerId: string, toolId: string, model: string) => {
    store.toolActivations = {
      ...store.toolActivations,
      [toolId]: { entries: [{ provider_id: providerId, model }], active_index: 0 },
    };
    return { success: true };
  });
});

describe("Claude workbench", () => {
  it("allows reapplying a saved binding after a failed write and remount", async () => {
    store.toolActivations["claude-code"] = {
      entries: [{ provider_id: "relay", model: "test-model" }],
      active_index: 0,
    };
    const first = render(<ModelsHub {...navigation()} />);
    expect(screen.getByRole("button", { name: text("apply") })).toBeEnabled();
    first.unmount();
    render(<ModelsHub {...navigation()} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: text("apply") })));
    expect(store.activateTool).toHaveBeenCalledWith("relay", "claude-code", "test-model");
  });
  it("locks source, client, and provider edits during a role write without unmounting mappings", () => {
    store.toolActivations["claude-code"] = {
      entries: [{ provider_id: "relay", model: "test-model" }],
      active_index: 0,
    };
    render(<ModelsHub {...navigation()} />);
    fireEvent.click(screen.getByRole("button", { name: "Start role write" }));
    expect(screen.getByRole("combobox")).toBeDisabled();
    expect(screen.getByRole("tab", { name: "Claude Code Desktop" })).toBeDisabled();
    expect(within(screen.getByText("Relay", { selector: "li p" }).closest("li")!).getByRole("button")).toBeDisabled();
    expect(screen.getByTestId("mapping")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Finish role write" }));
    expect(screen.getByRole("combobox")).toBeEnabled();
  });
  it("defers sidebar navigation until a role draft or write is released", () => {
    store.toolActivations["claude-code"] = {
      entries: [{ provider_id: "relay", model: "test-model" }],
      active_index: 0,
    };
    const nav = navigation();
    const view = render(<ModelsHub {...nav} />);
    fireEvent.click(screen.getByRole("button", { name: "Start role write" }));
    nav.modelsDrawerRequest = { kind: "create", nonce: 1 };
    view.rerender(<ModelsHub {...nav} />);
    expect(screen.queryByRole("button", { name: "Complete creation" })).not.toBeInTheDocument();
    expect(nav.clearModelsDrawerRequest).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Finish role write" }));
    expect(screen.getByRole("button", { name: "Complete creation" })).toBeVisible();
    expect(nav.clearModelsDrawerRequest).toHaveBeenCalledOnce();
  });

  it("offers a read retry without exposing writable controls on failure", () => {
    store.error = new Error("Store unreadable");
    render(<ModelsHub {...navigation()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Store unreadable");
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: text("retryLoad") }));
    expect(store.refresh).toHaveBeenCalledOnce();
  });
  it("shows every affected agent before deleting, including hidden inactive entries", async () => {
    store.toolActivations["hidden-agent"] = {
      entries: [
        { provider_id: "legacy", model: "" },
        { provider_id: "relay", model: "" },
      ],
      active_index: 0,
    };
    render(<ModelsHub {...navigation()} />);
    fireEvent.click(within(screen.getByText("Relay", { selector: "p" }).closest("li")!).getByRole("button"));
    fireEvent.click(screen.getByRole("button", { name: "Delete provider" }));
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("Codex");
    expect(dialog).toHaveTextContent("Oh My Pi");
    expect(dialog).toHaveTextContent("hidden-agent");
    expect(store.deleteProvider).not.toHaveBeenCalled();
    await act(async () =>
      fireEvent.click(within(dialog).getByRole("button", { name: i18n.t("models.deleteDialog.confirm") })),
    );
    expect(store.deleteProvider).toHaveBeenCalledExactlyOnceWith("relay");
  });
  it("opens creation from navigation and edits the newly created provider without binding", () => {
    const nav = navigation();
    nav.modelsDrawerRequest = { kind: "create", nonce: 1 };
    render(<ModelsHub {...nav} />);
    fireEvent.click(screen.getByRole("button", { name: "Complete creation" }));
    expect(screen.getByRole("dialog", { name: "Created" })).toBeVisible();
    expect(nav.setSelectedProviderId).toHaveBeenCalledWith("created");
    expect(nav.clearModelsDrawerRequest).toHaveBeenCalled();
    expect(store.activateTool).not.toHaveBeenCalled();
  });
  it("keeps incompatible providers editable but never applicable, and accepts backend-resolved credentials", () => {
    store.providers[0].api_key = "";
    render(<ModelsHub {...navigation()} />);
    chooseApi();
    expect(screen.getByRole("button", { name: text("apply") })).toBeEnabled();
    expect(screen.getByText(text("credentialDescription"))).toBeVisible();
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "legacy" } });
    expect(screen.getByRole("button", { name: text("apply") })).toBeDisabled();
    expect(screen.getByText(text("incompatibleDescription"))).toBeVisible();
    fireEvent.click(within(screen.getByText("Legacy", { selector: "p" }).closest("li")!).getByRole("button"));
    expect(screen.getByRole("dialog", { name: "Legacy" })).toBeVisible();
    expect(store.activateTool).not.toHaveBeenCalled();
  });
  it("uses the actual official seed and explains that native mode does not log in", async () => {
    store.providers.push({
      ...store.providers[0],
      id: "actual-native-id",
      preset_id: "claude-official",
      name: "My Claude login",
    });
    const nav = navigation();
    const view = render(<ModelsHub {...nav} />);
    expect(screen.getByText(text("nativeDescription"))).toBeVisible();
    expect(screen.getByText(text("unmanaged"))).toBeVisible();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: text("apply") })));
    view.rerender(<ModelsHub {...nav} />);
    expect(store.activateTool).toHaveBeenCalledExactlyOnceWith("actual-native-id", "claude-code", "");
    expect(screen.getByText(text("nativeSource"))).toBeVisible();
    expect(screen.queryByTestId("mapping")).not.toBeInTheDocument();
  });
  it("blocks repeat writes and source/client switching while applying", async () => {
    let finish!: (result: { success: boolean }) => void;
    store.activateTool.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    render(<ModelsHub {...navigation()} />);
    chooseApi();
    const apply = screen.getByRole("button", { name: text("apply") });
    fireEvent.click(apply);
    fireEvent.click(apply);
    expect(apply).toBeDisabled();
    expect(screen.getByRole("combobox")).toBeDisabled();
    expect(screen.getByRole("tab", { name: "Claude Code Desktop" })).toBeDisabled();
    expect(store.activateTool).toHaveBeenCalledTimes(1);
    await act(async () => finish({ success: true }));
  });
  it.each([
    "result",
    "reject",
  ])("keeps %s failures visible despite optimistic bindings and allows retry", async (failure) => {
    const nav = navigation();
    const view = render(<ModelsHub {...nav} />);
    chooseApi();
    store.activateTool.mockImplementationOnce(async () => {
      store.toolActivations["claude-code"] = {
        entries: [{ provider_id: "relay", model: "test-model" }],
        active_index: 0,
      };
      if (failure === "reject") throw new Error("Disk denied");
      return { success: false, error: "Disk denied" };
    });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: text("apply") })));
    view.rerender(<ModelsHub {...nav} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Disk denied");
    expect(screen.getByRole("status")).toHaveTextContent(text("notApplied"));
    expect(screen.queryByTestId("mapping")).not.toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: text("retryApply") })));
    view.rerender(<ModelsHub {...nav} />);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByTestId("mapping")).toHaveTextContent("Relay");
  });
  it("previews without writing, applies explicitly, and preserves hidden bindings", async () => {
    const nav = navigation();
    const view = render(<ModelsHub {...nav} />);
    const hidden = structuredClone(store.toolActivations);
    chooseApi();
    expect(screen.getByRole("status")).toHaveTextContent(text("notApplied"));
    expect(screen.queryByTestId("mapping")).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("test-secret");
    expect(store.activateTool).not.toHaveBeenCalled();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: text("apply") })));
    view.rerender(<ModelsHub {...nav} />);
    expect(store.activateTool).toHaveBeenCalledExactlyOnceWith("relay", "claude-code", "test-model");
    expect(screen.getByTestId("mapping")).toHaveTextContent("Relay");
    expect(screen.getByRole("status")).toHaveTextContent(text("applied"));
    expect(store.toolActivations.codex).toEqual(hidden.codex);
    expect(store.toolActivations.omp).toEqual(hidden.omp);
    expect(store.removeBindingEntry).not.toHaveBeenCalled();
    expect(store.deleteProvider).not.toHaveBeenCalled();
  });
  it("keeps Desktop read-only even with a legacy binding", () => {
    store.toolActivations["claude-desktop"] = { entries: [{ provider_id: "relay", model: "old" }], active_index: 0 };
    render(<ModelsHub {...navigation()} />);
    switchDesktop();
    expect(screen.getByText(text("desktopDescription"))).toBeVisible();
    expect(screen.getByText(text("desktopLegacy"))).toBeVisible();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByTestId("mapping")).not.toBeInTheDocument();
    expect(store.activateTool).not.toHaveBeenCalled();
    expect(store.deactivateTool).not.toHaveBeenCalled();
  });
});
