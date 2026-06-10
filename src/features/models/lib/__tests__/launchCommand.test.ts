import { describe, expect, it } from "vitest";
import { buildClaudeLaunchCommand, escapePowerShell, escapeShell, slugifyCommandName } from "../launchCommand";

describe("slugifyCommandName", () => {
  it("lowercases, replaces runs of non-alphanumerics and trims dashes", () => {
    expect(slugifyCommandName("Claude Sonnet 4.6")).toBe("claude-sonnet-4-6");
    expect(slugifyCommandName("--weird__name--")).toBe("weird-name");
  });

  it("caps at 32 chars and handles empty input", () => {
    expect(slugifyCommandName("x".repeat(40))).toHaveLength(32);
    expect(slugifyCommandName("###")).toBe("");
  });
});

describe("escaping", () => {
  it("escapes shell metacharacters for double-quoted strings", () => {
    expect(escapeShell('a"b$c`d\\e')).toBe('a\\"b\\$c\\`d\\\\e');
  });

  it("escapes powershell backticks, quotes and dollars", () => {
    expect(escapePowerShell('a"b$c`d')).toBe('a`"b`$c``d');
  });
});

describe("buildClaudeLaunchCommand", () => {
  it("builds a unix function pinning the model env vars", () => {
    const cmd = buildClaudeLaunchCommand("claude-sonnet-4-6", "unix");
    expect(cmd).toContain("cc-claude-sonnet-4-6() {");
    expect(cmd).toContain('ANTHROPIC_MODEL="claude-sonnet-4-6"');
    expect(cmd).toContain('CLAUDE_CODE_SUBAGENT_MODEL="claude-sonnet-4-6"');
    expect(cmd).toContain('claude --dangerously-skip-permissions "$@"');
  });

  it("builds a powershell function that cleans env vars in finally", () => {
    const cmd = buildClaudeLaunchCommand("gpt-5.5", "powershell");
    expect(cmd).toContain("function cc-gpt-5-5 {");
    expect(cmd).toContain('$env:ANTHROPIC_MODEL = "gpt-5.5"');
    expect(cmd).toContain("Remove-Item Env:\\ANTHROPIC_MODEL");
  });

  it("falls back to a generic name for unsluggable models", () => {
    expect(buildClaudeLaunchCommand("###", "unix")).toContain("cc-model() {");
  });
});
