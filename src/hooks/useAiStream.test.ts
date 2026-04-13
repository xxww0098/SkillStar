import { describe, expect, it } from "vitest";
import { estimateAiStreamSafetyTimeoutMs, estimateMarkdownSectionCount } from "./useAiStream";

describe("estimateMarkdownSectionCount", () => {
  it("counts headings outside fenced code blocks", () => {
    const markdown = ["# Intro", "body", "```md", "# Not a heading", "```", "## Details", "### More", ""].join("\n");

    expect(estimateMarkdownSectionCount(markdown)).toBe(3);
  });

  it("returns at least one section for plain content", () => {
    expect(estimateMarkdownSectionCount("plain text only")).toBe(1);
    expect(estimateMarkdownSectionCount("")).toBe(1);
  });
});

describe("estimateAiStreamSafetyTimeoutMs", () => {
  it("keeps the default timeout for non skill translation streams", () => {
    expect(estimateAiStreamSafetyTimeoutMs("ai_summarize_skill_stream", "# Title\nbody")).toBe(60_000);
  });

  it("keeps small skill translations on the default timeout", () => {
    const markdown = "# Title\nshort body\n## Details\nmore\n";
    expect(estimateAiStreamSafetyTimeoutMs("ai_translate_skill_stream", markdown)).toBe(60_000);
  });

  it("scales the timeout for large multi-section skill translations", () => {
    const markdown = Array.from({ length: 20 }, (_, index) => `# Section ${index + 1}\n${"x".repeat(1200)}\n`).join("");

    expect(estimateAiStreamSafetyTimeoutMs("ai_translate_skill_stream", markdown)).toBe(125_000);
  });
});
