import { describe, expect, it } from "vitest";
import { AGENT_ICON_BY_ID, getAgentIcon } from "./agentIcons";
import { DevinColor, KiroColor, LobeHubMono, PiMono, ZAIMono } from "./lobe";

const BUILTIN_AGENT_IDS = [
  "aider-desk",
  "amp",
  "antigravity",
  "astrbot",
  "autohand-code",
  "augment",
  "bob",
  "claude",
  "openclaw",
  "cline",
  "codearts-agent",
  "codebuddy",
  "codemaker",
  "codestudio",
  "codex",
  "command-code",
  "continue",
  "cortex",
  "crush",
  "cursor",
  "deepagents",
  "deepseek",
  "devin",
  "dexto",
  "droid",
  "eve",
  "firebender",
  "forgecode",
  "gemini-cli",
  "github-copilot",
  "goose",
  "hermes",
  "inference-sh",
  "jazz",
  "junie",
  "iflow-cli",
  "kilo",
  "kimi-code-cli",
  "kiro",
  "kode",
  "lingma",
  "loaf",
  "mcpjam",
  "mistral-vibe",
  "moxby",
  "mux",
  "neovate",
  "omp",
  "opencode",
  "openhands",
  "ona",
  "pi",
  "qoder",
  "qoder-cn",
  "qwen-code",
  "replit",
  "reasonix",
  "roo",
  "rovodev",
  "tabnine-cli",
  "terramind",
  "tinycloud",
  "trae",
  "trae-cn",
  "warp",
  "windsurf",
  "zed",
  "zcode",
  "zencoder",
  "zenflow",
  "pochi",
  "promptscript",
  "adal",
  "universal",
  "grok",
] as const;

describe("Agent icon registry", () => {
  it("maps every built-in Agent to an @lobehub/icons component", () => {
    expect(Object.keys(AGENT_ICON_BY_ID).sort()).toEqual([...BUILTIN_AGENT_IDS].sort());
    for (const id of BUILTIN_AGENT_IDS) {
      expect(getAgentIcon(id)).toBeTruthy();
    }
  });

  it("uses the LobeHub glyph for unknown and custom Agents", () => {
    expect(getAgentIcon("custom-acme")).toBe(LobeHubMono);
  });

  // Guards the generic-fallback regression: brands that ship in @lobehub/icons
  // must not silently fall back to the LobeHub glyph again.
  it("uses brand glyphs for Agents whose brand icon exists upstream", () => {
    expect(getAgentIcon("kiro")).toBe(KiroColor);
    expect(getAgentIcon("devin")).toBe(DevinColor);
    expect(getAgentIcon("pi")).toBe(PiMono);
    expect(getAgentIcon("zcode")).toBe(ZAIMono);
    expect(getAgentIcon("pi")).not.toBe(LobeHubMono);
    expect(getAgentIcon("zcode")).not.toBe(LobeHubMono);
  });
});
