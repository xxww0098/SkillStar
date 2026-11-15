import { expect, open, test } from "nsh/test";
import { Settings } from "skillstar-gui/native";
import {
  RAIL_MAX_VISIBLE,
  RAIL_SLOT_GAP,
  RAIL_SLOT_SIZE,
  RAIL_VIEWPORT_W,
  railMaxScrollX,
  resolveAgentIconUri,
} from "./agentRail";

const HOST = "./agentRail.test.host.tsx";

test("carousel geometry locks 7 visible 28px slots in a 232px viewport", () => {
  expect(RAIL_MAX_VISIBLE).toBe(7);
  expect(RAIL_SLOT_SIZE).toBe(28);
  expect(RAIL_SLOT_GAP).toBe(6);
  expect(RAIL_VIEWPORT_W).toBe(232);
  expect(railMaxScrollX(0)).toBe(0);
  expect(railMaxScrollX(7)).toBe(0);
  expect(railMaxScrollX(8)).toBe(34);
  expect(railMaxScrollX(9)).toBe(68);
});

test("resolveAgentIconUri returns embedded SVG data URIs and respects custom icons", () => {
  const darkClaude = resolveAgentIconUri("claude-code", false);
  expect(darkClaude.startsWith("data:image/svg+xml;base64,")).toBe(true);
  expect(darkClaude).toBe(Settings.agentIconDataUri("claude", false));

  expect(resolveAgentIconUri("cursor", false)).not.toBe(resolveAgentIconUri("cursor", true));
  expect(resolveAgentIconUri("antigravity", false)).toBe(
    resolveAgentIconUri("antigravity", true)
  );

  const customPng = "data:image/png;base64,AAAA";
  expect(resolveAgentIconUri("custom", false, customPng)).toBe(customPng);
});

test("<= 7 slots render flat without carousel chrome; > 7 slots enable arrows without bubbling clicks", async () => {
  const app = await open(HOST, { width: 420, height: 320 });
  await app.waitUntil(() => app.shows("id:skill-rail-card-1"));

  // <= 7 slots: direct flex row, no horizontal Scroll viewport or chevron steppers.
  expect(app.shows("id:skill-slot-card-1-claude")).toBe(true);
  expect(app.shows("id:skill-slot-card-1-cursor")).toBe(true);
  expect(app.shows("id:skill-slot-card-1-windsurf")).toBe(true);
  expect(app.shows("id:skill-rail-card-1-prev")).toBe(false);
  expect(app.shows("id:skill-rail-card-1-next")).toBe(false);
  expect(app.shows("id:skill-rail-card-1-viewport")).toBe(false);

  // Clicking a slot toggles link state and must not bubble to parent-card.
  await app.press("id:skill-slot-card-1-cursor");
  await app.waitUntil(() => app.texts().includes("slots:cursor"));
  expect(app.texts()).toContain("linked:claude,cursor");
  expect(app.texts()).toContain("parent:0");

  // Switch to 9 slots: carousel viewport and prev/next arrows appear.
  await app.press("id:host-set-9");
  await app.waitUntil(() => app.shows("id:skill-rail-card-1-viewport"));
  expect(app.shows("id:skill-rail-card-1-prev")).toBe(true);
  expect(app.shows("id:skill-rail-card-1-next")).toBe(true);
  expect(app.shows("id:skill-slot-card-1-opencode")).toBe(true);
  expect(app.shows("id:skill-slot-card-1-cline")).toBe(true);

  // Disabled prev click and active next click stay inside the carousel.
  await app.press("id:skill-rail-card-1-prev");
  await app.press("id:skill-rail-card-1-next");
  await app.press("id:skill-rail-card-1-prev");
  expect(app.texts()).toContain("parent:0");

  // Slot clicks inside the scrollable carousel invoke onSlotClick, not parent-card.
  await app.press("id:skill-slot-card-1-claude");
  await app.waitUntil(() => app.texts().includes("slots:cursor,claude"));
  expect(app.texts()).toContain("linked:cursor");
  expect(app.texts()).toContain("parent:0");

  // Clicking the card body itself still opens the card.
  await app.press("id:card-body-target");
  await app.waitUntil(() => app.texts().includes("parent:1"));
});
