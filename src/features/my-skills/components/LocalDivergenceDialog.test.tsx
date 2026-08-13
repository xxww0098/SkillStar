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
        blocked={[BLOCKED]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={onPreserve}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    const input = screen.getByLabelText("本地副本名称");
    expect(input).toHaveValue("code-review.local");
    fireEvent.change(input, { target: { value: "code-review-team-copy" } });
    fireEvent.click(screen.getByRole("button", { name: "保留副本并更新" }));

    expect(onPreserve).toHaveBeenCalledWith({ "code-review": "code-review-team-copy" });
  });

  it("requires an explicit destructive action before discarding local changes", () => {
    const onDiscard = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={[BLOCKED]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={onDiscard}
        onUninstall={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));

    expect(onDiscard).toHaveBeenCalledOnce();
  });

  it("shows the concrete snapshot failure that blocked the update", () => {
    render(
      <LocalDivergenceDialog
        blocked={[{ ...BLOCKED, reason: "snapshot_failed", error: "Skill content exceeds the snapshot limit" }]}
        busy={false}
        error="Skill content exceeds the snapshot limit"
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    expect(screen.getByText("Skill content exceeds the snapshot limit")).toBeInTheDocument();
  });

  it("lists every blocked Skill and resolves the batch with one choice", () => {
    const onPreserve = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={[
          BLOCKED,
          {
            name: "wizard",
            reason: "content_changed",
            suggested_local_name: "wizard.local",
            error: null,
          },
        ]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={onPreserve}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    expect(screen.getByText("code-review")).toBeInTheDocument();
    expect(screen.getByText("wizard")).toBeInTheDocument();
    expect(screen.getAllByLabelText("本地副本名称")).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "全部保留副本并更新" }));

    expect(onPreserve).toHaveBeenCalledOnce();
    expect(onPreserve).toHaveBeenCalledWith({
      "code-review": "code-review.local",
      wizard: "wizard.local",
    });
  });

  it("names why each Skill is blocked instead of calling every stop a local edit", () => {
    render(
      <LocalDivergenceDialog
        blocked={[
          BLOCKED,
          { name: "wizard", reason: "baseline_missing", suggested_local_name: "wizard.local", error: null },
        ]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    expect(screen.getByText("本地已修改")).toBeInTheDocument();
    expect(screen.getByText("缺少安装基线")).toBeInTheDocument();
    // Not every stop is the user's doing, so the heading must not claim it is.
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("更新已停止");
  });

  it("blocks a copy name that would fail partway through the batch", () => {
    const onPreserve = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={[
          BLOCKED,
          { name: "wizard", reason: "content_changed", suggested_local_name: "wizard.local", error: null },
        ]}
        busy={false}
        error={null}
        takenNames={["code-review", "wizard", "deep-research"]}
        onClose={vi.fn()}
        onPreserve={onPreserve}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    const inputs = screen.getAllByLabelText("本地副本名称");
    fireEvent.change(inputs[1], { target: { value: "code-review.local" } });

    expect(screen.getByText("本次操作中已使用该名称")).toBeInTheDocument();
    const preserve = screen.getByRole("button", { name: "全部保留副本并更新" });
    expect(preserve).toBeDisabled();
    fireEvent.click(preserve);
    expect(onPreserve).not.toHaveBeenCalled();

    fireEvent.change(inputs[1], { target: { value: "deep-research" } });
    expect(screen.getByText("已存在同名 Skill")).toBeInTheDocument();

    fireEvent.change(inputs[1], { target: { value: "wizard-copy" } });
    expect(screen.getByRole("button", { name: "全部保留副本并更新" })).toBeEnabled();
  });

  it("offers removal, not discard, for a Skill its source dropped", () => {
    const onUninstall = vi.fn();
    const onPreserve = vi.fn();
    render(
      <LocalDivergenceDialog
        blocked={[{ ...BLOCKED, reason: "source_removed" }]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={onPreserve}
        onDiscard={vi.fn()}
        onUninstall={onUninstall}
      />,
    );

    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("来源已不再提供");
    expect(screen.queryByRole("button", { name: "丢弃修改并更新" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "转为本地副本并更新" }));
    expect(onPreserve).toHaveBeenCalledWith({ "code-review": "code-review.local" });

    fireEvent.click(screen.getByRole("button", { name: "移除并更新" }));
    expect(onUninstall).toHaveBeenCalledOnce();
  });

  it("cannot offer a copy of content that is already gone", () => {
    render(
      <LocalDivergenceDialog
        blocked={[
          { ...BLOCKED, reason: "source_removed" },
          { name: "wizard", reason: "source_missing", suggested_local_name: "wizard.local", error: null },
        ]}
        busy={false}
        error={null}
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    // Only the Skill that still has content gets a name to type.
    expect(screen.getAllByLabelText("本地副本名称")).toHaveLength(1);
    expect(screen.getByText("本地内容已不存在，无法转为副本，只能移除。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "全部转为本地副本并更新" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "全部移除并更新" })).toBeEnabled();
  });

  it("reports how far a batch has got while it applies", () => {
    render(
      <LocalDivergenceDialog
        blocked={[BLOCKED]}
        busy={true}
        progress={{ done: 1, total: 3 }}
        error={null}
        onClose={vi.fn()}
        onPreserve={vi.fn()}
        onDiscard={vi.fn()}
        onUninstall={vi.fn()}
      />,
    );

    expect(screen.getByText("正在处理 1/3")).toBeInTheDocument();
    expect(screen.getByLabelText("本地副本名称")).toBeDisabled();
  });
});
