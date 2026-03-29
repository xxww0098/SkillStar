/**
 * Utility for generating and parsing SkillStar compressed and encrypted Share Codes.
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

const ALGO = "AES-GCM";
const DEFAULT_PASS = "skillstar-default-share-password";
const ITERATIONS = 10000;
const SALT = "agh-share-salt";

/** Derive anAES-GCM encryption/decryption key from an optional password */
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

  return "agh-" + btoa(binaryString);
}


/** 
 * Decode, decrypt and unpack the share code 
 */
export async function parseShareCode(
  code: string,
  password?: string
): Promise<ShareCodeData> {
  let cleanCode = code.trim();
  if (cleanCode.startsWith("agh-")) {
    cleanCode = cleanCode.substring(4);
  } else {
    throw new Error("分享码格式不正确 (Invalid prefix)");
  }

  // Base64 decode
  let binaryString: string;
  try {
    binaryString = atob(cleanCode);
  } catch (e) {
    throw new Error("分享码被损坏 (Base64 decode error)");
  }

  const combined = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    combined[i] = binaryString.charCodeAt(i);
  }

  // Need at least 12 IV + 1 flag + minimum AES GCM tag (16)
  if (combined.length < 29) {
    throw new Error("分享码内容过短，可能已损坏");
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
    throw new Error("加解密失败，可能是密码错误或分享码被篡改");
  }

  let bytes = new Uint8Array(decryptedBuf);

  if (isCompressed) {
    if (typeof DecompressionStream === "undefined") {
      throw new Error("当前浏览器不支持解压缩该分享码");
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
    throw new Error("分享码内部 JSON 解析失败");
  }

  if (!parsed.n || !Array.isArray(parsed.s)) {
    throw new Error("分享码数据格式不匹配");
  }

  return parsed as ShareCodeData;
}
