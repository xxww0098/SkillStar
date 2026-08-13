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
  templateTokens,
  validateInstallFields,
} from "./installForm";

function input(patch: Partial<McpInput> = {}): McpInput {
  return { isRequired: false, isSecret: false, format: "string", ...patch };
}

function declared(patch: Partial<McpInstallInput> = {}): McpInstallInput {
  return {
    key: "API_KEY",
    scope: "environment",
    input: input(),
    prefilled: "",
    mustAsk: false,
    ...patch,
  };
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

describe("templateTokens / renderTemplate", () => {
  it("finds each token once, in order", () => {
    expect(templateTokens("Bearer {TOKEN} for {TENANT} and {TOKEN}")).toEqual(["TOKEN", "TENANT"]);
    expect(templateTokens(null)).toEqual([]);
  });

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

  it("treats a publisher-pinned value as a template with a variable sub-form", () => {
    const fields = buildInstallFields([
      declared({
        key: "Authorization",
        scope: "header",
        prefilled: "Bearer {TOKEN}",
        input: input({
          value: "Bearer {TOKEN}",
          variables: { TOKEN: { isRequired: true, isSecret: true, format: "string" } },
        }),
      }),
    ]);

    expect(fields[0].templated).toBe(true);
    expect(fields[0].variables).toEqual([
      { name: "TOKEN", variable: { isRequired: true, isSecret: true, format: "string" }, value: "" },
    ]);
  });

  it("invents a required string variable for an undeclared token", () => {
    const fields = buildInstallFields([
      declared({ prefilled: "{UNDECLARED}", input: input({ value: "{UNDECLARED}" }) }),
    ]);
    expect(fields[0].variables[0].variable).toEqual({ isRequired: true, isSecret: false, format: "string" });
  });

  it("prefills an optional variable from its default", () => {
    const fields = buildInstallFields([
      declared({
        prefilled: "{REGION}",
        input: input({
          value: "{REGION}",
          variables: { REGION: { isRequired: false, isSecret: false, format: "string", default: "us" } },
        }),
      }),
    ]);
    expect(fields[0].variables[0].value).toBe("us");
  });

  it("returns an empty form for a plan with no inputs", () => {
    expect(buildInstallFields(undefined)).toEqual([]);
  });
});

describe("editing", () => {
  it("refuses to edit a pinned value directly", () => {
    const fields = buildInstallFields([declared({ prefilled: "fixed", input: input({ value: "fixed" }) })]);
    expect(setFieldValue(fields, "API_KEY", "environment", "hacked")[0].value).toBe("fixed");
  });

  it("edits a variable inside a pinned value", () => {
    const fields = buildInstallFields([
      declared({
        scope: "header",
        key: "Authorization",
        prefilled: "Bearer {TOKEN}",
        input: input({ value: "Bearer {TOKEN}", isSecret: true }),
      }),
    ]);
    const next = setFieldVariable(fields, "Authorization", "header", "TOKEN", "ghp_x");

    expect(resolvedFieldValue(next[0])).toBe("Bearer ghp_x");
  });

  it("only touches the field with the matching key and scope", () => {
    const fields = buildInstallFields([
      declared({ key: "TOKEN", scope: "environment" }),
      declared({ key: "TOKEN", scope: "header" }),
    ]);
    const next = setFieldValue(fields, "TOKEN", "header", "x");

    expect(next.map((f) => f.value)).toEqual(["", "x"]);
  });
});

describe("validateInstallFields", () => {
  it("requires a value for anything the backend flagged mustAsk", () => {
    const fields = buildInstallFields([declared({ mustAsk: true, input: input({ isRequired: true }) })]);
    expect(validateInstallFields(fields)).toEqual([{ key: "API_KEY", scope: "environment", code: "required" }]);
  });

  it("accepts an empty optional field", () => {
    expect(validateInstallFields(buildInstallFields([declared()]))).toEqual([]);
  });

  it("enforces choices as a closed set", () => {
    const fields = setFieldValue(
      buildInstallFields([declared({ key: "REGION", input: input({ choices: ["us", "eu"] }) })]),
      "REGION",
      "environment",
      "ap",
    );
    expect(validateInstallFields(fields)[0].code).toBe("notAChoice");
  });

  it("checks number and boolean formats", () => {
    const number = setFieldValue(
      buildInstallFields([declared({ key: "PORT", input: input({ format: "number" }) })]),
      "PORT",
      "environment",
      "80a",
    );
    expect(validateInstallFields(number)[0].code).toBe("notANumber");

    const flag = setFieldValue(
      buildInstallFields([declared({ key: "DEBUG", input: input({ format: "boolean" }) })]),
      "DEBUG",
      "environment",
      "yes",
    );
    expect(validateInstallFields(flag)[0].code).toBe("notABoolean");
  });

  it("defers a templated field's requiredness to its variables", () => {
    const fields = buildInstallFields([
      declared({
        key: "Authorization",
        scope: "header",
        mustAsk: false,
        prefilled: "Bearer {TOKEN}",
        input: input({
          value: "Bearer {TOKEN}",
          variables: { TOKEN: { isRequired: true, isSecret: true, format: "string" } },
        }),
      }),
    ]);

    expect(validateInstallFields(fields)).toEqual([
      { key: "Authorization", scope: "header", variable: "TOKEN", code: "required" },
    ]);
    expect(validateInstallFields(setFieldVariable(fields, "Authorization", "header", "TOKEN", "x"))).toEqual([]);
  });
});

describe("secretFieldKeys", () => {
  it("collects both secret fields and secret variables", () => {
    const fields = buildInstallFields([
      declared({ key: "API_KEY", input: input({ isSecret: true }) }),
      declared({
        key: "Authorization",
        scope: "header",
        prefilled: "Bearer {TOKEN}",
        input: input({
          value: "Bearer {TOKEN}",
          variables: { TOKEN: { isRequired: true, isSecret: true, format: "string" } },
        }),
      }),
    ]);
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
      setFieldValue(buildInstallFields([declared({ key: "API_KEY" })]), "API_KEY", "environment", "sk-x")[0],
      setFieldValue(
        buildInstallFields([declared({ key: "Authorization", scope: "header" })]),
        "Authorization",
        "header",
        "Bearer y",
      )[0],
      setFieldValue(
        buildInstallFields([declared({ key: "--port", scope: "packageArgument", mustAsk: true })]),
        "--port",
        "packageArgument",
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
      "TENANT",
      "urlVariable",
      "acme",
    );
    const entry = applyInstallFields({ draft: { ...draft, url: "https://{TENANT}.example.com/mcp" }, fields });

    expect(entry.url).toBe("https://acme.example.com/mcp");
  });

  it("drops an emptied value instead of pinning an empty string into every tool config", () => {
    const seeded = buildInstallFields([declared({ key: "PORT", prefilled: "50325" })]);
    const entry = applyInstallFields({
      draft: { ...draft, env: { PORT: "50325" } },
      fields: setFieldValue(seeded, "PORT", "environment", ""),
    });

    expect(entry.env).toEqual({});
  });

  it("leaves the draft object itself untouched", () => {
    const before = JSON.stringify(draft);
    applyInstallFields({
      draft,
      fields: setFieldValue(buildInstallFields([declared()]), "API_KEY", "environment", "x"),
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
