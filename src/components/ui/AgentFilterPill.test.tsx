import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentProfile } from "../../types";
import { AgentFilterPill, type AgentFilterItem } from "./AgentFilterPill";

type FilterProfile = Pick<AgentProfile, "id" | "icon" | "display_name">;

const CLAUDE: FilterProfile = { id: "claude", icon: "lobe:claude", display_name: "Claude Code" };
const COPILOT: FilterProfile = { id: "github-copilot", icon: "lobe:github-copilot", display_name: "GitHub Copilot" };

function item(toolId: string, profile: FilterProfile): AgentFilterItem {
  return { id: toolId, profile };
}

describe("AgentFilterPill", () => {
  it("paints an entry from its profile when the filter value uses another vocabulary", () => {
    // A consumer may filter by ids spelled differently from the Agent profile id
    // (claude-code vs claude, vscode vs github-copilot). Resolving the glyph
    // from the filter value silently fell back to the generic LobeHub mark.
    const { container } = render(
      <AgentFilterPill
        items={[item("claude-code", CLAUDE), item("vscode", COPILOT)]}
        value={null}
        onChange={() => {}}
      />,
    );

    const titles = [...container.querySelectorAll("svg > title")].map((node) => node.textContent);
    expect(titles).toEqual(["Claude Code", "GithubCopilot"]);
    expect(titles).not.toContain("LobeHub");
  });

  it("still reports the consumer's filter value and labels the entry with the profile name", () => {
    const onChange = vi.fn();
    render(<AgentFilterPill items={[item("claude-code", CLAUDE)]} value={null} onChange={onChange} />);

    fireEvent.click(screen.getByRole("button", { name: "Claude Code" }));
    expect(onChange).toHaveBeenCalledWith("claude-code");
  });
});
