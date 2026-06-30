import { describe, expect, it } from "vitest";
import type { ToolBinding } from "../../../../types";
import {
  activeEntry,
  bindsProvider,
  EMPTY_BINDING,
  removeBindingEntry,
  setActiveProvider,
  upsertBindingEntry,
} from "../toolBinding";

const entry = (id: string, model = "m") => ({ provider_id: id, model });

describe("toolBinding helpers", () => {
  describe("activeEntry", () => {
    it("returns null for empty/undefined bindings", () => {
      expect(activeEntry(undefined)).toBeNull();
      expect(activeEntry(EMPTY_BINDING)).toBeNull();
    });

    it("returns the entry at active_index", () => {
      const b: ToolBinding = { entries: [entry("a"), entry("b")], active_index: 1 };
      expect(activeEntry(b)?.provider_id).toBe("b");
    });

    it("clamps a stale active_index to the last entry", () => {
      const b: ToolBinding = { entries: [entry("a")], active_index: 5 };
      expect(activeEntry(b)?.provider_id).toBe("a");
    });
  });

  describe("bindsProvider", () => {
    it("matches any entry, not just the active one", () => {
      const b: ToolBinding = { entries: [entry("a"), entry("b")], active_index: 0 };
      expect(bindsProvider(b, "b")).toBe(true);
      expect(bindsProvider(b, "z")).toBe(false);
    });
  });

  describe("upsertBindingEntry", () => {
    it("single-provider agent replaces the sole entry", () => {
      const prev: ToolBinding = { entries: [entry("a")], active_index: 0 };
      const next = upsertBindingEntry(prev, "claude-code", entry("b", "x"));
      expect(next.entries).toHaveLength(1);
      expect(next.entries[0].provider_id).toBe("b");
      expect(next.active_index).toBe(0);
    });

    it("multi-provider agent appends a new provider and activates it", () => {
      const prev: ToolBinding = { entries: [entry("a")], active_index: 0 };
      const next = upsertBindingEntry(prev, "codex", entry("b", "x"));
      expect(next.entries.map((e) => e.provider_id)).toEqual(["a", "b"]);
      expect(next.active_index).toBe(1);
    });

    it("multi-provider agent updates an already-bound provider in place", () => {
      const prev: ToolBinding = { entries: [entry("a", "old"), entry("b")], active_index: 1 };
      const next = upsertBindingEntry(prev, "codex", entry("a", "new"));
      expect(next.entries).toHaveLength(2);
      expect(next.entries[0]).toMatchObject({ provider_id: "a", model: "new" });
      expect(next.active_index).toBe(0);
    });
  });

  describe("removeBindingEntry", () => {
    it("drops the entry and re-clamps the active pointer", () => {
      const prev: ToolBinding = { entries: [entry("a"), entry("b"), entry("c")], active_index: 2 };
      const next = removeBindingEntry(prev, "a");
      expect(next.entries.map((e) => e.provider_id)).toEqual(["b", "c"]);
      expect(next.active_index).toBe(1);
    });

    it("is a no-op when the provider is not bound", () => {
      const prev: ToolBinding = { entries: [entry("a")], active_index: 0 };
      expect(removeBindingEntry(prev, "z")).toEqual(prev);
    });

    it("never leaves active_index past the end", () => {
      const prev: ToolBinding = { entries: [entry("a"), entry("b")], active_index: 1 };
      const next = removeBindingEntry(prev, "b");
      expect(next.entries).toHaveLength(1);
      expect(next.active_index).toBe(0);
    });
  });

  describe("setActiveProvider", () => {
    it("moves the pointer to a bound provider", () => {
      const prev: ToolBinding = { entries: [entry("a"), entry("b")], active_index: 0 };
      expect(setActiveProvider(prev, "b").active_index).toBe(1);
    });

    it("is unchanged when the provider is not bound", () => {
      const prev: ToolBinding = { entries: [entry("a")], active_index: 0 };
      expect(setActiveProvider(prev, "z")).toEqual(prev);
    });
  });
});
