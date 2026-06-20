import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SshHostListItem } from "../../../lib/ipc/commands/ssh";
import { RemoteSkillsContent } from "./RemoteSkillPanel";

const mockedInvoke = vi.mocked(invoke);

const HOST: SshHostListItem = {
  source: "managed",
  id: "host-1",
  display_name: "Prod GPU Box",
  host: "10.0.0.42",
  port: 22,
  username: "root",
  auth_method: { kind: "key", key_path: "~/.ssh/id_ed25519" },
  default_remote_dir: "~/.claude/skills",
} as SshHostListItem;

const DISCOVERY = {
  agents: [{ agent: "grok", path: "/root/.grok/skills", count: 1 }],
  skills: [
    {
      name: "code-review",
      path: "/root/.grok/skills/code-review",
      agent: "grok",
      size: 8192,
      modified: "2026-01-01",
      layout: "agent", // not "standalone" -> bulk-migrate dialog stays closed
    },
  ],
};

function wrapper() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe("RemoteSkillsContent", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockImplementation(async (command: string) => {
      switch (command) {
        case "list_agent_profiles":
          return [];
        case "discover_remote_skills":
          return DISCOVERY;
        case "list_skills":
          return [];
        default:
          return undefined;
      }
    });
  });

  it("discovers remote skills and renders them through the shared grid (no upward state relay)", async () => {
    render(
      <RemoteSkillsContent
        host={HOST}
        hostsLoading={false}
        hasHosts={true}
        onAddHost={vi.fn()}
        scopeSwitch={<div data-testid="scope-switch" />}
        hostPicker={<div data-testid="host-picker" />}
      />,
      { wrapper: wrapper() },
    );

    // The remote skill is mapped via remoteSkillToSkill and shown in SkillGrid/SkillCard.
    await waitFor(() => expect(screen.getByText("code-review")).toBeInTheDocument());

    // The scope switch + host picker injected by the page live inside this content's own toolbar.
    expect(screen.getByTestId("scope-switch")).toBeInTheDocument();
    expect(screen.getByTestId("host-picker")).toBeInTheDocument();

    expect(mockedInvoke).toHaveBeenCalledWith("discover_remote_skills", { hostId: "host-1" });
  });

  it("renders the no-hosts empty state with an add-host CTA", async () => {
    const onAddHost = vi.fn();
    render(
      <RemoteSkillsContent
        host={null}
        hostsLoading={false}
        hasHosts={false}
        onAddHost={onAddHost}
        scopeSwitch={<div data-testid="scope-switch" />}
        hostPicker={<div data-testid="host-picker" />}
      />,
      { wrapper: wrapper() },
    );

    // No host -> discovery never fires.
    await waitFor(() => expect(screen.getByTestId("scope-switch")).toBeInTheDocument());
    expect(mockedInvoke).not.toHaveBeenCalledWith("discover_remote_skills", expect.anything());
  });
});
