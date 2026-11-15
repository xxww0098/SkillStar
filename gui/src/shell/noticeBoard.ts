import { signal } from "nsh";
import type { ToastRequest } from "./events";

// NoticeBoard core — notify.rs NoticeBoard parity, as plain state logic so
// the scheduling rules are testable without a window (the GPUI side tests
// them the same way, notify.rs:396-485):
//   - TTL 5s for unpinned notices; pinned ones only close manually;
//   - a notice carrying an action auto-pins;
//   - notices sharing a replaceKey displace each other;
//   - cap 4: the oldest CLOSING entry goes first, then the oldest unpinned;
//   - close starts a 240ms exit before removal (skipped under Reduce Motion
//     — the caller decides by passing `animate`).
//
// notify.tsx owns the single board instance and renders it; pages reach it
// through events.ts's channel.

export const NOTICE_TTL_MS = 5000;
export const NOTICE_ANIM_MS = 240;
export const NOTICE_CAP = 4;
export const NOTICE_MAX_W = 560;

export interface NoticeEntry extends ToastRequest {
  seq: number;
  pinned: boolean;
  closing: boolean;
}

export interface NoticeBoard {
  /** The live entries, newest last. */
  entries(): NoticeEntry[];
  /** Push a request; schedules `onExpire(seq)` for unpinned notices. */
  push(request: ToastRequest, onExpire: (seq: number) => void): NoticeEntry;
  /** Start (or finish) the exit for one entry. Returns true when the entry
   * exists; `remove` after `NOTICE_ANIM_MS` when `animate` is set. */
  close(seq: number, animate: boolean): boolean;
  /** Drop an entry outright (after the exit or immediately under RM). */
  remove(seq: number): boolean;
  /** Remove every notice posted with this replace key. */
  dismissReplaceKey(key: string): boolean;
}

export function createNoticeBoard(): NoticeBoard {
  const entries = signal<NoticeEntry[]>([]);
  let nextSeq = 1;

  const setEntries = (next: NoticeEntry[]) => entries.set(next);

  return {
    entries: () => entries(),
    push(request, onExpire) {
      let list = entries();
      if (request.replaceKey !== undefined) {
        list = list.filter((e) => e.replaceKey !== request.replaceKey);
      }
      while (list.length >= NOTICE_CAP) {
        const closingIx = list.findIndex((e) => e.closing);
        if (closingIx >= 0) {
          list = list.filter((_, ix) => ix !== closingIx);
          continue;
        }
        const unpinned = list.findIndex((e) => !e.pinned);
        if (unpinned < 0) break;
        list = list.filter((_, ix) => ix !== unpinned);
      }
      const entry: NoticeEntry = {
        ...request,
        pinned: request.pinned || request.action !== undefined,
        seq: nextSeq++,
        closing: false,
      };
      setEntries([...list, entry]);
      if (!entry.pinned) onExpire(entry.seq);
      return entry;
    },
    close(seq, animate) {
      const entry = entries().find((e) => e.seq === seq);
      if (!entry || entry.closing) return false;
      if (!animate) {
        this.remove(seq);
        return true;
      }
      setEntries(entries().map((e) => (e.seq === seq ? { ...e, closing: true } : e)));
      return true;
    },
    remove(seq) {
      const before = entries().length;
      setEntries(entries().filter((e) => e.seq !== seq));
      return entries().length !== before;
    },
    dismissReplaceKey(key) {
      const before = entries().length;
      setEntries(entries().filter((e) => e.replaceKey !== key));
      return entries().length !== before;
    },
  };
}
