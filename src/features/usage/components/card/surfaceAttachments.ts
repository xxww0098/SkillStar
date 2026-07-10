export type AttachmentSurface = "grid" | "window";
export type AttachmentKind = "credits" | "manual" | "usageError" | "apiKeys" | "balance";

/** SSOT: whether an attachment kind is allowed on a given surface (owns* / hasAuto are separate). */
export const SURFACE_ATTACHMENTS = {
  grid: { credits: true, manual: true, usageError: true, apiKeys: true, balance: true },
  window: { credits: true, manual: true, usageError: true, apiKeys: false, balance: true },
} as const satisfies Record<AttachmentSurface, Record<AttachmentKind, boolean>>;

export function surfaceAllows(kind: AttachmentKind, surface: AttachmentSurface): boolean {
  return SURFACE_ATTACHMENTS[surface][kind];
}
