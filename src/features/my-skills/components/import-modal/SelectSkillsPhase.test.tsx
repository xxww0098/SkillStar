import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiscoveredSkill } from "../../../../types";
import { SelectSkillsPhase } from "./SelectSkillsPhase";

const SKILL: DiscoveredSkill = {
  id: "invalid-description",
  folder_path: "skills/invalid-description",
  description: "Too long",
  already_installed: false,
  frontmatter_issues: ["description_too_long"],
};

describe("SelectSkillsPhase", () => {
  it("shows the concrete frontmatter issue instead of claiming fields are missing", () => {
    render(
      <SelectSkillsPhase
        skills={[SKILL]}
        source="owner/repo"
        selectedSkills={new Set()}
        onToggle={vi.fn()}
        onSelectAll={vi.fn()}
        onDeselectAll={vi.fn()}
        onInstall={vi.fn()}
        fullDepthEnabled={false}
        onDeepScan={vi.fn()}
      />,
    );

    expect(screen.getByText("元数据问题")).toHaveAttribute("title", "description 超过 1024 个字符，无法安装。");
  });
});
