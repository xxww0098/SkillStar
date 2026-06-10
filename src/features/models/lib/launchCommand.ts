/**
 * Claude Code launch-command generation (shown in the agent settings UI so the
 * user can pin a model per shell function). Pure string building — extracted
 * from the old ToolActivationPanel.
 */

export type ClaudeCommandShell = "unix" | "powershell";

export function buildClaudeLaunchCommand(model: string, shell: ClaudeCommandShell): string {
  const commandName = `cc-${slugifyCommandName(model) || "model"}`;
  if (shell === "powershell") {
    return [
      `function ${commandName} {`,
      `  $env:ANTHROPIC_MODEL = "${escapePowerShell(model)}"`,
      `  $env:CLAUDE_CODE_SUBAGENT_MODEL = "${escapePowerShell(model)}"`,
      `  $env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = "1"`,
      `  $env:CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK = "1"`,
      `  $env:CLAUDE_CODE_EFFORT_LEVEL = "max"`,
      `  try { claude --dangerously-skip-permissions @args }`,
      `  finally {`,
      `    Remove-Item Env:\\ANTHROPIC_MODEL -ErrorAction SilentlyContinue`,
      `    Remove-Item Env:\\CLAUDE_CODE_SUBAGENT_MODEL -ErrorAction SilentlyContinue`,
      `    Remove-Item Env:\\CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC -ErrorAction SilentlyContinue`,
      `    Remove-Item Env:\\CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK -ErrorAction SilentlyContinue`,
      `    Remove-Item Env:\\CLAUDE_CODE_EFFORT_LEVEL -ErrorAction SilentlyContinue`,
      `  }`,
      `}`,
    ].join("\n");
  }

  return [
    `${commandName}() {`,
    `  ANTHROPIC_MODEL="${escapeShell(model)}" \\`,
    `  CLAUDE_CODE_SUBAGENT_MODEL="${escapeShell(model)}" \\`,
    `  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \\`,
    `  CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK=1 \\`,
    `  CLAUDE_CODE_EFFORT_LEVEL="max" \\`,
    `  claude --dangerously-skip-permissions "$@"`,
    `}`,
  ].join("\n");
}

export function slugifyCommandName(model: string): string {
  return model
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
}

export function escapeShell(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\$/g, "\\$").replace(/`/g, "\\`");
}

export function escapePowerShell(value: string): string {
  return value.replace(/`/g, "``").replace(/"/g, '`"').replace(/\$/g, "`$");
}
