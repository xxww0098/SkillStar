import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SkillTutorial } from "../../types";
import { SkillTutorialPanel } from "./SkillTutorialPanel";

vi.mock("react-i18next", () => {
  const messages: Record<string, string> = {
    "common.close": "Close",
    "settings.acpStyleGuided": "Guided tour",
    "skillTutorial.acpMissingDescription": "Configure an ACP agent first.",
    "skillTutorial.acpMissingTitle": "ACP agent is not configured",
    "skillTutorial.configureAcp": "Configure ACP",
    "skillTutorial.freshBadge": "Matches current version",
    "skillTutorial.generateFailed": "Guide generation failed: {{message}}",
    "skillTutorial.hideOld": "Hide old guide",
    "skillTutorial.iframeTitle": "{{skillName}} usage guide",
    "skillTutorial.loadFailed": "Failed to load tutorial status",
    "skillTutorial.metadata": "{{fileCount}} files · {{totalBytes}} · {{generatedAt}}",
    "skillTutorial.oldIframeTitle": "Old usage guide for {{skillName}}",
    "skillTutorial.oldVersionBadge": "Old version",
    "skillTutorial.staleContentChanged": "The Skill has changed.",
    "skillTutorial.staleTitle": "Guide update available",
    "skillTutorial.title": "AI Usage Guide",
    "skillTutorial.update": "Update guide",
    "skillTutorial.viewOld": "View old guide",
  };

  return {
    useTranslation: () => ({
      i18n: { language: "en", resolvedLanguage: "en" },
      t: (key: string, values?: Record<string, unknown>) =>
        (messages[key] ?? key).replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name] ?? "")),
    }),
  };
});

vi.mock("../ui/ResizablePanel", () => ({
  ResizablePanel: ({ children }: { children: ReactNode }) => <section>{children}</section>,
}));

const metadata = {
  skillName: "demo",
  contentHash: "sha256:current",
  promptVersion: "tutorial-v1",
  schemaVersion: "1",
  tutorialStyle: "guided" as const,
  agentLabel: "Codex",
  generatedAt: "2026-07-14T08:00:00Z",
  fileCount: 3,
  totalBytes: 1_024,
};

function tutorial(overrides: Partial<SkillTutorial> = {}): SkillTutorial {
  return {
    state: "fresh",
    currentHash: "sha256:current",
    html: "<!doctype html><html><body>Current guide</body></html>",
    metadata,
    staleReason: null,
    ...overrides,
  };
}

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: { retry: false },
    },
  });
}

function renderPanel(queryClient = createQueryClient()) {
  return render(
    <QueryClientProvider client={queryClient}>
      <SkillTutorialPanel skillName="demo" onClose={vi.fn()} />
    </QueryClientProvider>,
  );
}

describe("SkillTutorialPanel", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it("opens a hash-matched artifact directly in a scriptless iframe sandbox", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_skill_tutorial") return tutorial();
      if (command === "get_acp_config") {
        return { enabled: true, agent_command: "codex --acp", agent_label: "Codex", tutorial_style: "guided" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    renderPanel();

    const frame = await screen.findByTitle("demo usage guide");
    expect(frame).toHaveAttribute("sandbox", "");
    expect(frame).toHaveAttribute("referrerpolicy", "no-referrer");
    expect(frame).not.toHaveAttribute("allow");
    expect(screen.getByText("Matches current version")).toBeInTheDocument();
    expect(screen.getByText("Guided tour")).toBeInTheDocument();
  });

  it("warns for stale content and keeps the old artifact readable when regeneration fails", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_skill_tutorial") {
        return tutorial({
          state: "stale",
          html: "<!doctype html><html><body>Old guide</body></html>",
          metadata: { ...metadata, contentHash: "sha256:old" },
          staleReason: "content_changed",
        });
      }
      if (command === "get_acp_config") {
        return { enabled: true, agent_command: "codex --acp", agent_label: "Codex", tutorial_style: "guided" };
      }
      if (command === "generate_skill_tutorial") throw new Error("ACP stopped");
      throw new Error(`Unexpected command: ${command}`);
    });

    renderPanel();

    expect(await screen.findByText("Guide update available")).toBeInTheDocument();
    expect(screen.queryByTitle("Old usage guide for demo")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "View old guide" }));
    expect(screen.getByTitle("Old usage guide for demo")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Update guide" }));
    expect(await screen.findByText(/Guide generation failed: Error: ACP stopped/)).toBeInTheDocument();
    expect(screen.getByTitle("Old usage guide for demo")).toBeInTheDocument();
  });

  it("routes missing ACP configuration to the ACP settings section", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_skill_tutorial") {
        return tutorial({ state: "missing", html: null, metadata: null });
      }
      if (command === "get_acp_config") {
        return { enabled: false, agent_command: "", agent_label: "", tutorial_style: "guided" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Configure ACP" }));
    expect(localStorage.getItem("skillstar:settings-focus")).toBe("acp");
  });

  it("does not present cached fresh HTML when the current mount validation fails", async () => {
    const queryClient = createQueryClient();
    queryClient.setQueryData(["get_skill_tutorial", { name: "demo", locale: "en" }], tutorial());
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_skill_tutorial") throw new Error("metadata is incomplete");
      if (command === "get_acp_config") {
        return { enabled: true, agent_command: "codex --acp", agent_label: "Codex", tutorial_style: "guided" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    renderPanel(queryClient);

    expect(await screen.findByText("Failed to load tutorial status")).toBeInTheDocument();
    expect(screen.queryByTitle("demo usage guide")).not.toBeInTheDocument();
    expect(screen.queryByText("Matches current version")).not.toBeInTheDocument();
  });
});
