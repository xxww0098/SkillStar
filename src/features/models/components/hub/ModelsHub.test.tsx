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
    if (cmd === "get_saved_groups") return [];
    if (cmd === "get_profile_names") return [];
    if (cmd === "get_listen_mode") return "loopback";
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
      new Set([
        "get_models_board",
        "get_recent_calls",
        "get_routing_page",
        "get_saved_groups",
        "get_profile_names",
        "get_listen_mode",
      ]),
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
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
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
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
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
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
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
    expect(screen.queryByText("还没有调用")).toBeNull();
    expect(screen.queryByText("还没有探测到可配置的 Agent")).toBeNull();
    expect(screen.queryByText("还没有密钥")).toBeNull();
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
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
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

  it("shows a display name and still saves the upstream id", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_model_choices") return [{ id: "probe/m1", label: "实验" }];
      if (cmd === "save_agent_model") return null;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex" }));

    const list = await screen.findByRole("list", { name: "model choices" });
    expect(within(list).getByRole("button", { name: "实验" })).toBeTruthy();
    expect(within(list).queryByRole("button", { name: "probe/m1" })).toBeNull();
    const text = list.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/sk-/);

    fireEvent.click(within(list).getByRole("button", { name: "实验" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("save_agent_model", {
        agentId: "codex",
        modelRef: "probe/m1",
      });
    });
  });

  it("leaves the upstream id on the row when the display name is refused", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_model_choices") return [{ id: "probe/m1", label: "probe/m1" }];
      if (cmd === "save_model_name") throw new Error("model_name");
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Codex" }));

    const list = await screen.findByRole("list", { name: "model choices" });
    fireEvent.change(screen.getByRole("textbox", { name: "model id" }), { target: { value: "probe/m1" } });
    fireEvent.change(screen.getByRole("textbox", { name: "display name" }), { target: { value: "实\n验" } });
    fireEvent.submit(screen.getByRole("textbox", { name: "model id" }).closest("form") as HTMLFormElement);

    expect((await screen.findByRole("alert")).textContent).toBe("model_name");
    expect(within(list).getByRole("button", { name: "probe/m1" })).toBeTruthy();
    expect(within(list).queryByText("实")).toBeNull();
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
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
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

  it("says what is missing when each column is empty", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return { agents: [], providers: [], gateway: [] };
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByText("还没有探测到可配置的 Agent")).toBeTruthy();
    expect(screen.getByText("还没有密钥")).toBeTruthy();
    expect(screen.getByText("还没有调用")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /DeepSeek|Codex|sk-/ })).toBeNull();
    expect(screen.queryByRole("columnheader")).toBeNull();
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/sk-/);
    expect(text).not.toContain("api.openai.com");
  });

  it("drops only the provider sentence after one provider exists", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") {
        return {
          agents: [],
          providers: [{ id: "p1", name: "DeepSeek", credential_summary: "" }],
          gateway: [],
        };
      }
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByRole("button", { name: "DeepSeek" })).toBeTruthy();
    expect(screen.getByText("还没有探测到可配置的 Agent")).toBeTruthy();
    expect(screen.queryByText("还没有密钥")).toBeNull();
    expect(screen.getByText("还没有调用")).toBeTruthy();
    expect(screen.queryByRole("columnheader")).toBeNull();
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/sk-/);
  });

  it("keeps the three titles when the board command fails", async () => {
    mockInvoke.mockRejectedValue(new Error("https://vendor.example/v1 sk-secret"));
    renderHub(<ModelsHub {...navigation()} />);

    expect(await screen.findByRole("heading", { name: "Agents" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Providers" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Gateway" })).toBeTruthy();
    expect(screen.queryByText("还没有探测到可配置的 Agent")).toBeNull();
    expect(screen.queryByText("还没有密钥")).toBeNull();
    expect(screen.queryByText("还没有调用")).toBeNull();
    expect(document.body.textContent).not.toMatch(/https:\/\//);
    expect(document.body.textContent).not.toMatch(/sk-secret/);
  });

  it("shows the backend refusal and leaves the saved member", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [{ id: "demo", members: ["openai/gpt-test"] }];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      if (cmd === "save_group_members") throw new Error("group_cycle");
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const row = await screen.findByRole("group", { name: "group members demo" });
    expect(within(row).getByText("openai/gpt-test")).toBeTruthy();

    const input = within(row).getByRole("textbox", { name: "member demo" });
    fireEvent.change(input, { target: { value: "group/demo" } });
    fireEvent.submit(input.closest("form") as HTMLFormElement);

    expect((await within(row).findByRole("alert")).textContent).toBe("group_cycle");
    expect(within(row).getByText("openai/gpt-test")).toBeTruthy();
    expect(within(row).queryByRole("listitem", { name: "group/demo" })).toBeNull();
    expect(within(row).queryByText("group/demo")).toBeNull();
    expect(mockInvoke).toHaveBeenCalledWith("save_group_members", {
      id: "demo",
      members: ["openai/gpt-test", "group/demo"],
    });
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/sk-/);
  });

  it("shows a refused new group from the backend and does not list it", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      if (cmd === "save_group_members") throw new Error("group_cycle");
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const id = await screen.findByRole("textbox", { name: "new group id" });
    fireEvent.change(id, { target: { value: "demo" } });
    fireEvent.change(screen.getByRole("textbox", { name: "new group member" }), {
      target: { value: "group/demo" },
    });
    fireEvent.submit(id.closest("form") as HTMLFormElement);

    expect((await screen.findByRole("alert")).textContent).toBe("group_cycle");
    expect(screen.queryByRole("group", { name: "group members demo" })).toBeNull();
    expect(mockInvoke).toHaveBeenCalledWith("save_group_members", {
      id: "demo",
      members: ["group/demo"],
    });
  });

  it("lists backend profile names and does not put a skipped id in the list", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return ["work", "home"];
      if (cmd === "get_listen_mode") return "loopback";
      if (cmd === "apply_profile") return { applied: ["opencode"], skipped: ["goose"] };
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const list = await screen.findByRole("list", { name: "profiles" });
    expect(await within(list).findByRole("button", { name: "work" })).toBeTruthy();
    expect(within(list).getByRole("button", { name: "home" })).toBeTruthy();
    fireEvent.click(within(list).getByRole("button", { name: "work" }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("apply_profile", { name: "work" });
    });
    expect(within(list).queryByRole("button", { name: "goose" })).toBeNull();
    expect(within(list).queryByText("goose")).toBeNull();
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/https:\/\//);
    expect(text).not.toMatch(/sk-/);
    expect(text).not.toContain("api.openai.com");
  });

  it("shows the backend refusal and does not invent a profile name", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return ["work"];
      if (cmd === "get_listen_mode") return "loopback";
      if (cmd === "save_profile") throw new Error("profile_name");
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const list = await screen.findByRole("list", { name: "profiles" });
    fireEvent.change(screen.getByRole("textbox", { name: "profile name" }), { target: { value: "later" } });
    fireEvent.change(screen.getByRole("textbox", { name: "profile agent" }), { target: { value: "opencode" } });
    fireEvent.change(screen.getByRole("textbox", { name: "profile model" }), { target: { value: "openai/gpt-test" } });
    fireEvent.submit(screen.getByRole("textbox", { name: "profile name" }).closest("form") as HTMLFormElement);

    expect((await screen.findByRole("alert")).textContent).toBe("profile_name");
    expect(within(list).queryByRole("button", { name: "later" })).toBeNull();
    expect(within(list).getByRole("button", { name: "work" })).toBeTruthy();
    expect(mockInvoke).toHaveBeenCalledWith("save_profile", {
      name: "later",
      agents: [{ id: "opencode", modelRef: "openai/gpt-test" }],
    });
  });

  it("presses loopback and keeps the agent address on 127.0.0.1", async () => {
    let mode = "loopback";
    mockInvoke.mockImplementation(async (cmd: string, args?: { mode?: string }) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return mode;
      if (cmd === "save_listen_mode") {
        mode = typeof args?.mode === "string" ? args.mode : mode;
        return null;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const group = await screen.findByRole("group", { name: "lan listen" });
    expect(await within(group).findByRole("button", { name: "环回", pressed: true })).toBeTruthy();
    expect(within(group).getByRole("button", { name: "局域网", pressed: false })).toBeTruthy();
    expect(group.textContent).toContain("127.0.0.1");
    expect(group.textContent).not.toContain("0.0.0.0");

    fireEvent.click(within(group).getByRole("button", { name: "局域网" }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("save_listen_mode", { mode: "lan" });
    });
    expect(await within(group).findByRole("button", { name: "局域网", pressed: true })).toBeTruthy();
    expect(within(group).getByRole("button", { name: "环回", pressed: false })).toBeTruthy();
  });

  it("leaves the pressed listen mode when the backend refuses", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_models_board") return BOARD;
      if (cmd === "get_recent_calls") return [];
      if (cmd === "get_routing_page") return { provider: null, groups: [] };
      if (cmd === "get_saved_groups") return [];
      if (cmd === "get_profile_names") return [];
      if (cmd === "get_listen_mode") return "loopback";
      if (cmd === "save_listen_mode") throw new Error("listen_store");
      throw new Error(`unexpected ${cmd}`);
    });
    renderHub(<ModelsHub {...navigation()} />);

    const group = await screen.findByRole("group", { name: "lan listen" });
    expect(await within(group).findByRole("button", { name: "环回", pressed: true })).toBeTruthy();
    fireEvent.click(within(group).getByRole("button", { name: "局域网" }));

    expect((await within(group).findByRole("alert")).textContent).toBe("listen_store");
    expect(within(group).getByRole("button", { name: "环回", pressed: true })).toBeTruthy();
    expect(within(group).getByRole("button", { name: "局域网", pressed: false })).toBeTruthy();
    expect(group.textContent).toContain("127.0.0.1");
    expect(group.textContent).not.toContain("0.0.0.0");
  });
});
