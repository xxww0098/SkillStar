import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Skill } from "../../../types";
import { SkillCard } from "./SkillCard";

const MOCK_SKILL: Skill = {
  name: "test-skill",
  description: "Test description",
  skill_type: "hub",
  stars: 100,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/owner/test-skill",
  tree_hash: "hash123",
  category: "None",
  author: "owner",
  topics: [],
  source: "owner/repo",
};

describe("SkillCard", () => {
  it("renders installed state when installed and no updates", () => {
    render(<SkillCard skill={MOCK_SKILL} onClick={vi.fn()} />);
    const installedBadge = screen.getByText("已安装");
    expect(installedBadge).toBeInTheDocument();
    expect(installedBadge.className).toContain("bg-emerald-500/10");
    expect(installedBadge.className).toContain("text-emerald-700");
  });

  it("renders install button when not installed", () => {
    const onInstall = vi.fn();
    render(<SkillCard skill={{ ...MOCK_SKILL, installed: false }} onClick={vi.fn()} onInstall={onInstall} />);
    const installBtn = screen.getByRole("button", { name: /安装/i });
    expect(installBtn).toBeInTheDocument();
    fireEvent.click(installBtn);
    expect(onInstall).toHaveBeenCalledWith(MOCK_SKILL.git_url, MOCK_SKILL.name);
  });

  it("renders prominent update button when update is available and triggers onUpdate", () => {
    const onUpdate = vi.fn();
    render(<SkillCard skill={{ ...MOCK_SKILL, update_available: true }} onClick={vi.fn()} onUpdate={onUpdate} />);

    const updateBtn = screen.getByRole("button", { name: /更新/i });
    expect(updateBtn).toBeInTheDocument();
    // Verify amber styling class is present
    expect(updateBtn.className).toContain("bg-amber-500/12");
    expect(updateBtn.className).toContain("text-amber-700");

    fireEvent.click(updateBtn);
    expect(onUpdate).toHaveBeenCalledWith(MOCK_SKILL.name);
  });

  it("renders updating state with spinner when updating is true", () => {
    render(<SkillCard skill={{ ...MOCK_SKILL, update_available: true }} onClick={vi.fn()} updating={true} />);

    const updatingBtn = screen.getByRole("button", { name: /更新中/i });
    expect(updatingBtn).toBeInTheDocument();
    expect(updatingBtn).toBeDisabled();
  });
});
