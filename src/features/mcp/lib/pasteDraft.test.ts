import { describe, expect, it } from "vitest";
import type { McpServerEntry } from "../../../types";
import { formatSchemaTokens, mcpDraftToFormValue } from "./pasteDraft";

const draft = {
  id: "",
  name: "github",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-github"],
  env: { GITHUB_TOKEN: "secret" },
  headers: {},
  tags: [],
  enabled: {},
  autoApproveAll: false,
  sortIndex: 0,
} as McpServerEntry;

describe("mcpDraftToFormValue", () => {
  it("seeds the create form without inventing an id", () => {
    const value = mcpDraftToFormValue(draft, { "claude-code": true });
    expect(value).toMatchObject({
      name: "github",
      transport: "stdio",
      command: "npx",
      enabled: { "claude-code": true },
    });
    expect(value).not.toHaveProperty("id");
  });
});

describe("formatSchemaTokens", () => {
  it("keeps small counts exact and abbreviates thousands", () => {
    expect(formatSchemaTokens(12)).toBe("~12");
    expect(formatSchemaTokens(1500)).toBe("~1.5k");
    expect(formatSchemaTokens(12000)).toBe("~12k");
  });
});
