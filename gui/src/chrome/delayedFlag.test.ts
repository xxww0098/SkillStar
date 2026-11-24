import { expect, test } from "nsh/test";
import {
  APPEAR_MS,
  HOLD_MS,
  SLOW_MS,
  TURN_MS,
  createDelayedGate,
  createSpinnerGate,
  type DelayTimer,
} from "./delayedFlag";

function fakeClock(): { timer: DelayTimer; advance(ms: number): void } {
  let now = 0;
  let seq = 1;
  const pending = new Map<number, { at: number; fn: () => void }>();
  const timer: DelayTimer = {
    now: () => now,
    at(ms, fn) {
      const id = seq++;
      pending.set(id, { at: now + ms, fn });
      return () => {
        pending.delete(id);
      };
    },
  };
  return {
    timer,
    advance(ms) {
      const target = now + ms;
      for (;;) {
        let next: { id: number; at: number; fn: () => void } | null = null;
        for (const [id, item] of pending) {
          if (item.at <= target && (next === null || item.at < next.at)) {
            next = { id, ...item };
          }
        }
        if (!next) break;
        pending.delete(next.id);
        now = next.at;
        next.fn();
      }
      now = target;
    },
  };
}

test("a delayed flag waits to appear and then holds", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createDelayedGate({
    timer: clock.timer,
    onShow: (shown) => seen.push(shown),
  });
  gate.setActive(true);
  clock.advance(APPEAR_MS - 1);
  expect(seen).toEqual([]);
  clock.advance(1);
  expect(seen).toEqual([true]);
  gate.setActive(false);
  clock.advance(HOLD_MS - 1);
  expect(seen).toEqual([true]);
  clock.advance(1);
  expect(seen).toEqual([true, false]);
});

test("a delayed flag that ends before it appears never shows", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createDelayedGate({
    timer: clock.timer,
    onShow: (shown) => seen.push(shown),
  });
  gate.setActive(true);
  clock.advance(APPEAR_MS - 1);
  gate.setActive(false);
  clock.advance(APPEAR_MS + HOLD_MS);
  expect(seen).toEqual([]);
});

test("going active again during the hold keeps the flag shown", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createDelayedGate({
    timer: clock.timer,
    onShow: (shown) => seen.push(shown),
  });
  gate.setActive(true);
  clock.advance(APPEAR_MS);
  gate.setActive(false);
  clock.advance(HOLD_MS - 1);
  gate.setActive(true);
  clock.advance(HOLD_MS);
  expect(seen).toEqual([true]);
});

test("an action spinner shows on the press and coasts one whole turn", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createSpinnerGate({
    timer: clock.timer,
    onFrame: (visible) => seen.push(visible),
  });
  gate.setWant(true, false, false);
  expect(seen).toEqual([true]);
  clock.advance(100);
  gate.setWant(false, false, false);
  clock.advance(TURN_MS - 100 - 1);
  expect(seen).toEqual([true]);
  clock.advance(1);
  expect(seen).toEqual([true, false]);
});

test("a deferred loader stays blank until the delay, then holds a whole turn", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createSpinnerGate({
    timer: clock.timer,
    onFrame: (visible) => seen.push(visible),
  });
  gate.setWant(true, true, false);
  clock.advance(APPEAR_MS - 1);
  expect(seen).toEqual([]);
  clock.advance(1);
  expect(seen).toEqual([true]);
  gate.setWant(false, true, false);
  clock.advance(TURN_MS - 1);
  expect(seen).toEqual([true]);
  clock.advance(1);
  expect(seen).toEqual([true, false]);
});

test("reduced motion follows the spinner without a coast", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createSpinnerGate({
    timer: clock.timer,
    onFrame: (visible) => seen.push(visible),
  });
  gate.setWant(true, true, true);
  expect(seen).toEqual([true]);
  gate.setWant(false, true, true);
  clock.advance(TURN_MS);
  expect(seen).toEqual([true, false]);
});

test("releasing a spinner and pressing again before the boundary keeps the frame", () => {
  const clock = fakeClock();
  const seen: boolean[] = [];
  const gate = createSpinnerGate({
    timer: clock.timer,
    onFrame: (visible) => seen.push(visible),
  });
  gate.setWant(true, false, false);
  clock.advance(100);
  gate.setWant(false, false, false);
  clock.advance(100);
  gate.setWant(true, false, false);
  clock.advance(TURN_MS);
  expect(seen).toEqual([true]);
});

test("wait constants stay on the shared durations", () => {
  expect(APPEAR_MS).toBe(300);
  expect(HOLD_MS).toBe(500);
  expect(TURN_MS).toBe(800);
  expect(SLOW_MS).toBe(10_000);
});
