/**
 * Utility for generating and parsing SkillStar Share Codes.
 *
 * Two share-code flavours:
 *   • Skills sharing  →  prefix "ags-"  (file ext: .ags)
 *   • Deck sharing    →  prefix "agd-"  (file ext: .agd)
 *
 * Legacy prefix "agh-" is accepted during parsing and treated as "deck".
 *
 * Format (v2 — no encryption, with expiration):
 *   prefix + Base64(Version(1) + CompressedFlag(1) + Timestamp(8) + Payload)
 *
 * Share codes expire after TTL_EXPIRE_DAYS (default 7 days).
 */

// We use abbreviated keys for max density
export interface ShareCodeData {
  n: string; // name
  d: string; // description
  i: string; // icon
  s: {
    n: string; // skill name
    u: string; // git_url
    c?: string; // inline content (base64-encoded SKILL.md, when git_url is empty)
    p?: boolean; // true if the repo is private (requires auth to clone)
  }[];
}

export type ShareCodeType = "skills" | "deck";

export interface ParseResult {
  data: ShareCodeData;
  type: ShareCodeType;
  expiresAt: number; // unix ms
}

const PREFIX_MAP: Record<ShareCodeType, string> = {
  skills: "ags-",
  deck: "agd-",
};

const CODE_VERSION = 2;
const TTL_EXPIRE_DAYS = 7;
const TTL_MS = TTL_EXPIRE_DAYS * 24 * 60 * 60 * 1000;

/**
 * Pack data into a share code.
 * Steps: JSON → TextEncode → Deflate (if supported) → prepend header → Base64
 */
export async function createShareCode(
  data: ShareCodeData,
  type: ShareCodeType = "deck",
): Promise<string> {
  const jsonStr = JSON.stringify(data);
  let payload = new TextEncoder().encode(jsonStr);
  let isCompressed = 0;

  if (typeof CompressionStream !== "undefined") {
    try {
      const stream = new Blob([payload])
        .stream()
        .pipeThrough(new CompressionStream("deflate-raw"));
      payload = new Uint8Array(await new Response(stream).arrayBuffer());
      isCompressed = 1;
    } catch (e) {
      console.warn("CompressionStream failed, using raw data", e);
    }
  }

  const timestamp = Date.now();
  const tsBytes = new Uint8Array(new Float64Array([timestamp]).buffer);

  // Header: Version(1) + CompressedFlag(1) + Timestamp(8) = 10 bytes
  const combined = new Uint8Array(10 + payload.length);
  combined[0] = CODE_VERSION;
  combined[1] = isCompressed;
  combined.set(tsBytes, 2);
  combined.set(payload, 10);

  let binaryString = "";
  for (let i = 0; i < combined.length; i++) {
    binaryString += String.fromCharCode(combined[i]);
  }

  return PREFIX_MAP[type] + btoa(binaryString);
}

/**
 * Decode and unpack a share code.
 * Accepts ags-, agd-, and legacy agh- prefixes.
 * Throws if the code has expired.
 */
export async function parseShareCode(
  code: string,
): Promise<ParseResult> {
  let cleanCode = code.trim();
  let type: ShareCodeType;

  if (cleanCode.startsWith("ags-")) {
    type = "skills";
    cleanCode = cleanCode.substring(4);
  } else if (cleanCode.startsWith("agd-")) {
    type = "deck";
    cleanCode = cleanCode.substring(4);
  } else if (cleanCode.startsWith("agh-")) {
    type = "deck";
    cleanCode = cleanCode.substring(4);
  } else {
    throw new Error("Invalid share code prefix (expected ags- or agd-)");
  }

  let binaryString: string;
  try {
    binaryString = atob(cleanCode);
  } catch {
    throw new Error("Share code is corrupted (Base64 decode error)");
  }

  const combined = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    combined[i] = binaryString.charCodeAt(i);
  }

  if (combined.length < 11) {
    throw new Error("Share code is too short, possibly corrupted");
  }

  // Version byte reserved for future format changes
  combined[0];
  const isCompressed = combined[1] === 1;
  const tsBytes = combined.slice(2, 10);
  const payload = combined.slice(10);

  // Parse timestamp
  const timestamp = new Float64Array(tsBytes.buffer)[0];
  const expiresAt = timestamp + TTL_MS;

  if (Date.now() > expiresAt) {
    const days = Math.round((Date.now() - expiresAt) / (24 * 60 * 60 * 1000));
    throw new Error(
      `Share code expired ${days > 0 ? days + " day(s) ago" : ""} (valid for ${TTL_EXPIRE_DAYS} days)`
    );
  }

  // Decompress if needed
  let bytes = payload;
  if (isCompressed) {
    if (typeof DecompressionStream === "undefined") {
      throw new Error("Browser does not support decompression for this share code");
    }
    const stream = new Blob([bytes])
      .stream()
      .pipeThrough(new DecompressionStream("deflate-raw"));
    bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  }

  const jsonStr = new TextDecoder().decode(bytes);
  let parsed: unknown;
  try {
    parsed = JSON.parse(jsonStr);
  } catch {
    throw new Error("Share code internal JSON parse failed");
  }

  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !("n" in parsed) ||
    !Array.isArray((parsed as { s?: unknown }).s)
  ) {
    throw new Error("Share code data format mismatch");
  }

  return { data: parsed as ShareCodeData, type, expiresAt };
}

/**
 * Quick pattern check without full decoding.
 * Used for clipboard detection and smart input detection.
 */
export function looksLikeShareCode(text: string): ShareCodeType | null {
  const trimmed = text.trim();
  if (trimmed.startsWith("ags-") && trimmed.length > 30) return "skills";
  if (trimmed.startsWith("agd-") && trimmed.length > 30) return "deck";
  if (trimmed.startsWith("agh-") && trimmed.length > 30) return "deck";
  const extracted = extractShareCode(trimmed);
  if (extracted !== trimmed) {
    return looksLikeShareCode(extracted);
  }
  return null;
}

/**
 * Format a share code into a human-readable share message.
 */
export function formatShareMessage(
  data: ShareCodeData,
  code: string,
  type: ShareCodeType,
): string {
  const name = data.n || (type === "deck" ? "Skill Deck" : "Skills");
  const skillNames = data.s?.map((s) => s.n).join(", ") || "";

  const lines: string[] = [];
  lines.push(`DecksName: ${name}`);
  if (data.d) lines.push(data.d);
  if (skillNames) lines.push(`Skills: ${skillNames}`);
  lines.push("");
  lines.push(`💡 Copy this entire message to import / 复制整段消息直接粘贴导入`);
  lines.push(code);

  return lines.join("\n");
}

/**
 * Extract the raw share code from a formatted share message.
 */
export function extractShareCode(text: string): string {
  const trimmed = text.trim();
  const match = trimmed.match(/(?:ags-|agd-|agh-)[A-Za-z0-9+/=_-]+/);
  return match ? match[0] : trimmed;
}
