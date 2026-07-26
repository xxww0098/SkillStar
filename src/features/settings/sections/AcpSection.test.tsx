import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AcpSection } from "./AcpSection";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "settings.acpAgent": "Tutorial Generation Agent",
        "settings.acpDesc": "Generate a complete guide.",
        "settings.acpStyleGuided": "Guided tour",
        "settings.acpStyleGuidedDesc": "A step-by-step introduction.",
        "settings.acpStyleReference": "Technical reference",
        "settings.acpStyleReferenceDesc": "A lookup-oriented manual.",
        "settings.acpStyleRecommended": "Recommended",
        "settings.acpStyleWorkshop": "Hands-on workshop",
        "settings.acpStyleWorkshopDesc": "A task-oriented workshop.",
        "settings.acpTitle": "ACP Agent",
        "settings.acpTutorialStyle": "Tutorial style",
        "settings.acpTutorialStyleHint": "Changing style marks guides stale.",
      })[key] ?? key,
  }),
}));

function renderSection() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <AcpSection />
    </QueryClientProvider>,
  );
}

describe("AcpSection tutorial style", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("offers three prompt-backed styles and persists a changed selection", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_acp_config") {
        return {
          enabled: true,
          agent_command: "codex --acp",
          agent_label: "Codex",
          tutorial_style: "guided",
        };
      }
      if (command === "save_acp_config") return undefined;
      throw new Error(`Unexpected command: ${command}`);
    });

    renderSection();

    fireEvent.click(await screen.findByText("Tutorial Generation Agent"));

    const guided = screen.getByRole("button", { name: /Guided tour/ });
    const reference = screen.getByRole("button", { name: /Technical reference/ });
    const workshop = screen.getByRole("button", { name: /Hands-on workshop/ });
    expect(guided).toHaveAttribute("aria-pressed", "true");
    expect(reference).toHaveAttribute("aria-pressed", "false");
    expect(workshop).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(reference);
    expect(reference).toHaveAttribute("aria-pressed", "true");

    await waitFor(
      () => {
        expect(invoke).toHaveBeenCalledWith("save_acp_config", {
          config: {
            enabled: true,
            agent_command: "codex --acp",
            agent_label: "Codex",
            tutorial_style: "reference",
          },
        });
      },
      { timeout: 2_000 },
    );
  });

  it("dispatches a style save before an immediate settings unmount", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_acp_config") {
        return {
          enabled: true,
          agent_command: "codex --acp",
          agent_label: "Codex",
          tutorial_style: "guided",
        };
      }
      if (command === "save_acp_config") return undefined;
      throw new Error(`Unexpected command: ${command}`);
    });

    const view = renderSection();
    fireEvent.click(await screen.findByText("Tutorial Generation Agent"));
    fireEvent.click(screen.getByRole("button", { name: /Technical reference/ }));
    view.unmount();

    expect(invoke).toHaveBeenCalledWith("save_acp_config", {
      config: {
        enabled: true,
        agent_command: "codex --acp",
        agent_label: "Codex",
        tutorial_style: "reference",
      },
    });
  });
});
