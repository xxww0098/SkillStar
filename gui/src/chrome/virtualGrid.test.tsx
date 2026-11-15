import { expect, open, test } from "nsh/test";
import { CARD_ROW_W, DETAIL_COLUMN_W, PAGE_PAD, SKILL_CARD_W, WINDOW_H, WINDOW_W } from "../layout";
import { cardTrackWidth, columnsFor, gridColumnsForPane, packCardRows } from "./cardShell";

const HOST = "./virtualGrid.test.host.tsx";

test("columnsFor, gridColumnsForPane, cardTrackWidth, and packCardRows preserve grid packing invariants", () => {
  expect(columnsFor(SKILL_CARD_W - 1)).toBe(1);
  expect(columnsFor(CARD_ROW_W)).toBe(3);
  expect(columnsFor(CARD_ROW_W - DETAIL_COLUMN_W)).toBe(2);

  // paneWidth === 0 falls back cleanly before initial onLayout measurement.
  expect(gridColumnsForPane(0, false, 0, CARD_ROW_W)).toBe(3);
  expect(gridColumnsForPane(0, false, 0, CARD_ROW_W - DETAIL_COLUMN_W)).toBe(2);
  expect(gridColumnsForPane(1280, true, 20, CARD_ROW_W)).toBe(1);

  // Responsive column track width matches 336px at default pane widths and expands flush elsewhere.
  expect(cardTrackWidth(CARD_ROW_W + PAGE_PAD * 2, false, PAGE_PAD)).toBe(SKILL_CARD_W);
  expect(cardTrackWidth(CARD_ROW_W - DETAIL_COLUMN_W + PAGE_PAD * 2, false, PAGE_PAD)).toBe(SKILL_CARD_W);
  expect(cardTrackWidth(960, false, PAGE_PAD)).toBe(452);
  expect(cardTrackWidth(1260, false, PAGE_PAD)).toBe(396);
  expect(cardTrackWidth(1260, true, PAGE_PAD)).toBe(1220);

  expect(packCardRows([1, 2, 3, 4, 5, 6, 7], 3)).toEqual([[1, 2, 3], [4, 5, 6], [7]]);
  expect(packCardRows([], 3)).toEqual([]);
});

test("MarketBoard, PublisherBoardPane, and SkillCard/RepoAvatar virtualize 90-item datasets", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });

  // 1. MarketBoard: head cards mounted, tail card #89 virtualized out across viewports.
  await app.waitUntil(() => app.shows("id:mk-card-skill-00"));
  expect(app.shows("id:mk-card-skill-89")).toBe(false);

  await app.setViewport(960, WINDOW_H);
  await app.waitUntil(() => app.shows("id:mk-card-skill-00"));
  expect(app.shows("id:mk-card-skill-89")).toBe(false);

  await app.setViewport(1440, WINDOW_H);
  await app.waitUntil(() => app.shows("id:mk-card-skill-00"));
  expect(app.shows("id:mk-card-skill-89")).toBe(false);

  await app.setViewport(WINDOW_W, WINDOW_H);
  await app.waitUntil(() => app.shows("id:mk-card-skill-00"));

  // 2. PublisherBoardPane repos: repo-00 mounted, repo-89 virtualized out.
  await app.press("id:vg-scene-pub-repos");
  await app.waitUntil(() => app.shows("id:pub-repo-row-repo-00"));
  expect(app.shows("id:pub-repo-row-repo-89")).toBe(false);

  // 3. PublisherBoardPane repo skills: pub-skill-00 mounted, pub-skill-89 virtualized out.
  await app.press("id:vg-scene-pub-skills");
  await app.waitUntil(() => app.shows("id:mk-card-pub-skill-00"));
  expect(app.shows("id:mk-card-pub-skill-89")).toBe(false);

  // 4. SkillCard + RepoAvatar fallback branches render without error.
  await app.press("id:vg-scene-skill-card");
  await app.waitUntil(() => app.shows("id:skill-card-local-demo"));
  expect(app.shows("id:vg-avatar-branch")).toBe(true);
  expect(app.shows("id:vg-avatar-local")).toBe(true);
});
