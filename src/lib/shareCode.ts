/**
 * Utility for generating and parsing SkillStar compressed and encrypted Share Codes.
 *
 * Two share-code flavours:
 *   • Skills sharing  →  prefix "ags-"  (file ext: .ags)
 *   • Deck sharing    →  prefix "agd-"  (file ext: .agd)
 *
 * Legacy prefix "agh-" is accepted during parsing and treated as "deck".
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
}

const PREFIX_MAP: Record<ShareCodeType, string> = {
  skills: "ags-",
  deck: "agd-",
};

const ALGO = "AES-GCM";
const DEFAULT_PASS = "skillstar-default-share-password";
const ITERATIONS = 10000;
const SALT = "agh-share-salt";

/** Derive an AES-GCM encryption/decryption key from an optional password */
async function deriveKey(password?: string): Promise<CryptoKey> {
  const encoder = new TextEncoder();
  const pass = password && password.trim().length > 0 ? password : DEFAULT_PASS;
  
  const keyMaterial = await crypto.subtle.importKey(
    "raw",
    encoder.encode(pass),
    { name: "PBKDF2" },
    false,
    ["deriveBits", "deriveKey"]
  );

  return crypto.subtle.deriveKey(
    {
      name: "PBKDF2",
      salt: encoder.encode(SALT),
      iterations: ITERATIONS,
      hash: "SHA-256",
    },
    keyMaterial,
    { name: ALGO, length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

/** 
 * Pack and encrypt the object 
 * Steps: JSON stringify -> TextEncode -> Deflate (optional/if supported) -> AES-GCM -> Base64
 */
export async function createShareCode(
  data: ShareCodeData,
  type: ShareCodeType = "deck",
  password?: string
): Promise<string> {
  const jsonStr = JSON.stringify(data);
  let bytes = new TextEncoder().encode(jsonStr);
  let isCompressed = 0;

  // Try compression if available in browser
  if (typeof CompressionStream !== "undefined") {
    try {
      const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream("deflate-raw"));
      const compressedBuffer = await new Response(stream).arrayBuffer();
      bytes = new Uint8Array(compressedBuffer);
      isCompressed = 1;
    } catch (e) {
      console.warn("CompressionStream failed, using raw data", e);
    }
  }

  const key = await deriveKey(password);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  
  const encryptedBuf = await crypto.subtle.encrypt(
    { name: ALGO, iv },
    key,
    bytes
  );

  // Pack: IV (12) + CompressedFlag (1) + Encrypted Payload
  const combined = new Uint8Array(12 + 1 + encryptedBuf.byteLength);
  combined.set(iv, 0);
  combined[12] = isCompressed;
  combined.set(new Uint8Array(encryptedBuf), 13);

  // btoa with larger arrays is safer via chunking, but array length is very small here.
  let binaryString = "";
  for (let i = 0; i < combined.length; i++) {
    binaryString += String.fromCharCode(combined[i]);
  }

  const prefix = PREFIX_MAP[type];
  return prefix + btoa(binaryString);
}


/** 
 * Decode, decrypt and unpack the share code.
 * Accepts ags-, agd-, and legacy agh- prefixes.
 */
export async function parseShareCode(
  code: string,
  password?: string
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
    // Legacy prefix → treat as deck
    type = "deck";
    cleanCode = cleanCode.substring(4);
  } else {
    throw new Error("Invalid share code prefix (expected ags- or agd-)");
  }

  // Base64 decode
  let binaryString: string;
  try {
    binaryString = atob(cleanCode);
  } catch (e) {
    throw new Error("Share code is corrupted (Base64 decode error)");
  }

  const combined = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    combined[i] = binaryString.charCodeAt(i);
  }

  // Need at least 12 IV + 1 flag + minimum AES GCM tag (16)
  if (combined.length < 29) {
    throw new Error("Share code is too short, possibly corrupted");
  }

  const iv = combined.slice(0, 12);
  const isCompressed = combined[12] === 1;
  const encryptedBytes = combined.slice(13);
  const key = await deriveKey(password);

  let decryptedBuf: ArrayBuffer;
  try {
    decryptedBuf = await crypto.subtle.decrypt(
      { name: ALGO, iv },
      key,
      encryptedBytes
    );
  } catch (e) {
    throw new Error("Decryption failed — wrong password or tampered share code");
  }

  let bytes = new Uint8Array(decryptedBuf);

  if (isCompressed) {
    if (typeof DecompressionStream === "undefined") {
      throw new Error("Browser does not support decompression for this share code");
    }
    const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
    const decompressedBuffer = await new Response(stream).arrayBuffer();
    bytes = new Uint8Array(decompressedBuffer);
  }

  const jsonStr = new TextDecoder().decode(bytes);
  let parsed: any;
  try {
    parsed = JSON.parse(jsonStr);
  } catch (e) {
    throw new Error("Share code internal JSON parse failed");
  }

  if (!parsed.n || !Array.isArray(parsed.s)) {
    throw new Error("Share code data format mismatch");
  }

  return { data: parsed as ShareCodeData, type };
}

/**
 * Quick pattern check without full decryption.
 * Used for clipboard detection and smart input detection.
 */
export function looksLikeShareCode(text: string): ShareCodeType | null {
  const trimmed = text.trim();
  if (trimmed.startsWith("ags-") && trimmed.length > 30) return "skills";
  if (trimmed.startsWith("agd-") && trimmed.length > 30) return "deck";
  if (trimmed.startsWith("agh-") && trimmed.length > 30) return "deck"; // legacy
  return null;
}
