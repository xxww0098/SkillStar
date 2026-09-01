import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentProfile, Skill } from "../../../types";
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
  category: "Hot",
  author: "owner",
  topics: [],
  source: "owner/repo",
  rank: 3,
};

const LIBRARY_PROFILE: AgentProfile = {
  id: "claude",
  display_name: "Claude",
  icon: "claude",
  global_skills_dir: "~/.claude/skills",
  project_skills_rel: ".claude/skills",
  installed: true,
  enabled: true,
  synced_count: 0,
};

describe("SkillCard", () => {
  it("omits the idle installed mark in the library — every card is already installed", () => {
    render(
      <SkillCard
        skill={MOCK_SKILL}
        onClick={vi.fn()}
        selectable
        profiles={[LIBRARY_PROFILE]}
        onToggleAgent={vi.fn()}
      />,
    );
    expect(screen.queryByText("已安装")).not.toBeInTheDocument();
    expect(screen.queryByText("Hot")).not.toBeInTheDocument();
    expect(screen.queryByText("#3")).not.toBeInTheDocument();
  });

  it("shows a quiet installed mark on the marketplace, not a celebration chip", () => {
    render(<SkillCard skill={MOCK_SKILL} onClick={vi.fn()} />);
    const installed = screen.getByText("已安装");
    expect(installed).toBeInTheDocument();
    expect(installed.className).not.toContain("bg-emerald");
    expect(screen.getByText("#3")).toBeInTheDocument();
    expect(screen.queryByText("Hot")).not.toBeInTheDocument();
  });

  it("renders install button when not installed", () => {
    const onInstall = vi.fn();
    render(<SkillCard skill={{ ...MOCK_SKILL, installed: false }} onClick={vi.fn()} onInstall={onInstall} />);
    const installBtn = screen.getByRole("button", { name: /安装/i });
    expect(installBtn).toBeInTheDocument();
    fireEvent.click(installBtn);
    expect(onInstall).toHaveBeenCalledWith(MOCK_SKILL.git_url, MOCK_SKILL.name);
  });

  it("installs from a harness icon instead of only (url, name)", () => {
    const onInstall = vi.fn();
    render(
      <SkillCard
        skill={{ ...MOCK_SKILL, installed: false }}
        onClick={vi.fn()}
        onInstall={onInstall}
        profiles={[LIBRARY_PROFILE]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /为 Claude 安装/i }));
    expect(onInstall).toHaveBeenCalledWith(MOCK_SKILL.git_url, MOCK_SKILL.name, "claude");
  });

  it("retargets an already-installed skill when a second harness icon is clicked", () => {
    const onInstall = vi.fn();
    const onToggleAgent = vi.fn();
    const deepseek: AgentProfile = { ...LIBRARY_PROFILE, id: "deepseek", display_name: "DeepSeek Harness" };
    render(
      <SkillCard
        skill={{ ...MOCK_SKILL, installed: true, agent_links: ["Claude"] }}
        onClick={vi.fn()}
        onInstall={onInstall}
        onToggleAgent={onToggleAgent}
        profiles={[LIBRARY_PROFILE, deepseek]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /为 DeepSeek Harness 安装/i }));
    expect(onInstall).toHaveBeenCalledWith(MOCK_SKILL.git_url, MOCK_SKILL.name, "deepseek");
    expect(onToggleAgent).not.toHaveBeenCalled();
  });

  it("keeps a Settings-disabled linked Agent as a stopped rail icon", () => {
    const onToggleAgent = vi.fn();
    render(
      <SkillCard
        skill={{ ...MOCK_SKILL, installed: true, agent_links: ["Claude"] }}
        onClick={vi.fn()}
        onToggleAgent={onToggleAgent}
        profiles={[{ ...LIBRARY_PROFILE, enabled: false }]}
      />,
    );
    const icon = screen.getByRole("button", { name: /Claude/i });
    expect(icon).toBeDisabled();
    fireEvent.click(icon);
    expect(onToggleAgent).not.toHaveBeenCalled();
  });

  it("falls back to toggle-on when an unlinked icon has no git_url", () => {
    const onInstall = vi.fn();
    const onToggleAgent = vi.fn();
    render(
      <SkillCard
        skill={{ ...MOCK_SKILL, git_url: "", installed: true, agent_links: [] }}
        onClick={vi.fn()}
        onInstall={onInstall}
        onToggleAgent={onToggleAgent}
        profiles={[LIBRARY_PROFILE]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /为 Claude 安装/i }));
    expect(onInstall).not.toHaveBeenCalled();
    expect(onToggleAgent).toHaveBeenCalledWith("test-skill", "claude", true, "Claude");
  });

  it("renders an update action when an update is available and triggers onUpdate", () => {
    const onUpdate = vi.fn();
    render(<SkillCard skill={{ ...MOCK_SKILL, update_available: true }} onClick={vi.fn()} onUpdate={onUpdate} />);

    const updateBtn = screen.getByRole("button", { name: /更新/i });
    expect(updateBtn).toBeInTheDocument();
    expect(updateBtn.className).toContain("text-amber-300");

    fireEvent.click(updateBtn);
    expect(onUpdate).toHaveBeenCalledWith(MOCK_SKILL.name);
  });

  it("renders updating state with spinner when updating is true", () => {
    render(<SkillCard skill={{ ...MOCK_SKILL, update_available: true }} onClick={vi.fn()} updating={true} />);

    const updatingBtn = screen.getByRole("button", { name: /更新中/i });
    expect(updatingBtn).toBeInTheDocument();
    expect(updatingBtn).toBeDisabled();
  });

  it("toggles selection when clicking the avatar checkbox", () => {
    const onSelect = vi.fn();
    render(<SkillCard skill={MOCK_SKILL} onClick={vi.fn()} selectable onSelect={onSelect} />);

    const selectBtn = screen.getByRole("button", { name: MOCK_SKILL.name });
    expect(selectBtn).toBeInTheDocument();
    fireEvent.click(selectBtn);
    expect(onSelect).toHaveBeenCalledWith(MOCK_SKILL.name);
  });

  it("renders a clickable local badge for local skills that calls open_skill_folder", async () => {
    const localSkill: Skill = {
      ...MOCK_SKILL,
      name: "my-local-skill",
      skill_type: "local",
      source: undefined,
    };
    render(<SkillCard skill={localSkill} onClick={vi.fn()} />);

    const localBtn = screen.getByRole("button", { name: /本地/i });
    expect(localBtn).toBeInTheDocument();
    fireEvent.click(localBtn);
  });

  it("offers a one-step migration when upstream renamed the skill", () => {
    const onMigrate = vi.fn();
    const onUpdate = vi.fn();
    render(
      <SkillCard
        skill={{
          ...MOCK_SKILL,
          update_available: true,
          upstream_change: {
            kind: "removed",
            suggested_local_name: "test-skill.local",
            successor: {
              skill_id: "test-skill-spec",
              folder_path: "skills/engineering/test-skill-spec",
              description: "Renamed",
              similarity: 91,
            },
          },
        }}
        onClick={vi.fn()}
        onUpdate={onUpdate}
        onMigrate={onMigrate}
        selectable
        profiles={[LIBRARY_PROFILE]}
        onToggleAgent={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("迁移到 test-skill-spec"));
    expect(onMigrate).toHaveBeenCalledWith("test-skill");
    // A renamed skill cannot be "updated" in place — the rename wins the slot.
    expect(screen.queryByText("更新")).not.toBeInTheDocument();
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("offers the removal exits when upstream dropped the skill outright", () => {
    const onResolveRemoved = vi.fn();
    render(
      <SkillCard
        skill={{
          ...MOCK_SKILL,
          upstream_change: { kind: "removed", suggested_local_name: "test-skill.local", successor: null },
        }}
        onClick={vi.fn()}
        onResolveRemoved={onResolveRemoved}
        selectable
        profiles={[LIBRARY_PROFILE]}
        onToggleAgent={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("上游已移除"));
    expect(onResolveRemoved).toHaveBeenCalledWith("test-skill");
  });

  it("keeps local skills free of upstream chips", () => {
    render(
      <SkillCard
        skill={{
          ...MOCK_SKILL,
          skill_type: "local",
          upstream_change: { kind: "removed", suggested_local_name: "x", successor: null },
        }}
        onClick={vi.fn()}
        selectable
        profiles={[LIBRARY_PROFILE]}
        onToggleAgent={vi.fn()}
      />,
    );
    expect(screen.queryByText("上游已移除")).not.toBeInTheDocument();
  });
});
