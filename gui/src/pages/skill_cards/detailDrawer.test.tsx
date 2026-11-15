// Deck detail drawer — headless data-flow and interaction checks
// (detail_drawer_tests.rs parity, the parts a headless engine can see).
// The harness lives in detailDrawer.test.host.tsx: fake ops record calls,
// scene buttons swap decks, counters render as text (the createGroup host
// pattern). Real domain writes are covered by the native crate's cargo
// tests under an isolated SKILLSTAR_DATA_DIR; these tests assert the
// drawer's wiring: what it renders from a deck record, which commands fire
// which ops, the notice the report shapes, and the busy gate.
import { expect, open, test } from "nsh/test";
import {
  agentLinkNotice,
  deckGlyph,
  isAgentLinked,
  missingCount,
} from "./detailDrawerActions";

const HOST = "./detailDrawer.test.host.tsx";

// ---- pure logic (notify.rs / chrome predicates parity) -------------------

test("agentLinkNotice stays silent when everything applied", () => {
  expect(
    agentLinkNotice({ applied: [["claude", "a"]], skipped: [], failed: [] })
  ).toBe(null);
});

test("agentLinkNotice leads with the first failure, then the skipped line", () => {
  const notice = agentLinkNotice({
    applied: [["claude", "a"]],
    skipped: [{ agentId: "codex", skill: "b", reason: "occupied" }],
    failed: [{ agentId: "gemini", skill: "c", reason: "no dir" }],
  });
  expect(notice).toContain("有 1/3 个链接未能更新");
  expect(notice).toContain("c → gemini: no dir");
  expect(notice).toContain("已跳过 1/3 个链接");
  expect(notice?.split("\n").length).toBe(2);
});

test("the rail predicate matches by id or display name", () => {
  const links = ["claude", "Code"];
  expect(isAgentLinked({ id: "claude", displayName: "Claude Code" }, links)).toBe(true);
  // Older decks recorded display names.
  expect(isAgentLinked({ id: "codex", displayName: "Code" }, links)).toBe(true);
  expect(isAgentLinked({ id: "gemini", displayName: "Gemini" }, links)).toBe(false);
});

test("missingCount counts members absent from the installed set", () => {
  const deck = { skills: ["alpha", "beta", "gamma"] } as never;
  expect(missingCount(deck, new Set(["alpha", "beta"]))).toBe(1);
  expect(missingCount(deck, new Set())).toBe(3);
});

test("deckGlyph falls back icon → name → the box emoji without splitting surrogates", () => {
  expect(deckGlyph({ icon: "🚀", name: "n" } as never)).toBe("🚀");
  expect(deckGlyph({ icon: "📦", name: "demo" } as never)).toBe("📦");
  expect(deckGlyph({ icon: "🛠️", name: "tools" } as never)).toBe("🛠️");
  expect(deckGlyph({ icon: "\uFFFD\uFFFD\uFFFD", name: "demo" } as never)).toBe("d");
  expect(deckGlyph({ icon: "", name: "演示" } as never)).toBe("演");
  expect(deckGlyph({ icon: "  ", name: "demo" } as never)).toBe("d");
  expect(deckGlyph({ icon: "  ", name: "  " } as never)).toBe("\u{1f4e6}");
});

// ---- the column (clicking_a_deck_opens_its_detail_column parity) --------

test("the column paints the deck, its badges and every member row", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  expect(app.texts()).toContain("演示卡组");
  expect(app.texts()).toContain("两组技能");
  expect(app.texts()).toContain("2");
  expect(app.texts()).toContain("缺 1 个");
  expect(app.texts()).toContain("alpha");
  expect(app.texts()).toContain("beta");
  // Both member rows exist, keyed by member name.
  expect(app.count("id:deck-member-alpha")).toBe(1);
  expect(app.count("id:deck-member-beta")).toBe(1);
  // The named controls the GPUI tests look for.
  expect(app.shows("id:deck-drawer-edit")).toBe(true);
  expect(app.shows("id:deck-drawer-share")).toBe(true);
  expect(app.shows("id:deck-drawer-more")).toBe(true);
  expect(app.shows("id:deck-drawer-link-all")).toBe(true);
});

test("the agent rows ride the body: one switch per targetable agent", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  expect(app.texts()).toContain("Claude Code");
  expect(app.texts()).toContain("Code");
  expect(app.texts()).toContain("链接到智能体");
  expect(app.texts()).toContain("链接到所有已启用智能体");
  // One switch per targetable agent, keyed by deck and agent.
  expect(app.count("id:deck-drawer-toggle-g1-claude")).toBe(1);
  expect(app.count("id:deck-drawer-toggle-g1-codex")).toBe(1);
  expect(app.count("id:deck-drawer-agent-g1-claude")).toBe(1);
});

test("an empty deck shows the empty line and a disabled deploy-all", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-scene-empty");
  expect(app.texts()).toContain("这个卡组里还没有技能");
  expect(app.enabled("id:deck-drawer-link-all")).toBe(false);
});

test("no targetable agents explains how to enable one", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-scene-noagents");
  expect(app.texts()).toContain("没有已启用的智能体——请先在「设置」中开启。");
  expect(app.shows("id:deck-drawer-link-all")).toBe(false);
});

// ---- commands (the batch deploy, the switches, the more menu) -----------

test("deploy-all calls the batch op and reports the skipped pair", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset-calls");
  await app.press("id:deck-drawer-link-all");
  await app.waitUntil(() => app.texts().some((t) => t.includes("all:g1")));
  // The report's skipped pair becomes the notice-band copy.
  await app.waitUntil(() => app.texts().some((t) => t.includes("已跳过 1/2 个链接")));
  // The job's tail published GroupsChanged for the page to reload.
  await app.waitUntil(() => app.texts().some((t) => t.startsWith("groups:1")));
});

test("a switch toggles exactly that agent: linked clears, unlinked links", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset-calls");
  await app.press("id:deck-drawer-toggle-g1-claude");
  await app.press("id:deck-drawer-toggle-g1-codex");
  await app.waitUntil(() =>
    app.texts().some((t) => t.includes("set:g1:claude:false") && t.includes("set:g1:codex:true"))
  );
});

test("a held job disables every rail control until it settles", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-hold-next");
  await app.press("id:deck-drawer-link-all");
  // While the domain job is in flight the batch is disabled (busy gate:
  // repeated clicks must not submit a second job).
  expect(app.enabled("id:deck-drawer-link-all")).toBe(false);
  expect(app.enabled("id:deck-drawer-toggle-g1-claude")).toBe(false);
  await app.press("id:host-release");
  await app.waitUntil(() => app.texts().some((t) => t.startsWith("groups:1")));
  expect(app.enabled("id:deck-drawer-link-all")).toBe(true);
});

test("page-level busy disables the drawer commands too", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-page-busy");
  expect(app.enabled("id:deck-drawer-edit")).toBe(false);
  expect(app.enabled("id:deck-drawer-link-all")).toBe(false);
  await app.press("id:host-page-busy");
  expect(app.enabled("id:deck-drawer-edit")).toBe(true);
});

test("the more menu duplicates the deck through the op", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset-calls");
  await app.press("id:deck-drawer-more");
  expect(app.texts()).toContain("复制卡组");
  expect(app.texts()).toContain("删除卡组…");
  await app.press("text:复制卡组");
  await app.waitUntil(() => app.texts().some((t) => t.includes("dup:g1")));
});

// drawer_menu_delete_requires_confirmation parity: the danger item opens
// the shared confirm card (openConfirm → DialogHost), which names the deck
// and says Hub skills stay; cancel keeps it, the danger commit runs the op.
test("delete asks for the deck by name; cancel keeps it, delete runs the op", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset-calls");
  await app.press("id:deck-drawer-more");
  await app.press("text:删除卡组…");
  await app.waitUntil(() => app.texts().some((t) => t.includes("删除「演示卡组」")));
  expect(app.texts()).toContain("删除这个卡组。Hub 里的技能会保留。");
  // Cancel: the deck survives, no op ran.
  await app.press("text:取消");
  await app.waitUntil(() => !app.texts().some((t) => t.includes("删除「演示卡组」")));
  expect(app.texts().some((t) => t.includes("del:"))).toBe(false);

  await app.press("id:deck-drawer-more");
  await app.press("text:删除卡组…");
  await app.waitUntil(() => app.texts().some((t) => t.includes("删除「演示卡组」")));
  await app.press("text:删除");
  await app.waitUntil(() => app.texts().some((t) => t.includes("del:g1")));
});

test("edit, share and close report back to the page", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:deck-drawer-edit");
  await app.press("id:deck-drawer-share");
  await app.press("id:close-deck-detail-drawer");
  await app.waitUntil(() => app.texts().some((t) => t === "edited:1"));
  expect(app.texts().some((t) => t === "shared:1")).toBe(true);
  expect(app.texts().some((t) => t === "closed:1")).toBe(true);
});

// ---- the virtualized member list ----------------------------------------

// Virtualization parity (long_deck_only_lays_out_the_visible_members):
// fifty members, only the window around the viewport is laid out — the far
// rows never exist. The scroll-to-tail / back-to-top half of the GPUI test
// rides the List's own scroller, which the headless engine's drag cannot
// drive (each scroll step advances the window by ~one row); the return-to-
// top rule is the drawer's deck-change effect (scrollToOffset(0) in
// detailDrawer.tsx) and is noted in the port result's gaps.
test("long decks lay out only the members near the viewport", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-scene-fifty");
  // The window starts at the list head (the label slot above it reserves
  // the first rows' space, so the first painted row may be a row or two in).
  await app.waitUntil(() => app.shows("id:deck-member-member-02"));
  expect(app.shows("id:deck-member-member-44")).toBe(false);
  expect(app.count("text:member-")).toBeLessThan(50);
});
