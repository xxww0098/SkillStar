import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiscoveredSkill } from "../../../../types";
import { SelectSkillsPhase } from "./SelectSkillsPhase";

const SKILL: DiscoveredSkill = {
  id: "invalid-description",
  folder_path: "skills/invalid-description",
  description: "Too long",
  already_installed: false,
  installable: true,
  frontmatter_issues: ["description_too_long"],
};

const BLOCKING_SKILL: DiscoveredSkill = {
  ...SKILL,
  id: "missing-description",
  folder_path: "skills/missing-description",
  installable: false,
  frontmatter_issues: ["missing_description"],
};

const VALID_SKILL: DiscoveredSkill = {
  ...SKILL,
  id: "valid-skill",
  folder_path: "skills/valid-skill",
  installable: true,
  frontmatter_issues: [],
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

    expect(screen.getByText("兼容性警告")).toHaveAttribute(
      "title",
      "description 超过 1024 个字符，部分 Agent 可能不兼容，但仍可安装。",
    );
    expect(screen.getByText("兼容性警告")).toHaveClass("text-amber-600");
  });

  it("keeps blocking skills out of selection while allowing valid siblings", () => {
    const onToggle = vi.fn();
    const onSelectAll = vi.fn();
    render(
      <SelectSkillsPhase
        skills={[BLOCKING_SKILL, VALID_SKILL]}
        source="owner/repo"
        selectedSkills={new Set()}
        onToggle={onToggle}
        onSelectAll={onSelectAll}
        onDeselectAll={vi.fn()}
        onInstall={vi.fn()}
        fullDepthEnabled={false}
        onDeepScan={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("missing-description").closest('[role="button"]')!);
    expect(onToggle).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("全选"));
    expect(onSelectAll).toHaveBeenCalledWith(["valid-skill"]);
  });
});
