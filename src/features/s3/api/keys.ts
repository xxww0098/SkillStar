/** Query-key factory for the S3 sync feature. Every TanStack Query key must
 * come from here so invalidation stays consistent. */
export const s3Keys = {
  all: ["s3"] as const,
  targets: () => [...s3Keys.all, "targets"] as const,
  manifest: (targetId: string | null) => [...s3Keys.all, "manifest", targetId ?? ""] as const,
};
