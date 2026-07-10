import { describe, expect, it } from "vitest";
import type { McpSyncResult } from "../../../types";
import { failedMcpSyncCount } from "./syncResults";

function result(success: boolean, skipped = false): McpSyncResult {
  return {
    toolId: "claude-code",
    serverId: "server-id",
    success,
    skipped,
  };
}

describe("failedMcpSyncCount", () => {
  it("counts failures but excludes successful and intentionally skipped targets", () => {
    expect(failedMcpSyncCount([result(true), result(false, true), result(false), result(false)])).toBe(2);
  });
});
