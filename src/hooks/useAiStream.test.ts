import { describe, expect, it } from "vitest";
import { useAiStream } from "./useAiStream";

describe("useAiStream", () => {
  it("exports the hook function", () => {
    expect(useAiStream).toBeDefined();
    expect(typeof useAiStream).toBe("function");
  });
});
