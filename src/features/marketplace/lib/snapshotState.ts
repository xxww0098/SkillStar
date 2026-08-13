//! Marketplace snapshot/error state model.
//!
//! The hook layer is non-component code and cannot call `t()`, so it never
//! produces user-facing copy. It emits structured `MarketplaceError` values
//! and the render layer maps them to i18n keys through `MARKETPLACE_ERROR_COPY`.
//! Raw backend strings (anyhow chains, SQL/HTTP details) only ever travel in
//! `detail`, which the UI shows in a collapsed secondary slot.

import type { LocalFirstResult, SnapshotStatus } from "../../../types";

/** Which local-first dataset a snapshot/error belongs to. */
export type MarketplaceScope = "leaderboard" | "publishers" | "search";

export type MarketplaceErrorKind =
  /** Backend reached the remote path and it failed; snapshot has no usable data. */
  | "remote_error"
  /** Local snapshot read failed, backend served remote data as a fallback. */
  | "error_fallback"
  /** The local-read command itself threw (IPC / DB level). */
  | "query_failed"
  /** A background or explicit snapshot refresh threw. */
  | "sync_failed"
  /** A search invocation threw. */
  | "search_failed";

export interface MarketplaceError {
  kind: MarketplaceErrorKind;
  scope: MarketplaceScope;
  /** Raw backend message — diagnostic only, never the primary user-facing copy. */
  detail: string | null;
}

export interface MarketplaceSnapshotState {
  /** `null` until the first response lands — never assert freshness we have not seen. */
  status: SnapshotStatus | null;
  updatedAt: string | null;
  error: MarketplaceError | null;
}

export const EMPTY_SNAPSHOT_STATE: MarketplaceSnapshotState = {
  status: null,
  updatedAt: null,
  error: null,
};

/** Normalize an unknown thrown value into a single diagnostic line. */
export function toErrorDetail(error: unknown): string | null {
  if (error == null) return null;
  if (error instanceof Error) return error.message || String(error);
  if (typeof error === "string") return error || null;
  return String(error);
}

/**
 * Fold a query result, a hard read failure and a refresh failure into one
 * snapshot state. Everything is derived, so a recovered request clears the
 * error automatically instead of leaving a sticky red bar behind.
 */
export function deriveSnapshotState(
  scope: MarketplaceScope,
  result: LocalFirstResult<unknown> | undefined,
  queryError: unknown,
  refreshError: MarketplaceError | null,
): MarketplaceSnapshotState {
  const status = result?.snapshot_status ?? null;
  const updatedAt = result?.snapshot_updated_at ?? null;

  // Precedence: a hard read failure hides everything else, then an explicit
  // refresh failure, then whatever the snapshot status itself reports.
  let error: MarketplaceError | null = null;
  if (queryError) {
    error = { kind: "query_failed", scope, detail: toErrorDetail(queryError) };
  } else if (refreshError) {
    error = refreshError;
  } else if (status === "remote_error") {
    error = { kind: "remote_error", scope, detail: result?.error ?? null };
  } else if (status === "error_fallback") {
    error = { kind: "error_fallback", scope, detail: result?.error ?? null };
  }

  return { status, updatedAt, error };
}

export type SnapshotSeverity = "error" | "warning";

interface ErrorCopy {
  titleKey: string;
  descriptionKey: string;
  severity: SnapshotSeverity;
}

/** Error kind → i18n keys. The only place error copy is decided. */
export const MARKETPLACE_ERROR_COPY: Record<MarketplaceErrorKind, ErrorCopy> = {
  remote_error: {
    titleKey: "marketplace.remoteErrorTitle",
    descriptionKey: "marketplace.remoteErrorDescription",
    severity: "error",
  },
  error_fallback: {
    titleKey: "marketplace.errorFallbackTitle",
    descriptionKey: "marketplace.errorFallbackDescription",
    severity: "warning",
  },
  query_failed: {
    titleKey: "marketplace.queryFailedTitle",
    descriptionKey: "marketplace.queryFailedDescription",
    severity: "error",
  },
  sync_failed: {
    titleKey: "marketplace.syncFailedTitle",
    descriptionKey: "marketplace.syncFailedDescription",
    severity: "warning",
  },
  search_failed: {
    titleKey: "marketplace.searchFailedTitle",
    descriptionKey: "marketplace.searchFailedDescription",
    severity: "error",
  },
};

/**
 * Status → toolbar label key. `fresh` and the unknown (`null`) state assert
 * nothing, so they render no label at all.
 */
export function snapshotStatusLabelKey(status: SnapshotStatus | null): string | null {
  switch (status) {
    case "seeding":
      return "marketplace.seedingSnapshot";
    case "stale":
      return "marketplace.snapshotStale";
    case "miss":
      return "marketplace.snapshotMiss";
    case "error_fallback":
      return "marketplace.snapshotErrorFallback";
    case "remote_error":
      return "marketplace.snapshotRemoteError";
    default:
      return null;
  }
}

/**
 * A status where an empty dataset means "the snapshot never landed", not
 * "the marketplace is empty" — those need their own empty state + action.
 */
export function isUnpopulatedSnapshot(status: SnapshotStatus | null): boolean {
  return status === "miss" || status === "remote_error";
}
