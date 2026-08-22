import { describe, expect, it } from "vitest";
import type { McpInput, McpInstallInput, McpServerEntry } from "../../../types";
import {
  applyInstallFields,
  buildInstallFields,
  groupInstallFields,
  renderTemplate,
  resolvedFieldValue,
  secretFieldKeys,
  setFieldValue,
  setFieldVariable,
  spliceArgument,
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

const draft: McpServerEntry = {
  id: "",
  name: "server",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@acme/server"],
  env: {},
  headers: {},
  enabled: {},
  autoApproveAll: false,
  sortIndex: 0,
};

describe("renderTemplate", () => {
  it("leaves unresolved tokens visible instead of collapsing them", () => {
    // A url with a visible {TENANT} hole says what is missing; one silently
    // emptied to https://api.example.com//mcp does not.
    expect(renderTemplate("https://{TENANT}.example.com/mcp", {})).toBe("https://{TENANT}.example.com/mcp");
    expect(renderTemplate("https://{TENANT}.example.com/mcp", { TENANT: "acme" })).toBe("https://acme.example.com/mcp");
  });
});

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

    expect(resolvedFieldValue(next[0])).toBe("Bearer ghp_x");
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

    expect(next.map((f) => resolvedFieldValue(f))).toEqual(["Bearer {TOKEN}", "Bearer ghp_second"]);
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

describe("spliceArgument", () => {
  it("inserts a named argument's value right after its flag", () => {
    expect(spliceArgument(["-y", "@acme/server", "--port"], "--port", "8080")).toEqual([
      "-y",
      "@acme/server",
      "--port",
      "8080",
    ]);
  });

  it("appends flag and value when the flag was dropped", () => {
    expect(spliceArgument(["-y", "@acme/server"], "--port", "8080")).toEqual(["-y", "@acme/server", "--port", "8080"]);
  });

  it("never puts a positional argument's value hint on the command line", () => {
    expect(spliceArgument(["-y", "@acme/server"], "PATH_TO_DIRECTORY", "/srv")).toEqual(["-y", "@acme/server", "/srv"]);
  });

  it("adds nothing for an empty value", () => {
    expect(spliceArgument(["-y"], "--port", "")).toEqual(["-y"]);
  });
});

describe("applyInstallFields", () => {
  it("routes each scope to the part of the launch spec it belongs to", () => {
    const fields = [
      setFieldValue(buildInstallFields([declared({ key: "API_KEY" })]), "environment", 0, "sk-x")[0],
      setFieldValue(
        buildInstallFields([declared({ key: "Authorization", scope: "header" })]),
        "header",
        0,
        "Bearer y",
      )[0],
      setFieldValue(
        buildInstallFields([declared({ key: "--port", scope: "packageArgument", mustAsk: true })]),
        "packageArgument",
        0,
        "8080",
      )[0],
    ];

    const entry = applyInstallFields({ draft: { ...draft, url: "https://{TENANT}.example.com" }, fields });

    expect(entry.env).toEqual({ API_KEY: "sk-x" });
    expect(entry.headers).toEqual({ Authorization: "Bearer y" });
    expect(entry.args).toEqual(["-y", "@acme/server", "--port", "8080"]);
  });

  it("substitutes url variables into the endpoint", () => {
    const fields = setFieldValue(
      buildInstallFields([declared({ key: "TENANT", scope: "urlVariable", mustAsk: true })]),
      "urlVariable",
      0,
      "acme",
    );
    const entry = applyInstallFields({ draft: { ...draft, url: "https://{TENANT}.example.com/mcp" }, fields });

    expect(entry.url).toBe("https://acme.example.com/mcp");
  });

  it("drops an emptied value instead of pinning an empty string into every tool config", () => {
    const seeded = buildInstallFields([declared({ key: "PORT", prefilled: "50325" })]);
    const entry = applyInstallFields({
      draft: { ...draft, env: { PORT: "50325" } },
      fields: setFieldValue(seeded, "environment", 0, ""),
    });

    expect(entry.env).toEqual({});
  });

  it("leaves the draft object itself untouched", () => {
    const before = JSON.stringify(draft);
    applyInstallFields({
      draft,
      fields: setFieldValue(buildInstallFields([declared()]), "environment", 0, "x"),
    });
    expect(JSON.stringify(draft)).toBe(before);
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
