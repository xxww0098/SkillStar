import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ProjectSkillPlanDiff } from "../../../types";
import { PendingPlanDiffView } from "./PendingPlanApproval";

const PLAN: ProjectSkillPlanDiff = {
  plan_id: "plan-1",
  plan_hash: "abc",
  root: "/tmp/demo",
  will_register: true,
  owner_id: "codex",
  affected_agents: ["codex", "deepseek"],
  changes: [{ name: "demo", action: "create", skill_path: ".agents/skills/demo/SKILL.md" }],
};

describe("PendingPlanDiffView", () => {
  it("renders nothing when there is no plan", () => {
    render(<PendingPlanDiffView plans={[]} approving={false} onApprove={vi.fn()} />);
    expect(screen.queryByTestId("pending-plan")).not.toBeInTheDocument();
  });

  it("shows the diff fields and does not deploy on approve", () => {
    const onApprove = vi.fn();
    render(<PendingPlanDiffView plans={[PLAN]} approving={false} onApprove={onApprove} />);
    expect(screen.getByText(/\/tmp\/demo/)).toBeInTheDocument();
    expect(screen.getAllByText(/codex/).length).toBeGreaterThan(0);
    expect(screen.getByText(/deepseek/)).toBeInTheDocument();
    expect(screen.getByText("将注册这个项目")).toBeInTheDocument();
    expect(screen.getByRole("listitem").textContent?.replace(/\s+/g, " ")).toContain(
      "create demo .agents/skills/demo/SKILL.md",
    );
    fireEvent.click(screen.getByRole("button", { name: "批准" }));
    expect(onApprove).toHaveBeenCalledWith("plan-1");
  });
});
