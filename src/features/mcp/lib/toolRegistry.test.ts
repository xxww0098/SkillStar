import { describe, expect, it } from "vitest";
import { MCP_TOOL_IDS } from "../../../types";
import {
  enabledMcpToolIds,
  MCP_OPTIONAL_FIELDS,
  MCP_TOOL_LABELS,
  mcpSupportLabels,
  mcpToolSupportsField,
  mcpToolsSupporting,
  splitTargetsByFieldSupport,
} from "./toolRegistry";

describe("MCP_TOOL_LABELS", () => {
  it("labels exactly the public target set", () => {
    expect(Object.keys(MCP_TOOL_LABELS).sort()).toEqual([...MCP_TOOL_IDS].sort());
  });

  it("has no blank labels", () => {
    for (const toolId of MCP_TOOL_IDS) {
      expect(MCP_TOOL_LABELS[toolId].trim()).not.toBe("");
    }
  });
});

describe("optional field support", () => {
  it("only claims support for tool ids that exist", () => {
    for (const field of MCP_OPTIONAL_FIELDS) {
      for (const toolId of mcpToolsSupporting(field)) {
        expect(MCP_TOOL_IDS).toContain(toolId);
      }
    }
  });

  it("mirrors the writers in specs.rs", () => {
    expect([...mcpToolsSupporting("autoApprove")]).toEqual(["kiro", "cline"]);
    expect([...mcpToolsSupporting("disabledTools")]).toEqual(["kiro", "codex", "gemini-cli"]);
    expect([...mcpToolsSupporting("timeout")]).toEqual(["opencode", "codex", "cline", "gemini-cli"]);
  });

  it("keeps both Claude targets out of every optional field", () => {
    // claude_code_spec and claude_desktop_chat_spec both write their bare
    // documented shape; projecting an approval list into either would be
    // inventing a field the client does not read.
    for (const field of MCP_OPTIONAL_FIELDS) {
      expect(mcpToolSupportsField("claude-code", field)).toBe(false);
      expect(mcpToolSupportsField("claude-desktop-chat", field)).toBe(false);
    }
  });

  it("does not project Gemini's trust flag as auto-approve", () => {
    expect(mcpToolSupportsField("gemini-cli", "autoApprove")).toBe(false);
    expect(mcpToolSupportsField("gemini-cli", "disabledTools")).toBe(true);
  });

  it("renders support labels rather than raw ids", () => {
    expect(mcpSupportLabels("autoApprove")).toEqual(["Kiro", "Cline"]);
  });
});

describe("splitTargetsByFieldSupport", () => {
  it("splits the selected targets into honoured and ignored", () => {
    const split = splitTargetsByFieldSupport("timeout", ["claude-code", "codex", "cursor", "cline"]);

    expect(split.honoured).toEqual(["codex", "cline"]);
    expect(split.ignored).toEqual(["claude-code", "cursor"]);
  });

  it("returns two empty lists when nothing is selected", () => {
    // "Nothing selected yet" must not read as "everything ignores this".
    expect(splitTargetsByFieldSupport("autoApprove", [])).toEqual({ honoured: [], ignored: [] });
  });
});

describe("enabledMcpToolIds", () => {
  it("returns switched-on targets in canonical order", () => {
    expect(enabledMcpToolIds({ cursor: true, "claude-code": true, codex: false })).toEqual(["claude-code", "cursor"]);
  });

  it("ignores unknown keys and falsy values", () => {
    expect(enabledMcpToolIds({ "claude-desktop": true, gemini: true, kiro: false })).toEqual([]);
  });
});
