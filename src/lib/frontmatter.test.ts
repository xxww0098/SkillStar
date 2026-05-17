import { describe, expect, it } from "vitest";
import {
  normalizeSkillMarkdownForPreview,
  normalizeTranslatedDocument,
  parseFrontmatterEntries,
  parseFrontmatterKeys,
  readFrontmatterValue,
  splitFrontmatter,
  stripLeadingDuplicatedMetadata,
  unwrapOuterMarkdownFence,
  writeFrontmatterValue,
} from "./frontmatter";

describe("splitFrontmatter", () => {
  it("should split YAML frontmatter from body", () => {
    const input = "---\nname: test\n---\n# Body";
    const result = splitFrontmatter(input);
    expect(result.frontmatter).toBe("name: test");
    expect(result.body).toBe("# Body");
  });

  it("should return null frontmatter when none exists", () => {
    const result = splitFrontmatter("# No frontmatter");
    expect(result.frontmatter).toBeNull();
    expect(result.body).toBe("# No frontmatter");
  });

  it("should handle BOM prefix", () => {
    const input = "\uFEFF---\nname: test\n---\n# Body";
    const result = splitFrontmatter(input);
    expect(result.frontmatter).toBe("name: test");
  });
});

describe("readFrontmatterValue", () => {
  it("should read a value by key", () => {
    const fm = "name: my-skill\ndescription: A test skill";
    expect(readFrontmatterValue(fm, "name")).toBe("my-skill");
    expect(readFrontmatterValue(fm, "description")).toBe("A test skill");
  });

  it("should return null for missing key", () => {
    expect(readFrontmatterValue("name: test", "missing")).toBeNull();
  });
});

describe("writeFrontmatterValue", () => {
  it("should update an existing key", () => {
    const fm = "name: old\ndescription: desc";
    const result = writeFrontmatterValue(fm, "name", "new");
    expect(result).toContain("name: new");
    expect(result).toContain("description: desc");
  });

  it("should add a new key at the top", () => {
    const fm = "description: existing";
    const result = writeFrontmatterValue(fm, "name", "added");
    expect(result.startsWith("name: added")).toBe(true);
  });
});

describe("parseFrontmatterEntries", () => {
  it("should parse simple key-value pairs", () => {
    const entries = parseFrontmatterEntries("name: test\ndescription: hello");
    expect(entries).toHaveLength(2);
    expect(entries[0]).toEqual({ key: "name", value: "test" });
    expect(entries[1]).toEqual({ key: "description", value: "hello" });
  });

  it("should return empty array for null input", () => {
    expect(parseFrontmatterEntries(null)).toEqual([]);
  });

  it("should handle multi-line continuation", () => {
    const entries = parseFrontmatterEntries("description: line one\n  line two");
    expect(entries).toHaveLength(1);
    expect(entries[0].value).toContain("line one");
    expect(entries[0].value).toContain("line two");
  });
});

describe("parseFrontmatterKeys", () => {
  it("should extract all keys as a Set", () => {
    const keys = parseFrontmatterKeys("name: test\ndescription: hello\nuser-invocable: true");
    expect(keys.has("name")).toBe(true);
    expect(keys.has("description")).toBe(true);
    expect(keys.has("user-invocable")).toBe(true);
    expect(keys.size).toBe(3);
  });
});

describe("unwrapOuterMarkdownFence", () => {
  it("should unwrap a markdown fence", () => {
    const input = "```markdown\nHello world\n```";
    expect(unwrapOuterMarkdownFence(input)).toBe("Hello world");
  });

  it("should pass through unfenced text", () => {
    expect(unwrapOuterMarkdownFence("Hello world")).toBe("Hello world");
  });

  it("should strip BOM", () => {
    expect(unwrapOuterMarkdownFence("\uFEFFHello")).toBe("Hello");
  });
});

describe("normalizeSkillMarkdownForPreview", () => {
  it("should strip duplicated metadata from body", () => {
    const input = "---\nname: test\ndescription: desc\n---\nname: test\ndescription: desc\n\n# Real Content";
    const result = normalizeSkillMarkdownForPreview(input);
    expect(result).toContain("# Real Content");
    // Should have frontmatter but not duplicated metadata in body
    expect(result).toContain("---");
  });

  it("should return unchanged when no duplication", () => {
    const input = "---\nname: test\n---\n# Content";
    const result = normalizeSkillMarkdownForPreview(input);
    expect(result).toContain("# Content");
  });
});

describe("stripLeadingDuplicatedMetadata", () => {
  it("should strip known keys from top of content", () => {
    const keys = new Set(["name", "description"]);
    const content = "name: test\ndescription: value\n\n# Body";
    const result = stripLeadingDuplicatedMetadata(content, keys);
    expect(result).toBe("# Body");
  });

  it("should preserve content when no keys match", () => {
    const keys = new Set(["name"]);
    const content = "# Body without metadata";
    expect(stripLeadingDuplicatedMetadata(content, keys)).toBe("# Body without metadata");
  });
});

describe("normalizeTranslatedDocument", () => {
  it("should merge translated frontmatter with original name", () => {
    const original = "---\nname: my-skill\ndescription: original desc\n---\n# Content";
    const translated = "---\nname: translated-name\ndescription: 翻译描述\n---\n# 内容";
    const result = normalizeTranslatedDocument(original, translated);
    expect(result).toContain("name: my-skill"); // name preserved
    expect(result).toContain("翻译描述");
  });

  it("should pass through when original has no frontmatter", () => {
    const result = normalizeTranslatedDocument("# No FM", "# 翻译");
    expect(result).toBe("# 翻译");
  });
});
