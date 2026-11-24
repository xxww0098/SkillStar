import type { PageId } from "./nav";

// Cross-page event bus. Pages never import each other; they publish and
// subscribe on channels here (parity with the ss-gpui shell event wiring:
// SelectPublisher / SelectPage / GroupsChanged / AgentsChanged /
// TranslationPrefsChanged are Entity events Shell subscribes to in
// shell.rs:59-127 — the nsh counterpart is this module's channels, with the
// Shell registering the same translations).

type Handler<T> = (payload: T) => void;

export interface Channel<T> {
  publish(payload: T): void;
  subscribe(handler: Handler<T>): () => void;
}

export function channel<T>(): Channel<T> {
  const handlers = new Set<Handler<T>>();
  return {
    publish(payload) {
      for (const handler of handlers) handler(payload);
    },
    subscribe(handler) {
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
  };
}

/** Market drill-down: the marketplace page emits it, Shell hops to the
 * pushed detail page (shell.rs:59-65). Publisher shape is the migration-time
 * mirror of ss_marketplace::OfficialPublisher; the marketplace wave types it
 * from the bridge. */
export interface PublisherRef {
  id: string;
  name: string;
}

/** Page-internal "go elsewhere" (empty-state CTAs, cross-page links). */
export const selectPage = channel<PageId>();
/** Marketplace → publisher detail. */
export const selectPublisher = channel<PublisherRef>();
/** The deck store changed: a MySkills flow wrote a deck (Quick Pack,
 * `agd-` share codes), or a deck page job rewrote one. A deck's rail toggle
 * also links its member Skills into Agent directories, so this channel is
 * what makes the keep-alive skills page reload its per-Skill agent links —
 * both pages subscribe and reload their own snapshot. */
export const groupsChanged = channel<void>();
/** An agent was enabled/disabled in Settings: keep-alive pages reload their
 * profile snapshots. */
export const agentsChanged = channel<void>();
/** Translation preferences were saved: keep-alive pages repaint from cache. */
export const translationPrefsChanged = channel<void>();
/** Sidebar theme toggle pressed: SettingsWorkbench persists the selection. */
export const requestThemeMode = channel<"current" | "paper">();

export interface ImportSourceRequest {
  input: string;
  autoScan: boolean;
}

export const requestImportSource = channel<ImportSourceRequest>();
/** Sidebar GitHub entry -> switch MySkills page directly into shared channels workspace. */
export const requestChannelsScope = channel<void>();

// ---- Notify band (notify.rs parity) ------------------------------------

export type ToastTone = "info" | "success" | "warning" | "error";

export interface ToastAction {
  label: string;
  run: () => void;
}

export type ToastKind = "status" | "action" | "progress";

export interface ToastRequest {
  tone: ToastTone;
  message: string;
  /** Notices sharing a replace key update the one on screen in place. */
  replaceKey?: string;
  /** No TTL while it is the current toast. A newer toast still replaces it. */
  pinned?: boolean;
  /** An inline button. It does not pin; pass `pinned` to keep the toast up. */
  action?: ToastAction;
  /** Unpinned lifetime. Success and info use 3s; warning leaves this unset (5s). */
  ttlMs?: number;
  /**
   * status sheens on arrival. action waits for the button and does not sheen.
   * progress stays while `pending` is not false, then sheens the result.
   * Omitted: an action makes it an action toast, otherwise status.
   */
  kind?: ToastKind;
  /** Progress stays up until this is false. */
  pending?: boolean;
  /** Undo action: countdown ring, then the restored line for 1.5s. */
  undo?: boolean;
}

const notifyChannel = channel<ToastRequest>();

export function subscribeNotify(handler: Handler<ToastRequest>): () => void {
  return notifyChannel.subscribe(handler);
}

/** Success and info leave on their own. Warning keeps the 5s band. */
export const TOAST_SHORT_TTL_MS = 3000;

/** Shorthand publishers (Notice::info / success / warning / error). */
export function toast(message: string, tone: ToastTone = "info"): void {
  if (tone === "error") {
    notifyChannel.publish({ tone, message, pinned: true });
    return;
  }
  notifyChannel.publish({
    tone,
    message,
    ttlMs: tone === "warning" ? undefined : TOAST_SHORT_TTL_MS,
  });
}

export function publishToast(request: ToastRequest): void {
  notifyChannel.publish(request);
}
