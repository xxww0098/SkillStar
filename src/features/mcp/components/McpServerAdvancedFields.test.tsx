import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpToolId } from "../../../types";
import { McpServerAdvancedFields } from "./McpServerAdvancedFields";

/** Renders the advanced block for one selection, form inputs otherwise empty. */
function renderFields(enabledToolIds: McpToolId[]) {
  render(
    <McpServerAdvancedFields
      enabledToolIds={enabledToolIds}
      autoApproveAll={false}
      onAutoApproveAllChange={vi.fn()}
      autoApproveText=""
      onAutoApproveTextChange={vi.fn()}
      disabledToolsText=""
      onDisabledToolsTextChange={vi.fn()}
      timeoutText=""
      onTimeoutTextChange={vi.fn()}
    />,
  );
}

describe("McpServerAdvancedFields support notes", () => {
  it("names the supported set instead of re-listing the targets that would ignore the field", () => {
    renderFields(["claude-code", "grok", "zcode"]);

    // autoApprove is honoured by Kiro and Cline only; neither is selected.
    expect(screen.getByText(/所选目标都不会写入这个字段——仅 Kiro、Cline 支持。/)).toBeInTheDocument();
    // The targets the user just ticked are already chips above the field, so the
    // note must not recite them.
    expect(screen.queryByText(/Claude Code/)).toBeNull();
  });

  it("compresses a long ignored list to 'the rest'", () => {
    renderFields(["deepseek", "claude-code", "grok", "zcode"]);

    // timeout reaches DeepSeek Harness; the other three selected targets do not.
    expect(screen.getByText(/DeepSeek Harness 会写入；其余 3 个会忽略。/)).toBeInTheDocument();
    expect(screen.queryByText(/Claude Code/)).toBeNull();
  });

  it("still names the ignored targets when there are only a couple", () => {
    renderFields(["kiro", "codex"]);

    expect(screen.getByText(/Kiro 会写入；Codex 会忽略。/)).toBeInTheDocument();
  });

  it("says so when every selected target writes the field", () => {
    renderFields(["kiro", "cline"]);

    expect(screen.getByText("所选目标都会写入这个字段。")).toBeInTheDocument();
  });

  it("lists the supported set from the registry, not a hand-copied locale string", () => {
    renderFields([]);

    // timeout reaches DeepSeek Harness. The `fieldSupportList_timeout` locale
    // copy this replaced had silently dropped it.
    expect(
      screen.getByText("只有 OpenCode、Codex、Cline、Gemini CLI、DeepSeek Harness 会写入，其他目标忽略该字段。"),
    ).toBeInTheDocument();
  });
});
