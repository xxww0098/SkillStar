import { describe, expect, it } from "vitest";
import type { McpInput, McpInstallInput } from "../../../types";
import {
  buildInstallAnswers,
  buildInstallFields,
  groupInstallFields,
  secretFieldKeys,
  setFieldValue,
  setFieldVariable,
  validateInstallFields,
} from "./installForm";

function input(patch: Partial<McpInput> = {}): McpInput {
  return { isRequired: false, isSecret: false, format: "string", ...patch };
}

function declared(patch: Partial<McpInstallInput> = {}): McpInstallInput {
  return {
    key: "API_KEY",
    scope: "environment",
    index: 0,
    input: input(),
    prefilled: "",
    mustAsk: false,
    ...patch,
  };
}

/** A pinned `Bearer {TOKEN}` header, seeded the way the install plan seeds it. */
function pinnedToken(patch: Partial<McpInstallInput> = {}): McpInstallInput {
  return declared({
    key: "Authorization",
    scope: "header",
    prefilled: "Bearer {TOKEN}",
    input: input({ value: "Bearer {TOKEN}" }),
    variables: [{ name: "TOKEN", variable: { isRequired: true, isSecret: true, format: "string" }, prefilled: "" }],
    ...patch,
  });
}

describe("buildInstallFields", () => {
  it("seeds from the plan's prefill rather than re-deriving it", () => {
    const fields = buildInstallFields([declared({ key: "PORT", prefilled: "50325" })]);
    expect(fields[0]).toMatchObject({ key: "PORT", value: "50325", templated: false, variables: [] });
  });

  it("treats a publisher-pinned value as a template and takes its variables from the plan", () => {
    // Which tokens exist, and what each one is seeded with, is the backend's
    // derivation — the form scans nothing.
    const fields = buildInstallFields([
      pinnedToken({
        variables: [
          { name: "TOKEN", variable: { isRequired: true, isSecret: true, format: "string" }, prefilled: "" },
          { name: "REGION", variable: { isRequired: false, isSecret: false, format: "string" }, prefilled: "us" },
        ],
      }),
    ]);

    expect(fields[0].templated).toBe(true);
    expect(fields[0].variables).toEqual([
      { name: "TOKEN", variable: { isRequired: true, isSecret: true, format: "string" }, value: "" },
      { name: "REGION", variable: { isRequired: false, isSecret: false, format: "string" }, value: "us" },
    ]);
  });

  it("returns an empty form for a plan with no inputs", () => {
    expect(buildInstallFields(undefined)).toEqual([]);
  });
});

describe("editing", () => {
  it("refuses to edit a pinned value directly", () => {
    const fields = buildInstallFields([declared({ prefilled: "fixed", input: input({ value: "fixed" }) })]);
    expect(setFieldValue(fields, "environment", 0, "hacked")[0].value).toBe("fixed");
  });

  it("edits a variable inside a pinned value", () => {
    const next = setFieldVariable(buildInstallFields([pinnedToken()]), "header", 0, "TOKEN", "ghp_x");

    // The template itself stays pinned; only its hole is the user's.
    expect(next[0].value).toBe("Bearer {TOKEN}");
    expect(next[0].variables.map((v) => v.value)).toEqual(["ghp_x"]);
  });

  it("only touches the field with the matching scope and ordinal", () => {
    const fields = buildInstallFields([
      declared({ key: "TOKEN", scope: "environment" }),
      declared({ key: "TOKEN", scope: "header" }),
    ]);
    const next = setFieldValue(fields, "header", 0, "x");

    expect(next.map((f) => f.value)).toEqual(["", "x"]);
  });

  it("keeps two hint-less positional arguments independent", () => {
    // Neither argument has a name or a valueHint, so both are labelled
    // "argument"; only the ordinal tells them apart. Keyed by label, filling
    // the first one filled the second.
    const fields = buildInstallFields([
      declared({ key: "argument", scope: "packageArgument", index: 0, mustAsk: true }),
      declared({ key: "argument", scope: "packageArgument", index: 1, mustAsk: true }),
    ]);

    const next = setFieldValue(fields, "packageArgument", 0, "/srv/source");

    expect(next.map((f) => f.value)).toEqual(["/srv/source", ""]);
    expect(setFieldValue(next, "packageArgument", 1, "/srv/dest").map((f) => f.value)).toEqual([
      "/srv/source",
      "/srv/dest",
    ]);
  });

  it("keeps a variable of the same name independent per field", () => {
    const fields = buildInstallFields([pinnedToken({ index: 0 }), pinnedToken({ index: 1, key: "X-Fallback-Auth" })]);

    const next = setFieldVariable(fields, "header", 1, "TOKEN", "ghp_second");

    expect(next.map((f) => f.variables[0].value)).toEqual(["", "ghp_second"]);
  });
});

describe("validateInstallFields", () => {
  it("requires a value for anything the backend flagged mustAsk", () => {
    const fields = buildInstallFields([declared({ mustAsk: true, input: input({ isRequired: true }) })]);
    expect(validateInstallFields(fields)).toEqual([{ scope: "environment", index: 0, code: "required" }]);
  });

  it("accepts an empty optional field", () => {
    expect(validateInstallFields(buildInstallFields([declared()]))).toEqual([]);
  });

  it("enforces choices as a closed set", () => {
    const fields = setFieldValue(
      buildInstallFields([declared({ key: "REGION", input: input({ choices: ["us", "eu"] }) })]),
      "environment",
      0,
      "ap",
    );
    expect(validateInstallFields(fields)[0].code).toBe("notAChoice");
  });

  it("checks number and boolean formats", () => {
    const number = setFieldValue(
      buildInstallFields([declared({ key: "PORT", input: input({ format: "number" }) })]),
      "environment",
      0,
      "80a",
    );
    expect(validateInstallFields(number)[0].code).toBe("notANumber");

    const flag = setFieldValue(
      buildInstallFields([declared({ key: "DEBUG", input: input({ format: "boolean" }) })]),
      "environment",
      0,
      "yes",
    );
    expect(validateInstallFields(flag)[0].code).toBe("notABoolean");
  });

  it("defers a templated field's requiredness to its variables", () => {
    const fields = buildInstallFields([pinnedToken()]);

    expect(validateInstallFields(fields)).toEqual([{ scope: "header", index: 0, variable: "TOKEN", code: "required" }]);
    expect(validateInstallFields(setFieldVariable(fields, "header", 0, "TOKEN", "x"))).toEqual([]);
  });
});

describe("secretFieldKeys", () => {
  it("collects both secret fields and secret variables", () => {
    const fields = buildInstallFields([declared({ key: "API_KEY", input: input({ isSecret: true }) }), pinnedToken()]);
    expect(secretFieldKeys(fields)).toEqual(["API_KEY", "TOKEN"]);
  });
});

describe("buildInstallAnswers", () => {
  it("addresses every answer by scope and ordinal, never by key", () => {
    const fields = setFieldValue(
      buildInstallFields([
        declared({ key: "argument", scope: "packageArgument", index: 0, mustAsk: true }),
        declared({ key: "argument", scope: "packageArgument", index: 1, mustAsk: true }),
      ]),
      "packageArgument",
      1,
      "/dst",
    );

    expect(buildInstallAnswers(fields)).toEqual([
      { scope: "packageArgument", index: 0, value: "" },
      { scope: "packageArgument", index: 1, value: "/dst" },
    ]);
  });

  it("sends a pinned template's variables rather than the template itself", () => {
    const fields = setFieldVariable(buildInstallFields([pinnedToken()]), "header", 0, "TOKEN", "sk-x");

    expect(buildInstallAnswers(fields)).toEqual([{ scope: "header", index: 0, variable: "TOKEN", value: "sk-x" }]);
  });
});

describe("groupInstallFields", () => {
  it("groups by scope in launch-spec order and drops empty groups", () => {
    const fields = buildInstallFields([
      declared({ key: "H", scope: "header" }),
      declared({ key: "E", scope: "environment" }),
    ]);
    expect(groupInstallFields(fields).map((g) => g.scope)).toEqual(["environment", "header"]);
  });
});
