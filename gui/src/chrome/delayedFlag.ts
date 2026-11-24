import { effect, signal, type Signal } from "nsh";
import { DURATION_MD } from "./motion";

// One place for "wait long enough to show, then don't flicker off".
//
// Appear after 300ms. Once shown, stay at least 500ms. A spinner also
// finishes the 800ms turn it is already on, so the glyph stops upright.
// Reduce Motion is the caller's flag: the spinner gate then follows the
// request on the same tick and does not schedule a turn.
//
// nsh runs an effect cleanup on both a dependency change and unmount.
// Gates are disposed only from a zero-delay timer that the next effect
// run cancels, so a spin/flag change does not kill the coast.

/** Wait this long before a skeleton or a generic loader appears. */
export const APPEAR_MS = DURATION_MD;

/** Once visible, stay at least this long. */
export const HOLD_MS = 500;

/** One spinner revolution. Matches `SPIN_MS` in feedback.tsx. */
export const TURN_MS = 800;

/** After this, a running wait may offer the corner job card. */
export const SLOW_MS = 10_000;

export interface DelayTimer {
  now(): number;
  /** Run `fn` after `ms`. The return value cancels it. */
  at(ms: number, fn: () => void): () => void;
}

export function realTimer(): DelayTimer {
  return {
    now: () => Date.now(),
    at(ms, fn) {
      const id = setTimeout(fn, ms);
      return () => clearTimeout(id);
    },
  };
}

export interface DelayedGate {
  setActive(active: boolean): void;
  dispose(): void;
}

/**
 * Show `onShow(true)` only after `appearMs` of continuous active time.
 * Hide after `holdMs` measured from the moment it became visible.
 * Going inactive before the appear timer fires never shows.
 */
export function createDelayedGate(opts: {
  timer: DelayTimer;
  appearMs?: number;
  holdMs?: number;
  onShow: (shown: boolean) => void;
}): DelayedGate {
  const appearMs = opts.appearMs ?? APPEAR_MS;
  const holdMs = opts.holdMs ?? HOLD_MS;
  let active = false;
  let shown = false;
  let shownAt = 0;
  let dead = false;
  let cancel: (() => void) | null = null;

  const clear = () => {
    cancel?.();
    cancel = null;
  };

  const pump = () => {
    if (dead) return;
    clear();
    if (!shown) {
      if (!active) return;
      cancel = opts.timer.at(appearMs, () => {
        cancel = null;
        if (dead || !active || shown) return;
        shown = true;
        shownAt = opts.timer.now();
        opts.onShow(true);
      });
      return;
    }
    if (active) return;
    const remain = Math.max(0, holdMs - (opts.timer.now() - shownAt));
    if (remain === 0) {
      shown = false;
      opts.onShow(false);
      return;
    }
    cancel = opts.timer.at(remain, () => {
      cancel = null;
      if (dead || active || !shown) return;
      shown = false;
      opts.onShow(false);
    });
  };

  return {
    setActive(next) {
      if (dead) return;
      active = next;
      pump();
    },
    dispose() {
      dead = true;
      clear();
    },
  };
}

export interface SpinnerGate {
  setWant(want: boolean, defer: boolean, reduced: boolean): void;
  dispose(): void;
}

/**
 * `defer` waits `appearMs` before the first frame (generic loaders).
 * An action glyph passes defer false and appears on the press.
 * After the frame is up, releasing it coasts to a turn boundary that
 * is also at least `holdMs` from the reveal. `reduced` shows and hides
 * on the same call and does not spin.
 */
export function createSpinnerGate(opts: {
  timer: DelayTimer;
  appearMs?: number;
  holdMs?: number;
  turnMs?: number;
  onFrame: (visible: boolean) => void;
}): SpinnerGate {
  const appearMs = opts.appearMs ?? APPEAR_MS;
  const holdMs = opts.holdMs ?? HOLD_MS;
  const turnMs = opts.turnMs ?? TURN_MS;
  let want = false;
  let defer = false;
  let reduced = false;
  let shown = false;
  let shownAt = 0;
  let dead = false;
  let cancel: (() => void) | null = null;

  const clear = () => {
    cancel?.();
    cancel = null;
  };

  const reveal = () => {
    shown = true;
    shownAt = opts.timer.now();
    opts.onFrame(true);
  };

  const conceal = () => {
    shown = false;
    opts.onFrame(false);
  };

  // Time until the next upright stop that also satisfies the hold.
  const coastMs = (now: number): number => {
    const holdRemain = Math.max(0, holdMs - (now - shownAt));
    const into = (now - shownAt) % turnMs;
    const toBoundary = into === 0 ? 0 : turnMs - into;
    if (toBoundary >= holdRemain) return toBoundary;
    const extra = Math.ceil((holdRemain - toBoundary) / turnMs);
    return toBoundary + extra * turnMs;
  };

  const pump = () => {
    if (dead) return;
    clear();
    if (reduced) {
      if (shown !== want) {
        if (want) shownAt = opts.timer.now();
        shown = want;
        opts.onFrame(want);
      }
      return;
    }
    if (!shown) {
      if (!want) return;
      if (!defer) {
        reveal();
        return;
      }
      cancel = opts.timer.at(appearMs, () => {
        cancel = null;
        if (dead || !want || shown || reduced) return;
        reveal();
      });
      return;
    }
    if (want) return;
    const wait = coastMs(opts.timer.now());
    if (wait === 0) {
      conceal();
      return;
    }
    cancel = opts.timer.at(wait, () => {
      cancel = null;
      if (dead || want || !shown || reduced) return;
      conceal();
    });
  };

  return {
    setWant(next, nextDefer, nextReduced) {
      if (dead) return;
      want = next;
      defer = nextDefer;
      reduced = nextReduced;
      pump();
    },
    dispose() {
      dead = true;
      clear();
    },
  };
}

/** Dispose `gate` on unmount, not when the effect re-runs. */
function effectLifetime(run: () => void, onUnmount: () => void): void {
  let kill: ReturnType<typeof setTimeout> | null = null;
  effect(() => {
    if (kill) {
      clearTimeout(kill);
      kill = null;
    }
    run();
    return () => {
      kill = setTimeout(() => {
        kill = null;
        onUnmount();
      }, 0);
    };
  });
}

/**
 * A boolean that turns on after 300ms of `read()` and stays at least 500ms.
 * Call once from a component or module body. Read the signal in JSX.
 */
export function bindDelayedFlag(read: () => boolean): Signal<boolean> {
  const shown = signal(false);
  const gate = createDelayedGate({
    timer: realTimer(),
    onShow: (visible) => shown.set(visible),
  });
  effectLifetime(
    () => gate.setActive(read()),
    () => gate.dispose(),
  );
  return shown;
}

/**
 * Turns true after `ms` of continuous `read()`, and false as soon as it drops.
 * No hold: this is the "taking longer" line, not a skeleton.
 */
export function bindSlow(read: () => boolean, ms = SLOW_MS): Signal<boolean> {
  const slow = signal(false);
  let on = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const stop = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  };
  effectLifetime(
    () => {
      stop();
      if (!read()) {
        if (on) {
          on = false;
          slow.set(false);
        }
        return;
      }
      timer = setTimeout(() => {
        timer = null;
        on = true;
        slow.set(true);
      }, ms);
    },
    () => stop(),
  );
  return slow;
}
