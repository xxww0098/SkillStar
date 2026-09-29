import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { ModelsHub } from "./ModelsHub";

const mockInvoke = vi.mocked(invoke);

const BOARD = {
  agents: [{ id: "codex", name: "Codex", credential_summary: "" }],
  providers: [{ id: "p1", name: "DeepSeek", credential_summary: "" }],
  gateway: [],
};

const PLAINTEXT_KEY = "sk-secret-value";

function navigation(): ModelsNavBridge {
  return {
    selectedProviderId: null,
    setSelectedProviderId: vi.fn(),
    modelsDrawerRequest: { kind: "create", nonce: 1 },
    clearModelsDrawerRequest: vi.fn(),
  };
}

function renderHub(ui: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_models_board") return BOARD;
    throw new Error(`unexpected ${cmd}`);
  });
});

describe("ModelsHub", () => {
  it("shows Agents, Providers, and Gateway and not the Claude workbench", async () => {
    renderHub(<ModelsHub {...navigation()} />);

    const agents = await screen.findByRole("heading", { name: "Agents" });
    const providers = screen.getByRole("heading", { name: "Providers" });
    const gateway = screen.getByRole("heading", { name: "Gateway" });
    expect(agents.compareDocumentPosition(providers) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(providers.compareDocumentPosition(gateway) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("Claude 工作台")).toBeNull();
    expect(screen.queryByText(/claude workbench/i)).toBeNull();
    expect(document.body.textContent).not.toMatch(/https:\/\//);
    expect(document.body.textContent).not.toMatch(/sk-/);
    expect(document.body.textContent).not.toMatch(/api[_-]?key/i);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("clicking the three columns only calls get_models_board", async () => {
    const nav = navigation();
    renderHub(<ModelsHub {...nav} />);
    await screen.findByRole("heading", { name: "Agents" });

    fireEvent.click(screen.getByRole("heading", { name: "Agents" }));
    fireEvent.click(screen.getByRole("heading", { name: "Providers" }));
    fireEvent.click(screen.getByRole("heading", { name: "Gateway" }));
    fireEvent.click(screen.getByRole("button", { name: "DeepSeek" }));

    expect(new Set(mockInvoke.mock.calls.map((call) => call[0]))).toEqual(new Set(["get_models_board"]));
    expect(nav.setSelectedProviderId).toHaveBeenCalledTimes(1);
    expect(nav.setSelectedProviderId).toHaveBeenCalledWith("p1");
  });

  it("draws the masked summary when the row has no key field", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") {
        return {
          agents: [{ id: "codex", name: "Codex", credential_summary: "" }],
          providers: [{ id: "p1", name: "DeepSeek", credential_summary: "sk-s••••alue" }],
          gateway: [],
        };
      }
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByText("sk-s••••alue")).toBeTruthy();
    expect(screen.getByRole("button", { name: /DeepSeek/ })).toBeTruthy();
    const text = document.body.textContent ?? "";
    expect(text).not.toContain(PLAINTEXT_KEY);
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/https:\/\/api\./);
    expect(text).not.toMatch(/vendor\.example/);
  });

  it("opens a picker of provider and group ids and saves the chosen id", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_model_choices") {
        return [{ id: "openai/gpt-test" }, { id: "group/fast" }];
      }
      if (cmd === "save_agent_model") return null;
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex" }));

    expect(await screen.findByRole("button", { name: "openai/gpt-test" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "group/fast" })).toBeTruthy();
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toContain(PLAINTEXT_KEY);
    expect(text).not.toMatch(/sk-/);

    fireEvent.click(screen.getByRole("button", { name: "group/fast" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(mockInvoke).toHaveBeenCalledWith("save_agent_model", {
      agentId: "codex",
      modelRef: "group/fast",
    });
  });

  it("keeps the three titles when the board command fails", async () => {
    mockInvoke.mockRejectedValue(new Error("https://vendor.example/v1 sk-secret"));
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByRole("heading", { name: "Agents" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Providers" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Gateway" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/https:\/\//);
    expect(document.body.textContent).not.toMatch(/sk-secret/);
  });
});
