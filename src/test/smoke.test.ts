import { describe, expect, it } from "vitest";

describe("test infrastructure smoke", () => {
  it("should provide jsdom window and document", () => {
    expect(typeof window).toBe("object");
    expect(typeof document).toBe("object");
  });

  it("should mock Tauri invoke", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    expect(invoke).toBeDefined();
    expect(typeof invoke).toBe("function");
  });
});
