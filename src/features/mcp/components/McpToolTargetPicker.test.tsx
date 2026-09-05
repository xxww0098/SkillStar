import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentProfile } from "../../../types";
import { McpToolTargetPicker } from "./McpToolTargetPicker";

function profile(id: string, display_name: string): AgentProfile {
  return {
    id,
    display_name,
    icon: `lobe:${id}`,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed: true,
    enabled: true,
    synced_count: 0,
  };
}

describe("McpToolTargetPicker", () => {
  it("renders only the Settings-enabled targets it is given", () => {
    const onToggle = vi.fn();
    render(
      <McpToolTargetPicker
        targets={[{ toolId: "cursor", profile: profile("cursor", "Cursor") }]}
        enabled={{ cursor: true }}
        onToggle={onToggle}
      />,
    );
    expect(screen.getByRole("button", { name: "Cursor" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: "Codex" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cursor" }));
    expect(onToggle).toHaveBeenCalledWith("cursor", false);
  });

  it("explains an empty enabled-agent set instead of listing every MCP target", () => {
    render(<McpToolTargetPicker targets={[]} enabled={{}} onToggle={vi.fn()} />);
    expect(screen.getByText(/没有已启用的 Agent/)).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });
});
