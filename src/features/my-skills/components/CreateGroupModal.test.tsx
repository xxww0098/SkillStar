import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Skill } from "../../../types";
import { CreateGroupModal } from "./CreateGroupModal";

const SKILL: Skill = {
  name: "computer-use",
  description: "Computer use",
  skill_type: "hub",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/owner/orca",
  tree_hash: "hash",
  category: "None",
  author: "owner",
  topics: [],
  source: "owner/orca",
  agent_links: [],
};

describe("CreateGroupModal", () => {
  it("prefills the name from Quick Pack and blocks an existing deck name in create mode", () => {
    render(
      <CreateGroupModal
        open
        onClose={vi.fn()}
        availableSkills={[SKILL]}
        initialSkills={[SKILL.name]}
        initialName="orca"
        existingNames={["orca"]}
        onSave={vi.fn()}
      />,
    );

    expect(screen.getByDisplayValue("orca")).toBeInTheDocument();
    expect(screen.getByText("名称已存在")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /创建/ })).toBeDisabled();
  });

  it("allows keeping the original name when editing", () => {
    render(
      <CreateGroupModal
        open
        onClose={vi.fn()}
        availableSkills={[SKILL]}
        initialSkills={[SKILL.name]}
        initialName="orca"
        existingNames={["orca"]}
        mode="edit"
        onSave={vi.fn()}
      />,
    );

    expect(screen.queryByText("名称已存在")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /保存/ })).toBeEnabled();
  });
});
