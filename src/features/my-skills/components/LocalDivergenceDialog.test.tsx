import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SkillUpdateBlocked } from "../../../types";
import { LocalDivergenceDialog } from "./LocalDivergenceDialog";

const BLOCKED: SkillUpdateBlocked = {
  name: "code-review",
  reason: "content_changed",
  suggested_local_name: "code-review.local",
  error: null,
};

describe("LocalDivergenceDialog", () => {
  it("lets the user edit the local-copy name before preserving and updating", () => {
    const onPreserve = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={BLOCKED}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={onPreserve}
        onDiscard={vi.fn()}
      />,
    );

    const input = screen.getByLabelText("本地副本名称");
    expect(input).toHaveValue("code-review.local");
    fireEvent.change(input, { target: { value: "code-review-team-copy" } });
    fireEvent.click(screen.getByRole("button", { name: "保留副本并更新" }));

    expect(onPreserve).toHaveBeenCalledWith("code-review-team-copy");
  });

  it("requires an explicit destructive action before discarding local changes", () => {
    const onDiscard = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={BLOCKED}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={onDiscard}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));

    expect(onDiscard).toHaveBeenCalledOnce();
  });

  it("shows the concrete snapshot failure that blocked the update", () => {
    render(
      <LocalDivergenceDialog
        blocked={{ ...BLOCKED, reason: "snapshot_failed", error: "Skill content exceeds the snapshot limit" }}
        busy={false}
        error="Skill content exceeds the snapshot limit"
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={vi.fn()}
      />,
    );

    expect(screen.getByText("Skill content exceeds the snapshot limit")).toBeInTheDocument();
  });
});
