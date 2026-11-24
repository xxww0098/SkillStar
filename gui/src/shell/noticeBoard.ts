import { signal, untracked } from "nsh";
import type { ToastKind, ToastRequest } from "./events";

// One toast at a time. The clock is pushed in (`now`) so pause, TTL, and the
// restored hold are testable without a window. The view polls `due`.
//
//   - pinned toasts do not expire, but a newer toast still replaces them;
//   - an action does not pin (pass `pinned` to keep it);
//   - status uses `ttlMs`, else 3–5s by length; warning with no ttl is 5s;
//   - an action lasts at least 5s; progress stays while pending, then 3s;
//   - the same replaceKey keeps `seq` and bumps `generation` (in-place);
//   - any other request replaces the single slot;
//   - `hold` / `release` freeze the remainder while the pointer or focus is on it.

export const NOTICE_TTL_MS = 5000;
export const NOTICE_MAX_W = 560;
export const NOTICE_ACTION_TTL_MS = 5000;
export const NOTICE_RESTORED_MS = 1500;
export const NOTICE_PROGRESS_TTL_MS = 3000;

const STATUS_MIN_MS = 3000;
const STATUS_MAX_MS = 5000;

/** 3s at 40 characters, 5s at 120, flat outside that. */
export function statusTtlMs(message: string): number {
  const extra = Math.max(0, message.length - 40);
  const add = Math.round((Math.min(extra, 80) / 80) * (STATUS_MAX_MS - STATUS_MIN_MS));
  return STATUS_MIN_MS + add;
}

export function toastKindOf(request: ToastRequest): ToastKind {
  if (request.kind) return request.kind;
  if (request.action) return "action";
  return "status";
}

export interface NoticeEntry extends ToastRequest {
  seq: number;
  generation: number;
  pinned: boolean;
  closing: boolean;
  kind: ToastKind;
  /** Play the sheen once for this generation. */
  sheen: boolean;
  /** The view shows the restored line instead of `message`. */
  restored: boolean;
  held: boolean;
  /** Full lifetime. Null when the toast does not expire. */
  lifeMs: number | null;
  /** Epoch ms. Null while held, or when it does not expire. */
  deadline: number | null;
  /** Frozen remainder while held. */
  remainingMs: number | null;
}

export function noticeRemainingMs(entry: NoticeEntry, now: number): number {
  if (entry.lifeMs == null) return 0;
  if (entry.held) return Math.max(0, entry.remainingMs ?? entry.lifeMs);
  if (entry.deadline == null) return 0;
  return Math.max(0, entry.deadline - now);
}

export interface NoticeBoard {
  entries(): NoticeEntry[];
  current(): NoticeEntry | undefined;
  push(request: ToastRequest, now: number): NoticeEntry;
  hold(seq: number, now: number): void;
  release(seq: number, now: number): void;
  /** Seq whose deadline has passed, or null. */
  due(now: number): number | null;
  /** Swap the current toast for the restored line. 1.5s, with sheen. */
  showRestored(seq: number, now: number): boolean;
  close(seq: number, animate: boolean): boolean;
  remove(seq: number): boolean;
  dismissReplaceKey(key: string): boolean;
}

function lifeOf(request: ToastRequest, kind: ToastKind): number | null {
  if (request.pinned) return null;
  if (kind === "progress" && request.pending !== false) return null;
  if (kind === "action") {
    return Math.max(NOTICE_ACTION_TTL_MS, request.ttlMs ?? statusTtlMs(request.message));
  }
  if (kind === "progress") return request.ttlMs ?? NOTICE_PROGRESS_TTL_MS;
  if (request.ttlMs !== undefined) return request.ttlMs;
  if (request.tone === "warning") return NOTICE_TTL_MS;
  return statusTtlMs(request.message);
}

function sheenOf(request: ToastRequest, kind: ToastKind): boolean {
  if (kind === "action") return false;
  if (kind === "progress") return request.pending === false;
  return true;
}

function stamp(
  request: ToastRequest,
  now: number,
  seq: number,
  generation: number,
  held: boolean,
): NoticeEntry {
  const kind = toastKindOf(request);
  const lifeMs = lifeOf(request, kind);
  return {
    ...request,
    seq,
    generation,
    pinned: request.pinned === true,
    closing: false,
    kind,
    sheen: sheenOf(request, kind),
    restored: false,
    held,
    lifeMs,
    remainingMs: held ? lifeMs : null,
    deadline: held || lifeMs == null ? null : now + lifeMs,
  };
}

export function createNoticeBoard(): NoticeBoard {
  const entries = signal<NoticeEntry[]>([]);
  let nextSeq = 1;

  const setCurrent = (entry: NoticeEntry | undefined) => {
    entries.set(entry ? [entry] : []);
  };

  return {
    entries: () => entries(),
    current: () => entries()[0],
    push(request, now) {
      const current = untracked(entries)[0];
      const sameKey =
        current !== undefined &&
        request.replaceKey !== undefined &&
        current.replaceKey === request.replaceKey;
      if (sameKey && current) {
        const next = stamp(request, now, current.seq, current.generation + 1, current.held);
        setCurrent(next);
        return next;
      }
      const next = stamp(request, now, nextSeq++, 1, false);
      setCurrent(next);
      return next;
    },
    hold(seq, now) {
      const current = untracked(entries)[0];
      if (!current || current.seq !== seq || current.closing || current.held) return;
      const remaining = current.deadline == null ? null : Math.max(0, current.deadline - now);
      setCurrent({ ...current, held: true, remainingMs: remaining, deadline: null });
    },
    release(seq, now) {
      const current = untracked(entries)[0];
      if (!current || current.seq !== seq || !current.held) return;
      const remaining = current.remainingMs;
      setCurrent({
        ...current,
        held: false,
        remainingMs: null,
        deadline: remaining == null ? null : now + remaining,
      });
    },
    due(now) {
      const current = entries()[0];
      if (!current || current.closing || current.held || current.deadline == null) return null;
      return now >= current.deadline ? current.seq : null;
    },
    showRestored(seq, now) {
      const current = untracked(entries)[0];
      if (!current || current.seq !== seq) return false;
      setCurrent({
        ...current,
        restored: true,
        sheen: true,
        generation: current.generation + 1,
        kind: "status",
        action: undefined,
        held: false,
        lifeMs: NOTICE_RESTORED_MS,
        remainingMs: null,
        deadline: now + NOTICE_RESTORED_MS,
        closing: false,
      });
      return true;
    },
    close(seq, animate) {
      const current = untracked(entries)[0];
      if (!current || current.seq !== seq || current.closing) return false;
      if (!animate) {
        setCurrent(undefined);
        return true;
      }
      setCurrent({ ...current, closing: true, deadline: null });
      return true;
    },
    remove(seq) {
      const current = untracked(entries)[0];
      if (!current || current.seq !== seq) return false;
      setCurrent(undefined);
      return true;
    },
    dismissReplaceKey(key) {
      const current = untracked(entries)[0];
      if (!current || current.replaceKey !== key) return false;
      setCurrent(undefined);
      return true;
    },
  };
}
