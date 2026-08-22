import type {
  McpInput,
  McpInputVariable,
  McpInstallAnswer,
  McpInstallInput,
  McpInstallInputScope,
} from "../../../types";

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
 * Nothing here assembles a launch spec. `mcp_market_install_plan` supplies the
 * inputs, this builds the form and validates it for instant feedback, and
 * `buildInstallAnswers` hands the collected values back to
 * `mcp_market_install_preview` — which owns the one derivation of
 * answers → entry → command line.
 */

/** One `{curly_brace}` hole inside a field's `value` template. */
export interface McpInstallFieldVariable {
  /** Token name, i.e. `TOKEN` for `{TOKEN}`. */
  name: string;
  variable: McpInputVariable;
  value: string;
}

export interface McpInstallField {
  /**
   * The publisher's label for this field — shown, never used to find it. Two
   * positional arguments with no name and no `valueHint` share the key
   * `"argument"`, so `(scope, index)` is the identity.
   */
  key: string;
  scope: McpInstallInputScope;
  /** Position within `scope`, straight from the install plan. */
  index: number;
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

/**
 * Build the editable form for one install plan's inputs.
 *
 * Every seed comes from the plan — `prefilled` for the field, and the already
 * resolved `variables` list for a pinned template — so neither the schema's
 * precedence rules nor the `{curly_brace}` scan is duplicated here.
 */
export function buildInstallFields(inputs: readonly McpInstallInput[] | null | undefined): McpInstallField[] {
  return (inputs ?? []).map((declared) => ({
    key: declared.key,
    scope: declared.scope,
    index: declared.index,
    input: declared.input,
    value: declared.prefilled,
    templated: Boolean(declared.input.value?.trim()),
    variables: (declared.variables ?? []).map(({ name, variable, prefilled }) => ({
      name,
      variable,
      value: prefilled,
    })),
    mustAsk: declared.mustAsk,
  }));
}

/** Replace one field's own value (non-templated fields only). */
export function setFieldValue(
  fields: readonly McpInstallField[],
  scope: McpInstallInputScope,
  index: number,
  value: string,
): McpInstallField[] {
  return fields.map((field) =>
    field.scope === scope && field.index === index && !field.templated ? { ...field, value } : field,
  );
}

/** Replace one `{curly_brace}` variable inside a templated field. */
export function setFieldVariable(
  fields: readonly McpInstallField[],
  scope: McpInstallInputScope,
  index: number,
  name: string,
  value: string,
): McpInstallField[] {
  return fields.map((field) =>
    field.scope === scope && field.index === index
      ? {
          ...field,
          variables: field.variables.map((variable) => (variable.name === name ? { ...variable, value } : variable)),
        }
      : field,
  );
}

/**
 * The collected values, addressed the way the install plan numbered them.
 *
 * A pinned template contributes its `{curly_brace}` answers rather than itself:
 * the backend re-reads the template from the catalog row, and the user is not
 * allowed to edit it anyway.
 */
export function buildInstallAnswers(fields: readonly McpInstallField[]): McpInstallAnswer[] {
  return fields.flatMap((field) =>
    field.templated
      ? field.variables.map((variable) => ({
          scope: field.scope,
          index: field.index,
          variable: variable.name,
          value: variable.value,
        }))
      : [{ scope: field.scope, index: field.index, value: field.value }],
  );
}

export type McpFieldErrorCode = "required" | "notAChoice" | "notANumber" | "notABoolean";

export interface McpFieldError {
  scope: McpInstallInputScope;
  /** Ordinal within `scope` — the same identity the setters take. */
  index: number;
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
            errors.push({ scope: field.scope, index: field.index, variable: name, code: "required" });
          }
          continue;
        }
        const code = checkFormat(variable.format, variable.choices, value);
        if (code) errors.push({ scope: field.scope, index: field.index, variable: name, code });
      }
      continue;
    }

    if (!field.value.trim()) {
      if (field.mustAsk) errors.push({ scope: field.scope, index: field.index, code: "required" });
      continue;
    }
    const code = checkFormat(field.input.format, field.input.choices, field.value);
    if (code) errors.push({ scope: field.scope, index: field.index, code });
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
