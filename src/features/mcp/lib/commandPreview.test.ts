import { describe, expect, it } from "vitest";
import {
  buildCommandConfirmation,
  buildEnvPreview,
  maskSecrets,
  renderMcpCommand,
  SECRET_MASK,
} from "./commandPreview";

describe("renderMcpCommand", () => {
  // These cases pin the port of `render_command` in
  // crates/skillstar-app/src/mcp/install.rs. If the Rust quoting rule changes,
  // one of these fails rather than the two previews silently diverging.
  it("leaves plain arguments unquoted", () => {
    expect(renderMcpCommand("npx", ["-y", "@modelcontextprotocol/server-filesystem"])).toBe(
      "npx -y @modelcontextprotocol/server-filesystem",
    );
  });

  it("single-quotes arguments containing whitespace", () => {
    expect(renderMcpCommand("npx", ["--root", "/Users/dev/My Files"])).toBe("npx --root '/Users/dev/My Files'");
  });

  it("single-quotes an empty argument so it stays visible", () => {
    expect(renderMcpCommand("cmd", [""])).toBe("cmd ''");
  });

  it("escapes embedded single quotes the POSIX way", () => {
    expect(renderMcpCommand("cmd", ["it's"])).toBe("cmd 'it'\\''s'");
  });

  it("quotes an argument containing a double quote", () => {
    expect(renderMcpCommand("cmd", ['say "hi"'])).toBe("cmd 'say \"hi\"'");
  });

  it("quotes non-ASCII whitespace that would otherwise split a word", () => {
    expect(renderMcpCommand("cmd", ["a b"])).toBe("cmd 'a b'");
    expect(renderMcpCommand("cmd", ["a　b"])).toBe("cmd 'a　b'");
  });

  it("never truncates, however long the command line is", () => {
    const long = Array.from({ length: 200 }, (_, i) => `--flag${i}`);
    const rendered = renderMcpCommand("npx", long);
    expect(rendered.endsWith("--flag199")).toBe(true);
    expect(rendered).not.toContain("…");
  });

  it("renders a bare command with no arguments", () => {
    expect(renderMcpCommand("uvx")).toBe("uvx");
  });
});

describe("maskSecrets", () => {
  it("replaces every occurrence of a secret value", () => {
    expect(maskSecrets("npx --token sk-abc --also sk-abc", ["sk-abc"])).toBe(
      `npx --token ${SECRET_MASK} --also ${SECRET_MASK}`,
    );
  });

  it("masks the longest secret first so no partial value survives", () => {
    // Masking "sk" before "sk-abc123" would leave "-abc123" on screen.
    expect(maskSecrets("token sk-abc123", ["sk", "sk-abc123"])).toBe(`token ${SECRET_MASK}`);
  });

  it("ignores empty secrets so the whole string is not shredded", () => {
    expect(maskSecrets("npx -y pkg", ["", ""])).toBe("npx -y pkg");
  });
});

describe("buildCommandConfirmation", () => {
  const base = {
    command: "npx",
    args: ["-y", "@acme/server"],
    resolvedCommandPath: "/usr/local/bin/npx",
    planPreview: "npx -y @acme/server",
  };

  it("re-renders the plan's command identically when nothing was edited", () => {
    const confirmation = buildCommandConfirmation(base);
    expect(confirmation.preview).toBe("npx -y @acme/server");
    expect(confirmation.editedSincePlan).toBe(false);
    expect(confirmation.resolvedPath).toBe("/usr/local/bin/npx");
    expect(confirmation.usesShell).toBe(false);
  });

  it("flags a command the user's own answers changed", () => {
    const confirmation = buildCommandConfirmation({ ...base, args: [...base.args, "--port", "8080"] });
    expect(confirmation.editedSincePlan).toBe(true);
    expect(confirmation.preview).toBe("npx -y @acme/server --port 8080");
  });

  it("masks a secret that reached the argument list", () => {
    const confirmation = buildCommandConfirmation({
      ...base,
      args: ["--token", "sk-secret"],
      secretValues: ["sk-secret"],
    });
    expect(confirmation.preview).toBe(`npx --token ${SECRET_MASK}`);
    expect(confirmation.preview).not.toContain("sk-secret");
  });

  it("returns an empty preview for a remote server with no command", () => {
    const confirmation = buildCommandConfirmation({ ...base, command: null, planPreview: null });
    expect(confirmation).toMatchObject({ preview: "", resolvedPath: null, editedSincePlan: false });
  });

  it("reports an unresolved launcher as unknown rather than inventing a path", () => {
    expect(buildCommandConfirmation({ ...base, resolvedCommandPath: null }).resolvedPath).toBeNull();
    expect(buildCommandConfirmation({ ...base, resolvedCommandPath: "  " }).resolvedPath).toBeNull();
  });
});

describe("buildEnvPreview", () => {
  it("masks secret values but keeps their keys visible", () => {
    const rows = buildEnvPreview({ API_KEY: "sk-abc", PORT: "8080" }, ["API_KEY"]);
    expect(rows).toEqual([
      { key: "API_KEY", value: SECRET_MASK, secret: true },
      { key: "PORT", value: "8080", secret: false },
    ]);
  });

  it("does not mask an empty secret into a fake value", () => {
    expect(buildEnvPreview({ API_KEY: "" }, ["API_KEY"])).toEqual([{ key: "API_KEY", value: "", secret: true }]);
  });

  it("returns nothing for an absent map", () => {
    expect(buildEnvPreview(null, [])).toEqual([]);
  });
});
