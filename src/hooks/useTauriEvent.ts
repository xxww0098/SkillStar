import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";

/**
 * Subscribe to a Tauri event for the lifetime of the component.
 *
 * Fixes the `listen().then((fn) => { unlisten = fn })` race: if the effect
 * is cleaned up before the subscription promise resolves, the listener is
 * detached as soon as it lands instead of leaking. The handler is kept in a
 * ref so callers don't need to memoize it and the subscription is created
 * once per event name.
 */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;

    listen<T>(event, (e) => handlerRef.current(e.payload))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((err) => {
        console.warn(`[useTauriEvent] failed to subscribe to ${event}:`, err);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event]);
}
