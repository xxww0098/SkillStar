import { describe, expect, it } from "vitest";
import type { LocalFirstResult } from "../../../types";
import {
  deriveSnapshotState,
  isUnpopulatedSnapshot,
  type MarketplaceError,
  MARKETPLACE_ERROR_COPY,
  snapshotStatusLabelKey,
  toErrorDetail,
} from "./snapshotState";

function result(
  status: LocalFirstResult<number[]>["snapshot_status"],
  error: string | null = null,
): LocalFirstResult<number[]> {
  return { data: [], snapshot_status: status, snapshot_updated_at: "2026-01-01T00:00:00Z", error };
}

describe("toErrorDetail", () => {
  it("keeps the message of an Error and passes strings through", () => {
    expect(toErrorDetail(new Error("boom"))).toBe("boom");
    expect(toErrorDetail("raw ipc failure")).toBe("raw ipc failure");
  });

  it("collapses empty / nullish causes to null", () => {
    expect(toErrorDetail(null)).toBeNull();
    expect(toErrorDetail(undefined)).toBeNull();
    expect(toErrorDetail("")).toBeNull();
  });
});

describe("deriveSnapshotState", () => {
  it("reports an unknown status before any response lands", () => {
    expect(deriveSnapshotState("leaderboard", undefined, undefined, null)).toEqual({
      status: null,
      updatedAt: null,
      error: null,
    });
  });

  it("carries the backend error string as a diagnostic detail on remote_error", () => {
    const state = deriveSnapshotState("leaderboard", result("remote_error", "dns lookup failed"), undefined, null);
    expect(state.error).toEqual({ kind: "remote_error", scope: "leaderboard", detail: "dns lookup failed" });
  });

  it("flags error_fallback as its own kind, not a plain empty result", () => {
    const state = deriveSnapshotState(
      "publishers",
      result("error_fallback", "sqlite: disk I/O error"),
      undefined,
      null,
    );
    expect(state.error?.kind).toBe("error_fallback");
    expect(MARKETPLACE_ERROR_COPY.error_fallback.severity).toBe("warning");
  });

  it("clears the error as soon as a healthy result arrives", () => {
    expect(deriveSnapshotState("leaderboard", result("fresh"), undefined, null).error).toBeNull();
    expect(deriveSnapshotState("leaderboard", result("stale"), undefined, null).error).toBeNull();
    expect(deriveSnapshotState("leaderboard", result("miss"), undefined, null).error).toBeNull();
  });

  it("prefers a hard read failure over the snapshot status", () => {
    const refreshError: MarketplaceError = { kind: "sync_failed", scope: "leaderboard", detail: "timeout" };
    const state = deriveSnapshotState("leaderboard", result("remote_error", "x"), new Error("db locked"), refreshError);
    expect(state.error).toEqual({ kind: "query_failed", scope: "leaderboard", detail: "db locked" });
  });

  it("prefers a refresh failure over a status-derived error", () => {
    const refreshError: MarketplaceError = { kind: "sync_failed", scope: "publishers", detail: "timeout" };
    const state = deriveSnapshotState("publishers", result("remote_error", "x"), undefined, refreshError);
    expect(state.error).toBe(refreshError);
  });
});

describe("snapshotStatusLabelKey", () => {
  it("asserts nothing for fresh or unknown", () => {
    expect(snapshotStatusLabelKey("fresh")).toBeNull();
    expect(snapshotStatusLabelKey(null)).toBeNull();
  });

  it("labels every status that is not fresh", () => {
    expect(snapshotStatusLabelKey("stale")).toBe("marketplace.snapshotStale");
    expect(snapshotStatusLabelKey("seeding")).toBe("marketplace.seedingSnapshot");
    expect(snapshotStatusLabelKey("miss")).toBe("marketplace.snapshotMiss");
    expect(snapshotStatusLabelKey("error_fallback")).toBe("marketplace.snapshotErrorFallback");
    expect(snapshotStatusLabelKey("remote_error")).toBe("marketplace.snapshotRemoteError");
  });
});

describe("isUnpopulatedSnapshot", () => {
  it("only treats miss / remote_error as 'the snapshot never landed'", () => {
    expect(isUnpopulatedSnapshot("miss")).toBe(true);
    expect(isUnpopulatedSnapshot("remote_error")).toBe(true);
    expect(isUnpopulatedSnapshot("error_fallback")).toBe(false);
    expect(isUnpopulatedSnapshot("fresh")).toBe(false);
    expect(isUnpopulatedSnapshot(null)).toBe(false);
  });
});
