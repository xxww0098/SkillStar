/**
 * `KEY=VALUE` block parsing for the MCP entry form.
 *
 * Extracted from `McpServerForm` to fix a silent-corruption hazard it carried:
 * the old parser ran `.trim()` over the *value* as well as the key, so an API
 * key with a leading or trailing space — which does happen, both from a
 * publisher who issues them that way and from a careless copy — was rewritten
 * on the way in. The server then rejects the credential and nothing in the UI
 * says why, because the field looks exactly right.
 *
 * The fix keeps the convenience (a pasted `KEY = value ` still does the
 * obvious thing) but gives the user a way to mean it literally, using dotenv's
 * long-established convention: a value wrapped in matching quotes is taken
 * verbatim, quotes stripped. Keys are still trimmed unconditionally — a config
 * key with an edge space is malformed under every wire format SkillStar writes.
 */

const QUOTED = /^(['"])([\s\S]*)\1$/;

/** Parse a "KEY=VALUE per line" block into a record. */
export function parseKv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split("\n")) {
    if (!raw.trim()) continue;
    const eq = raw.indexOf("=");
    if (eq < 0) continue;
    const key = raw.slice(0, eq).trim();
    if (!key) continue;
    out[key] = normalizeKvValue(raw.slice(eq + 1));
  }
  return out;
}

/**
 * Normalize one raw value: quoted → verbatim inner text, otherwise trimmed.
 * Exported because the same rule has to be explained in the field hint and
 * pinned by tests.
 */
export function normalizeKvValue(raw: string): string {
  const trimmed = raw.trim();
  const quoted = QUOTED.exec(trimmed);
  return quoted ? quoted[2] : trimmed;
}

/** True when a value can only survive a round trip if it is quoted. */
export function needsKvQuoting(value: string): boolean {
  return value !== value.trim() || QUOTED.test(value);
}

/**
 * Wrap a value so `normalizeKvValue` gives it back unchanged.
 *
 * No escape syntax on purpose — the parser has none either, and a half-honoured
 * one is worse than none. A value that contains both quote characters *and*
 * edge whitespace cannot be expressed; it is emitted bare, which round-trips to
 * the trimmed form. Rare enough to accept, and visible in the textarea rather
 * than hidden behind backslashes.
 */
function quoteKvValue(value: string): string {
  if (!value.includes('"')) return `"${value}"`;
  if (!value.includes("'")) return `'${value}'`;
  return value;
}

/** Render a record back into the textarea's `KEY=VALUE` form. */
export function kvToText(record?: Readonly<Record<string, string>> | null): string {
  if (!record) return "";
  return Object.entries(record)
    .map(([key, value]) => `${key}=${needsKvQuoting(value) ? quoteKvValue(value) : value}`)
    .join("\n");
}

/** Parse a comma/newline-separated list into a trimmed, de-duped string array. */
export function parseList(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[\n,]/)) {
    const item = raw.trim();
    if (item && !seen.has(item)) {
      seen.add(item);
      out.push(item);
    }
  }
  return out;
}
