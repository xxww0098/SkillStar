/** Renderer contract for update_agent_settings. Disk projection is covered by
 * tool_sync::tests::roles::claude_role_mapping_lands_in_the_env_block. */
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { toast } from "sonner";
import i18n from "../../../../../i18n";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ModelCatalogFetchResult,
  ProviderEntryFlat,
  RoleTarget,
  ToolBinding,
  ToolSyncResult,
} from "../../../../../types";
import type { useModelFetch } from "../../../api/modelCatalog";
import type { useProvidersFlat } from "../../../hooks/useProvidersFlat";
import { ClaudeMappingPanel } from "./ClaudeMappingPanel";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), message: vi.fn(), warning: vi.fn() } }));

const updateToolBindingSettings = vi.fn<ReturnType<typeof useProvidersFlat>["updateToolBindingSettings"]>();
const updateProvider = vi.fn<ReturnType<typeof useProvidersFlat>["updateProvider"]>();
const fetchModelCatalog = vi.fn<ReturnType<typeof useModelFetch>["fetchModelCatalog"]>();

vi.mock("../../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({ updateToolBindingSettings, updateProvider }),
}));

vi.mock("../../../api/modelCatalog", () => ({
  useModelFetch: () => ({ fetchModelCatalog, isLoading: false }),
}));

const descriptor = {
  id: "claude-code",
  display_name: "Claude Code",
  kind: "single" as const,
  required_wire: "anthropic_messages" as const,
  roles: [
    { id: "default", agent_key: "ANTHROPIC_MODEL", primary: true, inherits: null, requires: "any" as const },
    {
      id: "fast",
      agent_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      primary: true,
      inherits: null,
      requires: "any" as const,
    },
    {
      id: "subagent",
      agent_key: "CLAUDE_CODE_SUBAGENT_MODEL",
      primary: false,
      inherits: "default",
      requires: "any" as const,
    },
  ],
  config_files: [],
};
const agentDescriptor = vi.fn((): typeof descriptor | null => descriptor);
const roleDrops = vi.fn(() => [] as { role: string; reason: string }[]);

vi.mock("../../../api/agents", () => ({
  useAgentDescriptor: () => agentDescriptor(),
  useRoleDrops: () => roleDrops(),
}));

function provider(partial: Partial<ProviderEntryFlat> = {}): ProviderEntryFlat {
  return {
    id: "p1",
    name: "Relay",
    base_url_openai: "",
    base_url_anthropic: "https://relay.example.com/anthropic",
    models_url: "",
    api_key: "sk",
    models: ["big-model", "small-model"],
    default_model: "big-model",
    sort_index: 0,
    ...partial,
  };
}

function binding(roles: Record<string, RoleTarget> = {}): ToolBinding {
  return {
    entries: [{ provider_id: "p1", model: "big-model" }],
    active_index: 0,
    settings: { roles },
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  updateProvider.mockResolvedValue(provider());
  fetchModelCatalog.mockResolvedValue({
    models: ["new-model"],
    catalog: [],
    metadata_sources: [],
    missing_cost_count: 0,
  });
  updateToolBindingSettings.mockResolvedValue({ tool_id: "claude-code", success: true });
  agentDescriptor.mockReturnValue(descriptor);
  roleDrops.mockReturnValue([]);
});

describe("ClaudeMappingPanel persistence", () => {
  it.each([
    "unsubmitted",
    "failed",
  ])("can discard a %s draft without another write and unlock parent navigation", async (stage) => {
    const onBusyChange = vi.fn();
    updateToolBindingSettings.mockResolvedValue({ tool_id: "claude-code", success: false, error: "disk locked" });
    render(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "p1", model: "persisted-fast" } })}
        onBusyChange={onBusyChange}
      />,
    );
    const input = screen.getByLabelText("Haiku model");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "abandoned-fast" } });
    expect(onBusyChange).toHaveBeenLastCalledWith(true);
    if (stage === "failed") {
      fireEvent.blur(input);
      await screen.findByRole("alert");
    }
    const discard = screen.getByRole("button", { name: i18n.t("models.claudeMapping.discardDraft") });
    expect(discard).not.toBeDisabled();
    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(discard, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(discard);
    expect(input).toHaveValue("persisted-fast");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(updateToolBindingSettings).toHaveBeenCalledTimes(stage === "failed" ? 1 : 0);
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole("button", { name: i18n.t("models.claudeMapping.discardDraft") })).toBeNull();
  });

  it("locks parent navigation before blur and releases it only after a confirmed save", async () => {
    let finish!: (result: ToolSyncResult) => void;
    updateToolBindingSettings.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    function Parent({ currentBinding }: { currentBinding: ToolBinding }) {
      const [locked, setLocked] = useState(false);
      const [desktop, setDesktop] = useState(false);
      return (
        <>
          <button type="button" disabled={locked} onMouseDown={() => setDesktop(true)}>
            Desktop
          </button>
          {!desktop && (
            <ClaudeMappingPanel
              provider={provider()}
              toolId="claude-code"
              binding={currentBinding}
              onBusyChange={setLocked}
            />
          )}
        </>
      );
    }
    const { rerender } = render(<Parent currentBinding={binding()} />);
    const input = screen.getByLabelText("Haiku model");
    act(() => input.focus());
    fireEvent.change(input, { target: { value: "pending-draft" } });
    const desktop = screen.getByRole("button", { name: "Desktop" });
    expect(desktop).toBeDisabled();
    fireEvent.mouseDown(desktop);
    expect(screen.getByLabelText("Haiku model")).toHaveValue("pending-draft");
    expect(updateToolBindingSettings).not.toHaveBeenCalled();
    fireEvent.blur(input);
    expect(updateToolBindingSettings).toHaveBeenCalledTimes(1);
    rerender(<Parent currentBinding={binding({ fast: { provider_id: "p1", model: "pending-draft" } })} />);
    expect(desktop).toBeDisabled();
    await act(async () => finish({ tool_id: "claude-code", success: true }));
    expect(desktop).not.toBeDisabled();
  });

  it("keeps a confirmed reassignment until the cache echoes both model and provider", async () => {
    const original = binding({ fast: { provider_id: "other", model: "same-model" } });
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={original} />);
    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "different-model" } });
    fireEvent.change(input, { target: { value: "same-model" } });
    fireEvent.blur(input);
    await waitFor(() => expect(input).not.toBeDisabled());
    fireEvent.change(screen.getByLabelText("Default model"), { target: { value: "new-default" } });
    fireEvent.blur(screen.getByLabelText("Default model"));
    expect(updateToolBindingSettings).toHaveBeenLastCalledWith("claude-code", {
      roles: {
        fast: { provider_id: "p1", model: "same-model" },
        default: { provider_id: "p1", model: "new-default" },
      },
    });
    await waitFor(() => expect(input).not.toBeDisabled());
  });

  it("reports dropped roles instead of claiming all roles were written", async () => {
    updateToolBindingSettings.mockResolvedValueOnce({
      tool_id: "claude-code",
      success: true,
      dropped_roles: [{ role: "fast", reason: "no_model" }],
    });
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.quickSet") }));
    expect(await screen.findByRole("alert")).toHaveTextContent(i18n.t("models.claudeMapping.savedWithDrops"));
    expect(toast.success).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Haiku model")).toHaveValue("big-model");
  });

  it("notifies the workbench of busy state and releases it when unmounted", async () => {
    const onBusyChange = vi.fn();
    let finish!: (result: ToolSyncResult) => void;
    updateToolBindingSettings.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { unmount } = render(
      <ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} onBusyChange={onBusyChange} />,
    );
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.quickSet") }));
    expect(onBusyChange).toHaveBeenLastCalledWith(true);
    unmount();
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
    await act(async () => finish({ tool_id: "claude-code", success: true }));
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("keeps a failed draft through optimistic updates and rollback, then follows confirmed cache changes", async () => {
    let rejectWrite!: (error: Error) => void;
    updateToolBindingSettings.mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          rejectWrite = reject;
        }),
    );
    const original = binding();
    const { rerender } = render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={original} />);
    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "draft-fast" } });
    fireEvent.blur(input);
    const optimistic = binding({ fast: { provider_id: "p1", model: "draft-fast" } });
    rerender(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={optimistic} />);
    await act(async () => rejectWrite(new Error("write failed")));
    rerender(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={original} />);
    expect(input).toHaveValue("draft-fast");
    expect(screen.getByRole("alert")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.retrySave") }));
    await waitFor(() => expect(input).not.toBeDisabled());
    rerender(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={optimistic} />);
    rerender(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "p1", model: "external-fast" } })}
      />,
    );
    expect(input).toHaveValue("external-fast");
  });

  it.each(["fetch", "save"])("shows a catalog %s error without success and keeps the role draft", async (stage) => {
    if (stage === "fetch") fetchModelCatalog.mockRejectedValueOnce(new Error("catalog unavailable"));
    else updateProvider.mockRejectedValueOnce(new Error("catalog unavailable"));
    render(
      <ClaudeMappingPanel
        provider={provider({ models_url: "https://relay.example.com/models" })}
        toolId="claude-code"
        binding={binding()}
      />,
    );
    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "unsaved-role" } });
    const button = screen.getByRole("button", { name: i18n.t("models.claudeMapping.fetchModels") });
    fireEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      i18n.t("models.claudeMapping.fetchFailed", { message: "catalog unavailable" }),
    );
    expect(input).toHaveValue("unsaved-role");
    expect(toast.error).toHaveBeenCalled();
    expect(toast.success).not.toHaveBeenCalled();
    expect(button).not.toBeDisabled();
    fireEvent.click(button);
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).toBeNull();
    expect(input).toHaveValue("unsaved-role");
  });

  it("fetches through the API and preserves current metadata until the catalog save finishes", async () => {
    let finishFetch!: (result: ModelCatalogFetchResult) => void;
    let finishSave!: (result: ProviderEntryFlat) => void;
    fetchModelCatalog.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishFetch = resolve;
        }),
    );
    updateProvider.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishSave = resolve;
        }),
    );
    const current = provider({ models_url: "https://relay.example.com/models", meta: { custom: "keep" } });
    const { rerender } = render(<ClaudeMappingPanel provider={current} toolId="claude-code" binding={binding()} />);
    const fetchButton = screen.getByRole("button", { name: i18n.t("models.claudeMapping.fetchModels") });
    fireEvent.click(fetchButton);
    expect(fetchModelCatalog).toHaveBeenCalledWith("p1");
    expect(fetchButton).toBeDisabled();
    const updated = { ...current, models: ["big-model", "added-while-fetching"], meta: { custom: "keep", newer: 42 } };
    rerender(<ClaudeMappingPanel provider={updated} toolId="claude-code" binding={binding()} />);
    const result: ModelCatalogFetchResult = {
      models: [" new-model ", "big-model"],
      catalog: [{ id: "new-model", display_name: "New model", context_length: 100000 }],
      metadata_sources: ["provider"],
      missing_cost_count: 1,
    };
    await act(async () => finishFetch(result));
    expect(updateProvider).toHaveBeenCalledWith("p1", {
      models: ["big-model", "added-while-fetching", "new-model"],
      meta: { custom: "keep", newer: 42, model_catalog: result.catalog },
    });
    expect(toast.success).not.toHaveBeenCalled();
    fireEvent.click(fetchButton);
    expect(fetchModelCatalog).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText("Default model")).toBeDisabled();
    await act(async () => finishSave(updated));
    expect(fetchButton).not.toBeDisabled();
    expect(toast.success).toHaveBeenCalledWith(i18n.t("models.claudeMapping.fetchedModels", { count: 2 }));
    expect(toast.message).toHaveBeenCalledWith(i18n.t("models.claudeMapping.missingCost", { count: 1 }));
    const input = screen.getByLabelText("Haiku model");
    const list = document.getElementById(input.getAttribute("list")!);
    expect(Array.from(list!.querySelectorAll("option"), (option) => option.value)).toEqual([
      "big-model",
      "added-while-fetching",
      "new-model",
    ]);
  });

  it.each(["unbound", "another-provider", "native-login"])("requires an applied API binding, not %s", (scenario) => {
    const currentProvider = provider({
      models_url: "https://relay.example.com/models",
      ...(scenario === "native-login" ? { preset_id: "claude-official" } : {}),
    });
    const currentBinding =
      scenario === "unbound"
        ? null
        : scenario === "another-provider"
          ? { ...binding(), entries: [{ provider_id: "p2", model: "other" }] }
          : binding();
    render(<ClaudeMappingPanel provider={currentProvider} toolId="claude-code" binding={currentBinding} />);
    expect(screen.queryAllByRole("combobox")).toEqual([]);
    for (const button of screen.getAllByRole("button")) {
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    expect(updateToolBindingSettings).not.toHaveBeenCalled();
    expect(fetchModelCatalog).not.toHaveBeenCalled();
  });

  it.each([
    "claude-desktop",
    "codex",
  ])("never writes to %s even with declared roles and an applied API provider", (toolId) => {
    agentDescriptor.mockReturnValue({ ...descriptor, id: toolId });
    render(
      <ClaudeMappingPanel
        provider={provider({ models_url: "https://relay.example.com/models" })}
        toolId={toolId}
        binding={binding()}
      />,
    );
    expect(screen.queryAllByRole("combobox")).toEqual([]);
    expect(screen.getByRole("status")).toHaveTextContent(i18n.t("models.claudeMapping.cliOnly"));
    for (const button of screen.getAllByRole("button")) {
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    expect(updateToolBindingSettings).not.toHaveBeenCalled();
    expect(updateProvider).not.toHaveBeenCalled();
    expect(fetchModelCatalog).not.toHaveBeenCalled();
  });

  it.each([
    "false",
    "reject",
    "unconfirmed",
  ])("does not report %s writes as successful and can retry its retained draft", async (outcome) => {
    const error = "disk locked";
    if (outcome === "false")
      updateToolBindingSettings.mockResolvedValueOnce({ tool_id: "claude-code", success: false, error });
    else if (outcome === "reject") updateToolBindingSettings.mockRejectedValueOnce(new Error(error));
    else updateToolBindingSettings.mockResolvedValueOnce(undefined as unknown as ToolSyncResult);
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.quickSet") }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      i18n.t("models.claudeMapping.saveFailed", {
        message: outcome === "unconfirmed" ? i18n.t("models.claudeMapping.writeUnconfirmed") : error,
      }),
    );
    expect(screen.getByLabelText("Haiku model")).toHaveValue("big-model");
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.retrySave") }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(updateToolBindingSettings).toHaveBeenCalledTimes(2);
    expect(updateToolBindingSettings.mock.calls[1]).toEqual(updateToolBindingSettings.mock.calls[0]);
  });

  it("retains draft edits and untouched role metadata across failed writes", async () => {
    const original = binding({
      default: { provider_id: "p1", model: "old-model", thinking: "high" },
      subagent: { provider_id: "another-provider", model: "other-model", thinking: "low" },
      custom: { provider_id: "p1", model: "custom-model", thinking: "max" },
    });
    original.settings = { ...original.settings, extra_flag: true } as ToolBinding["settings"];
    updateToolBindingSettings.mockResolvedValueOnce({ tool_id: "claude-code", success: false, error: "disk locked" });
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={original} />);
    const input = screen.getByLabelText("Default model");
    fireEvent.change(input, { target: { value: "draft-model" } });
    fireEvent.blur(input);
    await waitFor(() => expect(input).not.toBeDisabled());
    expect(input).toHaveValue("draft-model");
    expect(toast.success).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Haiku model"), { target: { value: "fast-model" } });
    fireEvent.blur(screen.getByLabelText("Haiku model"));
    expect(updateToolBindingSettings).toHaveBeenLastCalledWith("claude-code", {
      extra_flag: true,
      roles: {
        ...original.settings!.roles,
        default: { provider_id: "p1", model: "draft-model", thinking: "high" },
        fast: { provider_id: "p1", model: "fast-model" },
      },
    });
    await waitFor(() => expect(input).not.toBeDisabled());
  });

  it("awaits a real successful write before announcing one-click success", async () => {
    let finish!: (value: { tool_id: string; success: boolean }) => void;
    updateToolBindingSettings.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("models.claudeMapping.quickSet") }));
    expect(toast.success).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Haiku model")).toBeDisabled();
    const quickSet = screen.getByRole("button", { name: i18n.t("models.claudeMapping.quickSet") });
    expect(quickSet).toBeDisabled();
    fireEvent.click(quickSet);
    expect(updateToolBindingSettings).toHaveBeenCalledTimes(1);
    await act(async () => finish({ tool_id: "claude-code", success: true }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(i18n.t("models.claudeMapping.quickSetSuccess")));
  });

  it.each([
    null,
    { ...descriptor, roles: [] },
  ])("never invents writable roles for a missing or empty descriptor: %j", (value) => {
    agentDescriptor.mockReturnValue(value);
    render(
      <ClaudeMappingPanel
        provider={provider({ models_url: "https://relay.example.com/models" })}
        toolId="claude-code"
        binding={binding()}
      />,
    );
    expect(screen.queryAllByRole("combobox")).toEqual([]);
    expect(screen.getByRole("status")).toHaveTextContent(
      i18n.t(value ? "models.claudeMapping.noRoles" : "models.claudeMapping.loadingRoles"),
    );
    for (const button of screen.getAllByRole("button")) {
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    expect(updateToolBindingSettings).not.toHaveBeenCalled();
    expect(fetchModelCatalog).not.toHaveBeenCalled();
    expect(updateProvider).not.toHaveBeenCalled();
  });

  it("writes an edited tier model through to the binding", async () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);

    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "small-model" } });
    fireEvent.blur(input);

    expect(updateToolBindingSettings).toHaveBeenCalledWith("claude-code", {
      roles: { fast: { provider_id: "p1", model: "small-model" } },
    });
    await waitFor(() => expect(input).not.toBeDisabled());
  });

  it("clears the role rather than storing a blank model", async () => {
    render(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "p1", model: "small-model" } })}
      />,
    );

    const input = screen.getByLabelText("Haiku model");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.blur(input);

    // An empty string would be written as `ANTHROPIC_DEFAULT_HAIKU_MODEL: ""`,
    // which Claude Code cannot resolve. Removing the key is what restores its
    // built-in default.
    expect(updateToolBindingSettings).toHaveBeenCalledWith("claude-code", { roles: {} });
    await waitFor(() => expect(input).not.toBeDisabled());
  });

  it("renders only the roles the registry declares", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);

    expect(screen.getByLabelText("Default model")).toBeTruthy();
    expect(screen.getByLabelText("Subagent model")).toBeTruthy();
    // Claude Code has no `ANTHROPIC_DEFAULT_FABLE_MODEL`, so the row that used
    // to exist for it is gone — it could never have been written.
    expect(screen.queryByLabelText("Fable model")).toBeNull();
  });

  it("names the env key each row writes", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    expect(screen.getByText(/ANTHROPIC_DEFAULT_HAIKU_MODEL/)).toBeTruthy();
    expect(screen.getByText(/CLAUDE_CODE_SUBAGENT_MODEL/)).toBeTruthy();
  });

  it("says what an unassigned row falls back to", () => {
    render(<ClaudeMappingPanel provider={provider()} toolId="claude-code" binding={binding()} />);
    // `subagent` inherits `default`; the tier keys inherit nothing SkillStar
    // models, so they say the agent decides rather than naming a fallback that
    // does not exist.
    // The suite runs under zh-CN, so the copy is asserted in that locale.
    expect(screen.getByText(i18n.t("models.claudeMapping.activeDefaultFallback"))).toBeTruthy();
    expect(screen.getByText(/回落到 default/)).toBeTruthy();
    expect(screen.getAllByText(/由该 Agent 自行选择/).length).toBeGreaterThan(0);
  });

  it("marks a row the last write skipped, with the backend's reason", () => {
    roleDrops.mockReturnValue([{ role: "fast", reason: "provider_not_bound" }]);
    render(
      <ClaudeMappingPanel
        provider={provider()}
        toolId="claude-code"
        binding={binding({ fast: { provider_id: "other", model: "cheap" } })}
      />,
    );
    expect(screen.getByText(/没有绑定到此 Agent/)).toBeTruthy();
  });
});
