import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Return Tailwind size classes for an agent icon.
 * PNG icons have built-in rounded-square chrome and need a
 * larger render size to match the visual weight of bare SVG icons.
 *
 * @param icon  - profile.icon path
 * @param base  - base size for SVG icons, default "w-3.5 h-3.5"
 */
export function agentIconCls(icon: string, base = "w-3.5 h-3.5"): string {
  if (icon.endsWith(".png")) {
    // Bump one step: 3.5→5, 4→5, 5→6
    return base
      .replace(/w-3\.5/, "w-5").replace(/h-3\.5/, "h-5")
      .replace(/w-4\b/, "w-5").replace(/h-4\b/, "h-5");
  }
  return base;
}

/** Format install count: 759100 → "759.1K", 1200000 → "1.2M" */
export function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
  return count.toLocaleString();
}

/** Unwrap an AI response that wraps content in a markdown code fence */
export function unwrapOuterMarkdownFence(text: string): string {
  const trimmed = text.trim();
  const fenced = trimmed.match(/^```(?:markdown|md)?\s*\n([\s\S]*?)\n```$/i);
  return (fenced ? fenced[1] : text).replace(/^\uFEFF/, "");
}

const FRONTMATTER_RE = /^\uFEFF?---\s*\r?\n([\s\S]*?)\r?\n---\s*(?:\r?\n|$)/;

function splitFrontmatter(content: string): { frontmatter: string | null; body: string } {
  const match = content.match(FRONTMATTER_RE);
  if (!match) {
    return { frontmatter: null, body: content };
  }
  return {
    frontmatter: match[1],
    body: content.slice(match[0].length),
  };
}

function parseFrontmatterKeys(frontmatter: string | null): Set<string> {
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

function stripLeadingDuplicatedMetadata(
  content: string,
  allowedKeys: ReadonlySet<string>
): string {
  if (allowedKeys.size === 0) return content;

  const lines = content.replace(/^\uFEFF/, "").split(/\r?\n/);
  let start = 0;
  while (start < lines.length && !lines[start].trim()) {
    start += 1;
  }
  if (start >= lines.length) return content;

  const keyRe = /^([a-zA-Z0-9_-]+)\s*:/;
  const firstLine = lines[start].trimStart();
  const firstKey = firstLine.match(keyRe)?.[1] ?? null;
  if (!firstKey || !allowedKeys.has(firstKey)) {
    return content;
  }

  const inlineKeys = Array.from(
    firstLine.matchAll(/([a-zA-Z0-9_-]+)\s*:/g),
    (match) => match[1]
  );
  const inlineKnownCount = inlineKeys.filter((key) => allowedKeys.has(key)).length;
  if (inlineKnownCount >= 2) {
    let index = start + 1;
    while (index < lines.length && !lines[index].trim()) {
      index += 1;
    }
    return lines.slice(index).join("\n");
  }

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

  return consumed ? lines.slice(index).join("\n") : content;
}

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

/** Navigate to AI settings page via custom event */
export function navigateToAiSettings() {
  try {
    localStorage.setItem("skillstar:settings-focus", "ai-provider");
  } catch {
    // ignore localStorage access errors
  }
  window.dispatchEvent(
    new CustomEvent("skillstar:navigate", { detail: { page: "settings" } })
  );
}

type Translator = (key: string, options?: Record<string, unknown>) => string;

export function formatAiErrorMessage(error: string | null | undefined, t: Translator): string | null {
  if (!error) return null;
  const msg = String(error).trim();
  const lower = msg.toLowerCase();

  if (
    lower.includes("ai provider is disabled") ||
    lower.includes("ai provider is not configured") ||
    lower.includes("api key is empty")
  ) {
    return t("skillEditor.aiNotConfigured", {
      defaultValue: "AI is not configured. Please configure it in Settings.",
    });
  }

  if (lower.includes("mymemory")) {
    return t("detailPanel.mymemoryUnavailable", {
      defaultValue: "MyMemory translation is unavailable right now. Please try again later.",
    });
  }

  if (
    lower.includes("failed to send request") ||
    lower.includes("timed out") ||
    lower.includes("connection") ||
    lower.includes("dns")
  ) {
    return t("detailPanel.networkError", {
      defaultValue: "Network request failed. Please check your network or proxy settings.",
    });
  }

  return msg;
}
