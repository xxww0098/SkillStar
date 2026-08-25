import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Skill } from "../../../types";
import { extractSkillOwner, SkillAvatar } from "./SkillAvatar";

const GITHUB_SKILL: Skill = {
  name: "dsh-code-review",
  description: "Code review skill",
  skill_type: "hub",
  stars: 42,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/deepseek-ai/deepseek-harness",
  tree_hash: null,
  category: "None",
  author: "deepseek-ai",
  topics: ["review"],
  source: "deepseek-ai/deepseek-harness",
};

const LOCAL_SKILL: Skill = {
  name: "rust-skills",
  description: "Local rust tools",
  skill_type: "local",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "",
  tree_hash: null,
  category: "None",
  author: null,
  topics: ["rust"],
  source: "local",
};

describe("SkillAvatar", () => {
  it("extracts owner from github source", () => {
    expect(extractSkillOwner(GITHUB_SKILL)).toBe("deepseek-ai");
  });

  it("returns null for local skill", () => {
    expect(extractSkillOwner(LOCAL_SKILL)).toBeNull();
  });

  it("renders an avatar image for remote github skill", () => {
    render(<SkillAvatar skill={GITHUB_SKILL} size="md" />);
    const img = screen.getByRole("img", { hidden: true });
    expect(img).toBeInTheDocument();
    expect(img.getAttribute("src")).toContain("deepseek-ai");
  });

  it("keeps the pre-cached local avatar after it loads", () => {
    render(<SkillAvatar skill={GITHUB_SKILL} size="md" />);
    const img = screen.getByRole("img", { hidden: true });
    expect(img.getAttribute("src")).toBe("/publishers/deepseek-ai.png");
    fireEvent.load(img);
    expect(img.getAttribute("src")).toBe("/publishers/deepseek-ai.png");
  });

  it("falls back to the github redirect url when no local avatar exists", () => {
    render(<SkillAvatar skill={GITHUB_SKILL} size="md" />);
    const img = screen.getByRole("img", { hidden: true });
    fireEvent.error(img);
    expect(img.getAttribute("src")).toBe("https://github.com/deepseek-ai.png?size=120");
  });

  it("renders fallback category icon for local skill without image tag", () => {
    render(<SkillAvatar skill={LOCAL_SKILL} size="md" />);
    expect(screen.queryByRole("img", { hidden: true })).not.toBeInTheDocument();
  });
});
