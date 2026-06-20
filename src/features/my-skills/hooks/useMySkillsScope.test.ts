import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useMySkillsScope } from "./useMySkillsScope";

const KEY = "skillstar.mySkills.scope";

describe("useMySkillsScope", () => {
  beforeEach(() => {
    localStorage.clear();
    window.location.hash = "";
  });
  afterEach(() => {
    localStorage.clear();
    window.location.hash = "";
  });

  it("defaults to local when nothing is persisted", () => {
    const { result } = renderHook(() => useMySkillsScope());
    expect(result.current.scope).toBe("local");
  });

  it("restores the persisted scope", () => {
    localStorage.setItem(KEY, "remote");
    const { result } = renderHook(() => useMySkillsScope());
    expect(result.current.scope).toBe("remote");
  });

  it("setScope updates state and persists", () => {
    const { result } = renderHook(() => useMySkillsScope());
    act(() => result.current.setScope("remote"));
    expect(result.current.scope).toBe("remote");
    expect(localStorage.getItem(KEY)).toBe("remote");
  });

  it("the #ssh deep-link opens the remote scope and normalises the hash", () => {
    window.location.hash = "#ssh";
    const { result } = renderHook(() => useMySkillsScope());
    expect(result.current.scope).toBe("remote");
    expect(localStorage.getItem(KEY)).toBe("remote");
    expect(window.location.hash).toBe("#skills");
  });
});
