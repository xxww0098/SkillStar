import { expect, test } from "nsh/test";
import {
  DURATION_LG,
  DURATION_MD,
  DURATION_PRESS,
  DURATION_REDUCED,
  DURATION_SETTLE,
  DURATION_SM,
  DURATION_SM_OUT,
  DURATION_STRETCH,
  EASE_IN,
  EASE_IN_OUT,
  EASE_OUT,
  EASE_OUT_STRONG,
  EASE_OVERSHOOT,
  EASE_SPRING,
  MOTION_MS,
  settleTransition,
  springTransition,
} from "./motion";

test("motion tokens are the shared curves and durations", () => {
  expect(EASE_OUT).toBe("cubic-bezier(0, 0, .58, 1)");
  expect(EASE_OUT_STRONG).toBe("cubic-bezier(.22, 1, .36, 1)");
  expect(EASE_IN).toBe("cubic-bezier(.42, 0, 1, 1)");
  expect(EASE_IN_OUT).toBe("cubic-bezier(.42, 0, .58, 1)");
  expect(EASE_OVERSHOOT).toBe("cubic-bezier(.34, 1.56, .64, 1)");
  expect(EASE_SPRING).toBe("spring(1480, 46)");
  expect(DURATION_SM).toBe(200);
  expect(DURATION_MD).toBe(300);
  expect(DURATION_LG).toBe(400);
  expect(DURATION_SM_OUT).toBeGreaterThanOrEqual(120);
  expect(DURATION_SM_OUT).toBeLessThanOrEqual(200);
  expect(DURATION_SM_OUT).toBeLessThan(DURATION_SM);
  expect(DURATION_PRESS).toBeGreaterThanOrEqual(100);
  expect(DURATION_PRESS).toBeLessThanOrEqual(150);
  expect(DURATION_REDUCED).toBe(150);
  expect(DURATION_SETTLE).toBe(Math.round(DURATION_SM * 1.5));
  expect(DURATION_STRETCH).toBeGreaterThan(MOTION_MS);
  expect(MOTION_MS).toBe(DURATION_SM);
});

test("settleTransition and springTransition emit native spring styles", () => {
  expect(settleTransition()).toEqual({
    transitionProperty: "transform",
    transitionTimingFunction: EASE_SPRING,
  });
  expect(springTransition("opacity", "spring(620, 36)")).toEqual({
    transitionProperty: "opacity",
    transitionTimingFunction: "spring(620, 36)",
  });
});
