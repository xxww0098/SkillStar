import type { McpInput, McpInputVariable, McpInstallInput, McpInstallInputScope, McpServerEntry } from "../../../types";

/**
 * `server.json` `Input` semantics → install-form model.
 *
 * The publisher declares, per field: is it required, is it secret, what format
 * does it take, which values may it hold (`choices`), what should it be
 * prefilled with (`default`), what is a mere hint (`placeholder`), and whether
 * the value is a fixed template the end user must not edit (`value`) with
 * `{curly_brace}` holes (`variables`) that they must. This module turns that
 * into a flat list of controls plus the rules that decide when the install
 * button may unlock — the same shape VS Code's `inputs` mechanism
 * (`promptString` + `password`, `pickString` + `options`) exposes, driven by
 * the registry instead of a hand-written config block.
 *
 * Nothing here talks to the backend. `mcp_market_install_plan` supplies the
 * inputs, this builds the form, and `applyInstallFields` folds the collected
 * values back into the draft that `create_mcp_server` receives.
 */

/** One `{curly_brace}` hole inside a field's `value` template. */
export interface McpInstallFieldVariable {
  /** Token name, i.e. `TOKEN` for `{TOKEN}`. */
  name: string;
  variable: McpInputVariable;
  value: string;
}

export interface McpInstallField {
  key: string;
  scope: McpInstallInputScope;
  input: McpInput;
  /**
   * Current value. For a templated field this is the *rendered* template and is
   * never edited directly — see `variables`.
   */
  value: string;
  /**
   * The publisher pinned this field's value: the schema says the end user must
   * not edit it. Its `variables`, if any, are still the user's to fill.
   */
  templated: boolean;
  variables: McpInstallFieldVariable[];
  /** The backend's verdict: install must not proceed until this is supplied. */
  mustAsk: boolean;
}

const TEMPLATE_TOKEN = /\{([A-Za-z0-9_.-]+)\}/g;

/** Every `{curly_brace}` token in a template, in order, de-duplicated. */
export function templateTokens(value: string | null | undefined): string[] {
  if (!value) return [];
  const out: string[] = [];
  for (const match of value.matchAll(TEMPLATE_TOKEN)) {
    if (!out.includes(match[1])) out.push(match[1]);
  }
  return out;
}

/**
 * Substitute the tokens a template declares.
 *
 * Unresolved tokens are left in place rather than replaced with an empty
 * string: a url with a visible `{TENANT}` hole tells the user what is missing;
 * one silently collapsed to `https://api.example.com//mcp` does not. This
 * mirrors the backend's `resolve_url`.
 */
export function renderTemplate(template: string, values: Readonly<Record<string, string>>): string {
  return template.replace(TEMPLATE_TOKEN, (whole, token: string) => {
    const value = values[token];
    return value != null && value !== "" ? value : whole;
  });
}

function variableSeed(variable: McpInputVariable): string {
  if (variable.isRequired || variable.isSecret) return "";
  return variable.default ?? (variable.choices?.length === 1 ? variable.choices[0] : "") ?? "";
}

/**
 * Build the editable form for one install plan's inputs.
 *
 * The seed value is the plan's `prefilled` (which already applies the schema's
 * precedence: a publisher `value` wins, anything required/secret is blanked so
 * the form has to ask, otherwise `default`), so this never re-derives it.
 */
export function buildInstallFields(inputs: readonly McpInstallInput[] | null | undefined): McpInstallField[] {
  return (inputs ?? []).map((declared) => {
    const template = declared.input.value?.trim() ? declared.input.value : null;
    const tokens = templateTokens(template);
    const declaredVariables = declared.input.variables ?? {};
    return {
      key: declared.key,
      scope: declared.scope,
      input: declared.input,
      value: declared.prefilled,
      templated: template != null,
      variables: tokens.map((name) => {
        const variable: McpInputVariable = declaredVariables[name] ?? {
          isRequired: true,
          isSecret: false,
          format: "string",
        };
        return { name, variable, value: variableSeed(variable) };
      }),
      mustAsk: declared.mustAsk,
    };
  });
}

/** Replace one field's own value (non-templated fields only). */
export function setFieldValue(
  fields: readonly McpInstallField[],
  key: string,
  scope: McpInstallInputScope,
  value: string,
): McpInstallField[] {
  return fields.map((field) =>
    field.key === key && field.scope === scope && !field.templated ? { ...field, value } : field,
  );
}

/** Replace one `{curly_brace}` variable inside a templated field. */
export function setFieldVariable(
  fields: readonly McpInstallField[],
  key: string,
  scope: McpInstallInputScope,
  name: string,
  value: string,
): McpInstallField[] {
  return fields.map((field) =>
    field.key === key && field.scope === scope
      ? {
          ...field,
          variables: field.variables.map((variable) => (variable.name === name ? { ...variable, value } : variable)),
        }
      : field,
  );
}

/** The value this field contributes, with its template resolved. */
export function resolvedFieldValue(field: McpInstallField): string {
  if (!field.templated) return field.value;
  const values: Record<string, string> = {};
  for (const variable of field.variables) values[variable.name] = variable.value;
  return renderTemplate(field.value, values);
}

export type McpFieldErrorCode = "required" | "notAChoice" | "notANumber" | "notABoolean";

export interface McpFieldError {
  key: string;
  scope: McpInstallInputScope;
  /** Set when the failure belongs to one `{curly_brace}` variable. */
  variable?: string;
  code: McpFieldErrorCode;
}

function checkFormat(
  format: McpInput["format"],
  choices: readonly string[] | undefined,
  value: string,
): McpFieldErrorCode | null {
  if (choices && choices.length > 0 && !choices.includes(value)) return "notAChoice";
  if (format === "number" && !/^-?\d+(\.\d+)?$/.test(value)) return "notANumber";
  if (format === "boolean" && value !== "true" && value !== "false") return "notABoolean";
  return null;
}

/**
 * Validate the collected values.
 *
 * Requiredness is checked against the backend's `mustAsk` (which already
 * encodes "required or secret, and the publisher did not pin a value") rather
 * than re-deriving it from `isRequired`; a templated field defers its
 * requiredness to its variables, since the template itself is not editable.
 * `format`/`choices` are only checked once a value exists — an empty optional
 * field is not a malformed one.
 */
export function validateInstallFields(fields: readonly McpInstallField[]): McpFieldError[] {
  const errors: McpFieldError[] = [];
  for (const field of fields) {
    if (field.templated) {
      for (const { name, variable, value } of field.variables) {
        if (!value.trim()) {
          if (variable.isRequired || variable.isSecret) {
            errors.push({ key: field.key, scope: field.scope, variable: name, code: "required" });
          }
          continue;
        }
        const code = checkFormat(variable.format, variable.choices, value);
        if (code) errors.push({ key: field.key, scope: field.scope, variable: name, code });
      }
      continue;
    }

    if (!field.value.trim()) {
      if (field.mustAsk) errors.push({ key: field.key, scope: field.scope, code: "required" });
      continue;
    }
    const code = checkFormat(field.input.format, field.input.choices, field.value);
    if (code) errors.push({ key: field.key, scope: field.scope, code });
  }
  return errors;
}

/** Keys the publisher marked secret — masked in the form, never in a preview. */
export function secretFieldKeys(fields: readonly McpInstallField[]): string[] {
  const keys: string[] = [];
  for (const field of fields) {
    if (field.input.isSecret && !keys.includes(field.key)) keys.push(field.key);
    for (const { name, variable } of field.variables) {
      if (variable.isSecret && !keys.includes(name)) keys.push(name);
    }
  }
  return keys;
}

/**
 * Splice one argument's collected value into the launch args.
 *
 * The backend flattens `runtimeArguments[]` / `packageArguments[]` into `args`
 * *without* emitting placeholders, so a field the user still has to fill left
 * no slot behind. Named arguments therefore land right after their own flag
 * when the flag is already present (and as `flag value` appended when it is
 * not); positional arguments append. This is the one place where the frontend
 * reconstructs part of the command line, which is exactly why the confirmation
 * step re-renders the full command from the final args rather than trusting the
 * plan's precomputed preview.
 */
export function spliceArgument(args: readonly string[], key: string, value: string): string[] {
  if (!value) return [...args];
  const flagIndex = args.indexOf(key);
  if (flagIndex >= 0) {
    const next = [...args];
    next.splice(flagIndex + 1, 0, value);
    return next;
  }
  // A key that looks like a flag is a named argument whose flag was dropped
  // (blank value); anything else is the publisher's `valueHint` for a
  // positional, which must never itself reach the command line.
  return key.startsWith("-") ? [...args, key, value] : [...args, value];
}

export interface McpInstallDraftInput {
  draft: McpServerEntry;
  fields: readonly McpInstallField[];
}

/**
 * Fold the collected values into the draft `create_mcp_server` will receive.
 *
 * Each scope writes exactly the part of the launch spec it belongs to; nothing
 * is inferred from the field name. Empty values are dropped rather than written
 * as `""`, so an untouched optional env var does not end up pinning an empty
 * string into every tool's config.
 */
export function applyInstallFields({ draft, fields }: McpInstallDraftInput): McpServerEntry {
  const env: Record<string, string> = { ...(draft.env ?? {}) };
  const headers: Record<string, string> = { ...(draft.headers ?? {}) };
  let args = [...(draft.args ?? [])];
  let url = draft.url ?? null;

  for (const field of fields) {
    const value = resolvedFieldValue(field);
    switch (field.scope) {
      case "environment":
        if (value) env[field.key] = value;
        else delete env[field.key];
        break;
      case "header":
        if (value) headers[field.key] = value;
        else delete headers[field.key];
        break;
      case "urlVariable":
        if (url && value) url = renderTemplate(url, { [field.key]: value });
        break;
      case "runtimeArgument":
      case "packageArgument":
        if (field.mustAsk || !field.templated) args = spliceArgument(args, field.key, value);
        break;
    }
  }

  return {
    ...draft,
    env,
    headers,
    args,
    url,
  };
}

/** Group fields for rendering — one section per part of the launch spec. */
export const INSTALL_SCOPE_ORDER: readonly McpInstallInputScope[] = [
  "environment",
  "header",
  "urlVariable",
  "runtimeArgument",
  "packageArgument",
];

export function groupInstallFields(
  fields: readonly McpInstallField[],
): Array<{ scope: McpInstallInputScope; fields: McpInstallField[] }> {
  return INSTALL_SCOPE_ORDER.map((scope) => ({
    scope,
    fields: fields.filter((field) => field.scope === scope),
  })).filter((group) => group.fields.length > 0);
}
