/**
 * Centralized YAML frontmatter parsing and manipulation utilities.
 *
 * Previously duplicated across SkillEditor, SkillReader, and utils.ts.
 * All frontmatter operations must go through this module.
 */

import type { FrontmatterEntry } from "../types";

// ── Core regex & split ─────────────────────────────────────────────

export const FRONTMATTER_RE = /^\uFEFF?---\s*\r?\n([\s\S]*?)\r?\n---\s*(?:\r?\n|$)/;

export function splitFrontmatter(content: string): { frontmatter: string | null; body: string } {
  const match = content.match(FRONTMATTER_RE);
  if (!match) {
    return { frontmatter: null, body: content };
  }
  return {
    frontmatter: match[1],
    body: content.slice(match[0].length),
  };
}

// ── Read / Write helpers ───────────────────────────────────────────

export function readFrontmatterValue(frontmatter: string, key: string): string | null {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = frontmatter.match(new RegExp(`^\\s*${escapedKey}:\\s*(.+)\\s*$`, "m"));
  return match ? match[1].trim() : null;
}

export function writeFrontmatterValue(frontmatter: string, key: string, value: string): string {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const line = `${key}: ${value}`;
  const keyLineRe = new RegExp(`^\\s*${escapedKey}:\\s*.+$`, "m");
  if (keyLineRe.test(frontmatter)) {
    return frontmatter.replace(keyLineRe, line);
  }
  return `${line}\n${frontmatter}`.trim();
}

export function readDescriptionFromAnyText(text: string): string | null {
  const lineMatch = text.match(/^\s*description:\s*(.+)\s*$/m);
  if (lineMatch) {
    return lineMatch[1].trim();
  }

  // Handle collapsed single-line metadata like:
  // "name: ... description: ... user-invocable: false"
  const inlineMatch = text.match(
    /\bdescription:\s*([\s\S]*?)(?=\s+\b[a-zA-Z][a-zA-Z-]*:\s|$)/i
  );
  return inlineMatch ? inlineMatch[1].trim() : null;
}

// ── Frontmatter entry parsing ──────────────────────────────────────

/**
 * Parse frontmatter into structured key-value entries.
 * Supports block scalars (| / >) and multi-line continuation.
 */
export function parseFrontmatterEntries(frontmatter: string | null): FrontmatterEntry[] {
  if (!frontmatter) {
    return [];
  }

  const entries: FrontmatterEntry[] = [];
  const lines = frontmatter.split(/\r?\n/);
  let current: FrontmatterEntry | null = null;

  for (let i = 0; i < lines.length; i += 1) {
    const rawLine = lines[i];
    const line = rawLine.trimEnd();
    if (!line.trim()) continue;

    const keyValueMatch = line.match(/^([a-zA-Z0-9_-]+)\s*:\s*(.*)$/);
    if (keyValueMatch) {
      const rawValue = keyValueMatch[2] ?? "";
      const isBlockScalar = /^[|>][-+]?$/.test(rawValue.trim());
      let value = rawValue;

      if (isBlockScalar) {
        const blockLines: string[] = [];
        let j = i + 1;
        while (j < lines.length) {
          const next = lines[j];
          if (!next.trim()) {
            blockLines.push("");
            j += 1;
            continue;
          }
          if (/^\s+/.test(next)) {
            blockLines.push(next);
            j += 1;
            continue;
          }
          break;
        }

        const nonEmpty = blockLines.filter((l) => l.trim().length > 0);
        const minIndent = nonEmpty.length > 0
          ? Math.min(...nonEmpty.map((l) => (l.match(/^\s*/) || [""])[0].length))
          : 0;
        value = blockLines
          .map((l) => (l.trim().length > 0 ? l.slice(minIndent) : ""))
          .join("\n")
          .trimEnd();
        i = j - 1;
      }

      current = {
        key: keyValueMatch[1],
        value,
      };
      entries.push(current);
      continue;
    }

    if (current && /^\s+/.test(rawLine)) {
      current.value = `${current.value}\n${line.trim()}`;
    }
  }

  return entries;
}

/**
 * Extract just the set of keys from frontmatter (lighter than full entry parsing).
 */
export function parseFrontmatterKeys(frontmatter: string | null): Set<string> {
  if (!frontmatter) return new Set<string>();

  const keys = new Set<string>();
  for (const rawLine of frontmatter.split(/\r?\n/)) {
    const match = rawLine.trimEnd().match(/^([a-zA-Z0-9_-]+)\s*:\s*(.*)$/);
    if (match) {
      keys.add(match[1]);
    }
  }
  return keys;
}

// ── Body cleanup ───────────────────────────────────────────────────

/**
 * Strip duplicated metadata from the top of a document body when
 * the same keys already exist in frontmatter.
 */
export function stripLeadingDuplicatedMetadata(
  content: string,
  allowedKeys: ReadonlySet<string>
): string {
  if (allowedKeys.size === 0) {
    return content;
  }

  const lines = content.replace(/^\uFEFF/, "").split(/\r?\n/);
  let start = 0;
  while (start < lines.length && !lines[start].trim()) {
    start += 1;
  }
  if (start >= lines.length) {
    return content;
  }

  const keyRe = /^([a-zA-Z0-9_-]+)\s*:/;
  const firstLine = lines[start].trimStart();
  const firstKey = firstLine.match(keyRe)?.[1] ?? null;
  if (!firstKey || !allowedKeys.has(firstKey)) {
    return content;
  }

  // Collapsed one-liner metadata:
  // name: ... description: ... argument-hint: ... user-invocable: ...
  const inlineKeys = Array.from(
    firstLine.matchAll(/([a-zA-Z0-9_-]+)\s*:/g),
    (m) => m[1]
  );
  const inlineKnownCount = inlineKeys.filter((k) => allowedKeys.has(k)).length;
  if (inlineKnownCount >= 2) {
    let index = start + 1;
    while (index < lines.length && !lines[index].trim()) {
      index += 1;
    }
    return lines.slice(index).join("\n");
  }

  // Multi-line key/value metadata block at document top.
  let index = start;
  let consumed = false;
  while (index < lines.length) {
    const raw = lines[index];
    const trimmed = raw.trim();
    if (!trimmed) {
      if (consumed) {
        index += 1;
        break;
      }
      index += 1;
      continue;
    }

    const key = raw.trimStart().match(keyRe)?.[1] ?? null;
    if (key && allowedKeys.has(key)) {
      consumed = true;
      index += 1;
      continue;
    }

    if (consumed && /^\s+/.test(raw)) {
      index += 1;
      continue;
    }
    break;
  }

  if (!consumed) {
    return content;
  }
  return lines.slice(index).join("\n");
}

// ── Markdown fence unwrap ──────────────────────────────────────────

/** Unwrap an AI response that wraps content in a markdown code fence */
export function unwrapOuterMarkdownFence(text: string): string {
  const trimmed = text.trim();
  const fenced = trimmed.match(/^```(?:markdown|md)?\s*\n([\s\S]*?)\n```$/i);
  return (fenced ? fenced[1] : text).replace(/^\uFEFF/, "");
}

// ── Preview normalization ──────────────────────────────────────────

/**
 * Clean up a SKILL.md document for preview rendering:
 * unwrap fences, strip duplicated metadata from body.
 */
export function normalizeSkillMarkdownForPreview(content: string): string {
  const raw = unwrapOuterMarkdownFence(content);
  const { frontmatter, body } = splitFrontmatter(raw);
  if (!frontmatter) return raw;

  const frontmatterKeys = parseFrontmatterKeys(frontmatter);
  if (frontmatterKeys.size === 0) return raw;

  const cleanedBody = stripLeadingDuplicatedMetadata(body, frontmatterKeys);
  if (cleanedBody === body) return raw;

  return `---\n${frontmatter}\n---${cleanedBody ? `\n${cleanedBody}` : ""}`;
}

// ── Translation normalization ──────────────────────────────────────

/**
 * Merge an AI-translated document with the original's frontmatter structure.
 *
 * Handles three cases:
 * 1. AI returned proper frontmatter + body → merge, preserve original `name`
 * 2. AI returned only body with inline metadata → extract description, keep original FM
 * 3. No frontmatter in original → use translated document as-is
 */
export function normalizeTranslatedDocument(
  originalContent: string,
  translatedContent: string
): string {
  const translatedRaw = unwrapOuterMarkdownFence(translatedContent);
  const original = splitFrontmatter(originalContent);
  const translated = splitFrontmatter(translatedRaw);
  const frontmatterKeys = new Set(
    parseFrontmatterEntries(original.frontmatter).map((entry) => entry.key)
  );

  // No frontmatter: use translated document directly.
  if (!original.frontmatter) {
    return translatedRaw;
  }

  // Preferred path: AI returned frontmatter and body.
  if (translated.frontmatter) {
    let mergedFrontmatter = translated.frontmatter;
    const originalName = readFrontmatterValue(original.frontmatter, "name");
    if (originalName) {
      mergedFrontmatter = writeFrontmatterValue(mergedFrontmatter, "name", originalName);
    }
    const translatedBody = stripLeadingDuplicatedMetadata(translated.body, frontmatterKeys);
    return normalizeSkillMarkdownForPreview(
      `---\n${mergedFrontmatter}\n---${translatedBody ? `\n${translatedBody}` : ""}`
    );
  }

  // Fallback path: keep original frontmatter structure, patch translated description if present.
  const translatedDescription =
    readDescriptionFromAnyText(translatedRaw) ??
    readFrontmatterValue(original.frontmatter, "description");

  const mergedFrontmatter = translatedDescription
    ? writeFrontmatterValue(original.frontmatter, "description", translatedDescription)
    : original.frontmatter;

  const translatedBody = stripLeadingDuplicatedMetadata(translatedRaw, frontmatterKeys);
  return normalizeSkillMarkdownForPreview(
    `---\n${mergedFrontmatter}\n---${translatedBody ? `\n${translatedBody}` : ""}`
  );
}
