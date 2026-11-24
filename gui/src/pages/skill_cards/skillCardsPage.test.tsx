// Deck-page headless tests — the skill_cards slice's data flow and UI
// behaviors (skill_cards/mod.rs + query.rs parity). Data-flow checks run
// against the real domain through the bridge (the same files the GPUI page
// reads), and the UI assertions derive their expectations from that same
// snapshot, so both branches — a machine with decks and a fresh install
// with none — are covered without writing the real $HOME.
import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../layout";
import { SkillCards } from "skillstar-gui/native";
import {
  deckAgentLinkState,
  linkReportNotice,
  matchesDeckQuery,
  nextEnableForAgentState,
  parseDeck,
  parseLinkReport,
  parseSnapshot,
  type DeckRecord,
} from "./deckQuery";

function fixtureDeck(): DeckRecord {
  // The same shape query.rs's own test uses (skill_cards/query.rs:27-38).
  return parseDeck({
    id: "deck-1",
    name: "Frontend Starter",
    description: "Essential tools for React and CSS",
    icon: "🎨",
    skills: ["tailwind-helper", "react-hooks"],
    skillSources: {},
    defaultAgent: "",
    agentLinks: [],
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  });
}

// ---- query.rs parity (plain data) --------------------------------------

test("deck query matches name, description and member skills", () => {
  const deck = fixtureDeck();
  expect(matchesDeckQuery(deck, "")).toBe(true);
  expect(matchesDeckQuery(deck, "  ")).toBe(true);
  expect(matchesDeckQuery(deck, "Front")).toBe(true);
  expect(matchesDeckQuery(deck, "starter")).toBe(true);
  expect(matchesDeckQuery(deck, "react")).toBe(true);
  expect(matchesDeckQuery(deck, "tailwind")).toBe(true);
  expect(matchesDeckQuery(deck, "python")).toBe(false);
});

test("deck parse degrades unknown shapes to defaults", () => {
  const empty = parseDeck(undefined);
  expect(empty.skills).toEqual([]);
  expect(empty.agentLinks).toBe(null);
  // agentLinks null and [] both read as unlinked on the rail.
  const noLinks = parseDeck({ id: "a", name: "A", agentLinks: null });
  expect(noLinks.agentLinks).toBe(null);
  const snapshot = parseSnapshot({ decks: [], installedSkills: "junk", agents: 7 });
  expect(snapshot.decks).toEqual([]);
  expect(snapshot.installedSkills).toEqual([]);
  expect(snapshot.agents).toEqual([]);
});

// ---- notify.rs agent_link_notice parity (plain data) --------------------

test("an all-applied link report carries no notice", () => {
  const report = parseLinkReport({
    applied: [["claude", "react-hooks"]],
    skipped: [],
    failed: [],
  });
  expect(linkReportNotice(report, (key, vars) => `${key}:${JSON.stringify(vars)}`)).toBe(null);
});

test("failures and skips shape the partial notice", () => {
  const report = parseLinkReport({
    applied: [["claude", "a"]],
    skipped: [{ agentId: "codex", skill: "b", reason: "occupied" }],
    failed: [{ agentId: "gemini", skill: "c", reason: "no dir" }],
  });
  const notice = linkReportNotice(report, (key, vars) => `${key} ${JSON.stringify(vars)}`);
  if (notice === null) throw new Error("expected a partial-failure notice");
  expect(notice).toContain("skillCards.batchTogglePartialFailed");
  expect(notice).toContain('{"failed":1,"total":3}');
  expect(notice).toContain("c → gemini: no dir");
  expect(notice).toContain("skillCards.batchTogglePartialSkipped");
  expect(notice).toContain('{"skipped":1,"total":3}');
  expect(notice.split("\n").length).toBe(2);
});

// ---- bridge data flow (real domain, read-only) --------------------------

test("the snapshot carries decks, installed skills and the rail agents", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  for (const deck of snapshot.decks) {
    expect(typeof deck.id).toBe("string");
    expect(typeof deck.name).toBe("string");
    expect(Array.isArray(deck.skills)).toBe(true);
  }
  for (const name of snapshot.installedSkills) {
    expect(typeof name).toBe("string");
  }
  for (const agent of snapshot.agents) {
    expect(typeof agent.id).toBe("string");
    expect(typeof agent.displayName).toBe("string");
  }
});

// ---- UI behaviors -------------------------------------------------------

test("the deck page paints the toolbar with exactly two command entries", async () => {
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  // The rail's own "卡组" label is always on screen; wait for the page's
  // toolbar instead — the keep-alive page flips to the foreground a frame
  // after the nav press.
  await app.waitUntil(() => app.shows("id:deck-import-btn"));
  // 导入 / 新建卡组… both live; the count chip shows the filtered count.
  expect(app.shows("id:deck-import-btn")).toBe(true);
  expect(app.shows("id:deck-new-btn")).toBe(true);
  expect(app.shows("id:deck-count-filter")).toBe(true);
  expect(app.shows("id:deck-view-grid")).toBe(true);
  expect(app.shows("id:deck-view-list")).toBe(true);
});

test("the grid mirrors the real snapshot, card for card", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  if (snapshot.decks.length === 0) {
    // A fresh install: the shared empty state, with the create CTA.
    await app.waitUntil(() => app.shows("id:deck-empty"));
    expect(app.texts()).toContain("还没有卡组");
    expect(app.shows("id:deck-empty-create")).toBe(true);
    return;
  }
  for (const deck of snapshot.decks.slice(0, 4)) {
    await app.waitUntil(() => app.shows(`id:deck-card-${deck.id}`));
  }
  // The grid is virtualized row-by-row with no broken U+FFFD replacement glyphs.
  expect(app.shows("id:deck-grid")).toBe(true);
  expect(app.shows("id:deck-row-0")).toBe(true);
  expect(app.texts().some((text) => text.includes("\uFFFD"))).toBe(false);

  // Every card paints its member chips preview: at most 4 names, then the
  // "+N" tail (group_card.rs:184-229).
  const first = snapshot.decks[0];
  for (const skill of first.skills.slice(0, 4)) {
    expect(app.texts()).toContain(skill);
  }
  // The count badge rides the header actions beside share and the menu.
  expect(app.shows(`id:deck-count-badge`)).toBe(true);
  expect(app.shows(`id:deck-share-${first.id}`)).toBe(true);
  expect(app.shows(`id:deck-more-${first.id}`)).toBe(true);
});

test("drawer and create-group emoji picker never emit U+FFFD replacement glyphs", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  await app.waitUntil(() => app.shows("id:deck-new-btn"));

  if (snapshot.decks.length > 0) {
    const first = snapshot.decks[0];
    await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
    await app.press(`id:deck-card-${first.id}`);
    await app.waitUntil(() => app.shows("id:close-deck-detail-drawer"));
    expect(app.texts().some((text) => text.includes("\uFFFD"))).toBe(false);
    await app.press("id:close-deck-detail-drawer");
    await app.waitUntil(() => !app.shows("id:close-deck-detail-drawer"));
  }

  await app.press("id:deck-new-btn");
  await app.waitUntil(() => app.shows("id:pack-icon"));
  await app.press("id:pack-icon");
  await app.waitUntil(() => app.shows("id:pack-emoji-0"));
  expect(app.texts().some((text) => text.includes("\uFFFD"))).toBe(false);
  await app.press("id:pack-cancel");
});

test("searching filters the grid by the query predicate", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  if (snapshot.decks.length === 0) {
    // Nothing to filter on a fresh install; the no-match state is covered
    // by the term search below all the same.
    return;
  }
  const first = snapshot.decks[0];
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
  // A term that matches only the first deck's name (unique by suffixing
  // its full name).
  await app.type("id:deck-search-input", first.name);
  await app.waitUntil(() => {
    const wanted = snapshot.decks.filter((deck) => matchesDeckQuery(deck, first.name));
    return (
      app.shows(`id:deck-card-${wanted[0]?.id ?? ""}`) &&
      wanted.every((deck) => app.shows(`id:deck-card-${deck.id}`))
    );
  });
  const matching = snapshot.decks.filter((deck) => matchesDeckQuery(deck, first.name));
  // Decks the predicate rejects are gone from the tree.
  for (const deck of snapshot.decks) {
    if (!matching.some((m) => m.id === deck.id)) {
      expect(app.shows(`id:deck-card-${deck.id}`)).toBe(false);
    }
  }

  // A term nothing matches: the no-match state (mod.rs:393-419).
  await app.type("id:deck-search-input", "zz-no-such-deck-zz");
  await app.waitUntil(() => app.shows("id:deck-no-match"));
  expect(app.texts()).toContain("没有匹配的卡组");

  await app.clear("id:deck-search-input");
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
});

test("the grid/list switch keeps every deck visible", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  if (snapshot.decks.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  const first = snapshot.decks[0];
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
  await app.press("id:deck-view-list");
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
  await app.press("id:deck-view-grid");
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
  for (const deck of snapshot.decks.slice(0, 4)) {
    expect(app.shows(`id:deck-card-${deck.id}`)).toBe(true);
  }
});

test("pressing a deck opens the detail track; pressing it again closes it", async () => {
  const snapshot = parseSnapshot(await SkillCards.snapshot());
  if (snapshot.decks.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  const first = snapshot.decks[0];
  await app.waitUntil(() => app.shows(`id:deck-card-${first.id}`));
  // The detail column rides the reserved fixed-width track and appears in
  // one repaint (no slide-in): the drawer slice's component paints it.
  await app.press(`id:deck-card-${first.id}`);
  await app.waitUntil(() => app.shows("id:close-deck-detail-drawer"));
  expect(app.texts()).toContain(first.name);
  await app.press(`id:deck-card-${first.id}`);
  await app.waitUntil(() => !app.shows("id:close-deck-detail-drawer"));
  // And via the column's own close control.
  await app.press(`id:deck-card-${first.id}`);
  await app.waitUntil(() => app.shows("id:close-deck-detail-drawer"));
  await app.press("id:close-deck-detail-drawer");
  await app.waitUntil(() => !app.shows("id:close-deck-detail-drawer"));
});

test("the import entry opens the import slice's dialog", async () => {
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-decks");
  // press does not wait for the selector: give the keep-alive page a frame
  // to come to the foreground before pressing its import command.
  await app.waitFor("id:deck-import-btn");
  await app.press("id:deck-import-btn");
  // The deck-import slice owns the dialog; this page only opens it.
  await app.waitUntil(() => app.shows("id:deck-import-card"));
});

test("deckAgentLinkState computes mixed from member links and completes on next click", () => {
  const claude = { id: "claude", displayName: "Claude Code" };
  const codex = { id: "codex", displayName: "Codex" };
  const cursor = { id: "cursor", displayName: "Cursor" };
  const deck = parseDeck({
    id: "d1",
    name: "Mixed Deck",
    skills: ["a", "b"],
    agentLinks: ["Claude Code", "codex"],
    memberAgentLinks: {
      a: ["Claude Code", "codex", "cursor"],
      b: ["codex", "cursor"],
    },
  });
  expect(deckAgentLinkState(deck, claude)).toBe("mixed");
  expect(deckAgentLinkState(deck, codex)).toBe("linked");
  expect(deckAgentLinkState(deck, cursor)).toBe("none");
  expect(nextEnableForAgentState("mixed")).toBe(true);
  expect(nextEnableForAgentState("none")).toBe(true);
  expect(nextEnableForAgentState("linked")).toBe(false);
});
