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
/** A MySkills flow wrote a deck (Quick Pack, `agd-` share codes): the
 * keep-alive deck page reloads its list. */
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

// ---- Notify band (notify.rs parity) ------------------------------------

export type ToastTone = "info" | "success" | "warning" | "error";

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastRequest {
  tone: ToastTone;
  message: string;
  /** Notices sharing a replace key displace each other. */
  replaceKey?: string;
  /** Ignore the TTL; only the close button removes it. */
  pinned?: boolean;
  /** An inline button beside the alert; carrying one auto-pins. */
  action?: ToastAction;
}

const notifyChannel = channel<ToastRequest>();

export function subscribeNotify(handler: Handler<ToastRequest>): () => void {
  return notifyChannel.subscribe(handler);
}

/** Shorthand publishers (Notice::info / success / warning / error). */
export function toast(message: string, tone: ToastTone = "info"): void {
  notifyChannel.publish({ tone, message });
}

export function publishToast(request: ToastRequest): void {
  notifyChannel.publish(request);
}
