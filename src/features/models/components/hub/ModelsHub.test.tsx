import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    if (cmd === "get_recent_calls") return [];
    if (cmd === "get_routing_page") return { provider: null, groups: [] };
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

  it("clicking the three columns does not fetch past the board, recent calls, and routing", async () => {
    const nav = navigation();
    renderHub(<ModelsHub {...nav} />);
    await screen.findByRole("heading", { name: "Agents" });

    fireEvent.click(screen.getByRole("heading", { name: "Agents" }));
    fireEvent.click(screen.getByRole("heading", { name: "Providers" }));
    fireEvent.click(screen.getByRole("heading", { name: "Gateway" }));
    fireEvent.click(screen.getByRole("button", { name: "DeepSeek" }));

    expect(new Set(mockInvoke.mock.calls.map((call) => call[0]))).toEqual(
      new Set(["get_models_board", "get_recent_calls", "get_routing_page"]),
    );
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
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
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

  it("shows the written loopback under the agent and hides a vendor url", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") {
        return {
          agents: [
            { id: "codex", name: "Codex", credential_summary: "", loopback_label: "127.0.0.1:21847" },
            {
              id: "opencode",
              name: "OpenCode",
              credential_summary: "",
              loopback_label: "https://api.openai.com/v1",
            },
          ],
          providers: [{ id: "p1", name: "DeepSeek", credential_summary: "", loopback_label: "" }],
          gateway: [],
        };
      }
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByText("127.0.0.1:21847")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Codex/ })).toBeTruthy();
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toContain("api.openai.com");
    expect(text).not.toContain(PLAINTEXT_KEY);
  });

  it("lists recent calls without a quota word or a vendor url", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") {
        return [
          {
            at: "12:00:00",
            agent: "codex",
            model: "openai/gpt-test",
            status: 200,
            completion_tokens: "5",
          },
          {
            at: "12:00:01",
            agent: "opencode",
            model: "https://api.openai.com/v1",
            status: 502,
            completion_tokens: "",
            upstream: "https://api.openai.com/v1",
            quota: "remaining 90",
          },
        ];
      }
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByText("5")).toBeTruthy();
    expect(screen.getByText("codex")).toBeTruthy();
    expect(screen.getByText("openai/gpt-test")).toBeTruthy();
    expect(screen.queryByText("0")).toBeNull();
    const headers = screen
      .getAllByRole("columnheader")
      .map((header) => header.textContent ?? "")
      .join(" ");
    expect(headers).toMatch(/Agent/);
    expect(headers).toMatch(/Model/);
    expect(headers).toMatch(/Status/);
    expect(headers).toMatch(/Token/);
    expect(headers).not.toMatch(/quota|remaining|allowance|配额|剩余/i);
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toContain("api.openai.com");
    expect(text).not.toContain("remaining");
    expect(text).not.toContain(PLAINTEXT_KEY);
  });

  it("opens a picker of provider and group ids and saves the chosen id", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_model_choices") {
        return [{ id: "openai/gpt-test" }, { id: "group/fast" }];
      }
      if (cmd === "save_agent_model") return null;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
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

  it("saves rotate from the eight routing and affinity words", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") {
        return {
          provider: { routing: "smart", affinity: "auto" },
          groups: [{ id: "fast", routing: "smart", affinity: "session" }],
        };
      }
      if (cmd === "save_routing") return null;
      throw new Error(`unexpected ${cmd}`);
    });
    const nav = navigation();
    nav.selectedProviderId = "p1";
    renderHub(<ModelsHub {...nav} />);

    const provider = await screen.findByRole("group", { name: "routing provider p1" });
    const group = screen.getByRole("group", { name: "routing group fast" });
    for (const word of ["smart", "order", "rotate", "usage"]) {
      expect(within(provider).getByRole("button", { name: word })).toBeTruthy();
      expect(within(group).getByRole("button", { name: word })).toBeTruthy();
    }
    for (const word of ["auto", "session", "turn", "off"]) {
      expect(within(provider).getByRole("button", { name: word })).toBeTruthy();
      expect(within(group).getByRole("button", { name: word })).toBeTruthy();
    }
    expect(within(provider).getByRole("button", { name: "smart" }).getAttribute("aria-pressed")).toBe("true");
    expect(within(group).getByRole("button", { name: "session" }).getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(within(provider).getByRole("button", { name: "rotate" }));
    fireEvent.click(within(group).getByRole("button", { name: "off" }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("save_routing", {
        owner: "provider",
        id: "p1",
        routing: "rotate",
        affinity: "auto",
      });
    });
    expect(mockInvoke).toHaveBeenCalledWith("save_routing", {
      owner: "group",
      id: "fast",
      routing: "smart",
      affinity: "off",
    });
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toContain("api.openai.com");
    expect(text).not.toContain(PLAINTEXT_KEY);
    expect(text).not.toMatch(/quota|remaining|配额|剩余/i);
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
