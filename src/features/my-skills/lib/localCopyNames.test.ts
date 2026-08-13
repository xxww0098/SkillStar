import { describe, expect, it } from "vitest";
import type { SkillUpdateBlocked } from "../../../types";
import { isValidSkillFolderName, validateLocalCopyNames } from "./localCopyNames";

function blocked(name: string): SkillUpdateBlocked {
  return { name, reason: "content_changed", suggested_local_name: `${name}.local`, error: null };
}

describe("isValidSkillFolderName", () => {
  it("accepts the names Skills actually use", () => {
    for (const name of ["pdf-tools", "pdf-tools.local", "xlsx.local.2", "my skill", "技能"]) {
      expect(isValidSkillFolderName(name), name).toBe(true);
    }
  });

  it("rejects what the hub cannot hold as a directory", () => {
    for (const name of ["", ".", "..", "a/b", "a\\b", 'a"b', "a:b", "a*b", "trailing.", "trailing ", "CON", "lpt3"]) {
      expect(isValidSkillFolderName(name), name).toBe(false);
    }
  });
});

describe("validateLocalCopyNames", () => {
  const queue = [blocked("pdf-tools"), blocked("xlsx")];

  it("passes the backend's own suggestions", () => {
    expect(
      validateLocalCopyNames(queue, { "pdf-tools": "pdf-tools.local", xlsx: "xlsx.local" }, ["pdf-tools", "xlsx"]),
    ).toEqual({});
  });

  it("catches the collisions that would fail the batch halfway through", () => {
    const issues = validateLocalCopyNames(queue, { "pdf-tools": "shared-name", xlsx: "shared-name" }, [
      "pdf-tools",
      "xlsx",
    ]);
    expect(issues).toEqual({ xlsx: "duplicate" });
  });

  it("refuses a name an installed Skill already owns, including its own", () => {
    expect(
      validateLocalCopyNames(queue, { "pdf-tools": "deep-research", xlsx: "xlsx" }, [
        "pdf-tools",
        "xlsx",
        "deep-research",
      ]),
    ).toEqual({ "pdf-tools": "taken", xlsx: "taken" });
  });

  it("reports an empty or unusable name per Skill", () => {
    expect(validateLocalCopyNames(queue, { "pdf-tools": "   ", xlsx: "bad/name" }, [])).toEqual({
      "pdf-tools": "required",
      xlsx: "invalid",
    });
  });
});
