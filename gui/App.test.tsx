// Shared-layer self-checks: the native bridge proof, the shell (sidebar
// navigation, page switching, locale, rail collapse), the notice-band
// scheduling rules (unit level, the same rules notify.rs tests) and the
// layout invariants. `nsh test` runs the crate's `cargo test` first, then
// these files headless.
import { expect, open, test } from "nsh/test";
import { Marketplace, Settings } from "skillstar-gui/native";
import { normalizeLocale } from "./src/services/locale";
import { PAGE_PAD, SHELL_GAP, WINDOW_H, WINDOW_W, checkLayoutInvariants } from "./src/layout";
import { bandPadRight } from "./src/shell/caption";
import { createNoticeBoard, NOTICE_CAP } from "./src/shell/noticeBoard";

test("bridge gates exact github clone urls only", () => {
  expect(Marketplace.mayQueryInstallAudits("https://github.com/anthropics/skills")).toBe(true);
  expect(Marketplace.mayQueryInstallAudits("https://gitlab.com/owner/repo")).toBe(false);
  expect(Marketplace.mayQueryInstallAudits("https://github.com/anthropics/skills/tree/main")).toBe(
    false
  );
  expect(Marketplace.mayQueryInstallAudits("")).toBe(false);
});

test("layout invariants hold (layout.rs const-assertion parity)", () => {
  expect(checkLayoutInvariants()).toEqual([]);
});

// The hidden title bar draws the platform's window controls over the app
// (D-115): the page bands have to end before them instead of under them.
test("page bands stop beside the platform's caption buttons", () => {
  // macOS: the traffic lights are on the left, so the bands keep PAGE_PAD.
  expect(bandPadRight(0)).toBe(PAGE_PAD);
  // Windows: three 46pt caption buttons at the top right, and the page sits
  // SHELL_GAP inside the window, so the band ends that much narrower.
  expect(bandPadRight(3 * 46)).toBe(3 * 46 - SHELL_GAP);
  // A caption area narrower than the gutter never squeezes the band further.
  expect(bandPadRight(SHELL_GAP)).toBe(PAGE_PAD);
});

test("sidebar shows the skill-mode navigation in zh-CN", async () => {
  const app = await open("./App.tsx");
  // The four skills-nav rows (footer buttons are icon-only, as in the GPUI
  // rail: collapse | theme | settings).
  expect(app.texts()).toContain("技能");
  expect(app.texts()).toContain("市场");
  expect(app.texts()).toContain("卡组");
  expect(app.texts()).toContain("项目");
  // Brand wordmark + tagline + restored 32px icon (no "S" placeholder).
  expect(Settings.brandIconDataUri().startsWith("data:image/png;base64,")).toBe(true);
  expect(Settings.agentIconDataUri("claude-code", false).startsWith("data:image/svg+xml;base64,")).toBe(true);
  expect(Settings.agentIconDataUri("claude-code", false)).toBe(
    Settings.agentIconDataUri("claude", false)
  );
  expect(Settings.agentIconDataUri("cursor", false)).not.toBe(
    Settings.agentIconDataUri("cursor", true)
  );
  expect(app.shows("id:sidebar-brand-mark")).toBe(true);
  expect(app.shows("id:sidebar-brand-icon")).toBe(true);
  expect(app.texts()).not.toContain("S");
  expect(app.texts()).toContain("少即是多");
});

test("pressing the marketplace entry surfaces the market toolbar", async () => {
  // Desktop window: the default 390pt phone width cannot lay the toolbar out.
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("nav-market");
  // The wave-2 market page base: four tabs, sort, view switch, refresh and
  // the ⌘F spotlight trigger (the wave-1 bridge self-check card is gone with
  // the placeholder page it lived on).
  await app.waitUntil(() => app.shows("id:mk-tabs"));
  expect(app.texts()).toContain("全部");
  expect(app.texts()).toContain("趋势");
  expect(app.texts()).toContain("热门");
  expect(app.texts()).toContain("官方");
  expect(app.shows("id:mk-spotlight-trigger")).toBe(true);
});

test("locale normalization follows the i18n.rs rules", () => {
  expect(normalizeLocale("en")).toBe("en");
  expect(normalizeLocale("en-US")).toBe("en");
  expect(normalizeLocale("zh")).toBe("zh-CN");
  expect(normalizeLocale("zh-CN")).toBe("zh-CN");
  expect(normalizeLocale("fr-FR")).toBe("zh-CN");
  expect(normalizeLocale("")).toBe("zh-CN");
});

test("toggling the theme keeps the shell standing", async () => {
  const app = await open("./App.tsx");
  await app.press("sidebar-theme");
  // Paper mode: light canvas. A second press returns the dark canvas.
  await app.press("sidebar-theme");
  expect(app.texts()).toContain("SkillStar");
});

test("pressing the settings footer button opens the settings page and returns cleanly", async () => {
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("sidebar-settings");
  await app.waitUntil(() => app.shows("id:page-settings"));
  await app.press("nav-skills");
  await app.waitUntil(() => app.shows("id:page-my_skills"));
});

test("collapsing the rail keeps the four nav rows clickable", async () => {
  const app = await open("./App.tsx");
  await app.press("sidebar-collapse");
  await app.press("nav-decks");
  expect(app.texts()).toContain("卡组");
  await app.press("sidebar-collapse");
});

test("the board pushes and caps notices at four", () => {
  const board = createNoticeBoard();
  const expiries: number[] = [];
  for (let i = 1; i <= 5; i++) {
    board.push({ tone: "info", message: `msg ${i}` }, (seq) => expiries.push(seq));
  }
  const entries = board.entries();
  // The 5th dropped the oldest unpinned one.
  expect(entries.length).toBe(NOTICE_CAP);
  expect(entries[entries.length - 1].message).toBe("msg 5");
  expect(entries.map((e) => e.message)).not.toContain("msg 1");
  // Pinned ones are never crowded out: four pinned stay, a fifth joins them.
  const pinnedBoard = createNoticeBoard();
  for (let i = 1; i <= 5; i++) {
    pinnedBoard.push({ tone: "info", message: `pinned ${i}`, pinned: true }, () => {});
  }
  expect(pinnedBoard.entries().length).toBe(5);
  expect(expiries.length).toBe(5); // every unpinned notice scheduled its TTL
});

test("notices sharing a replace key displace each other", () => {
  const board = createNoticeBoard();
  board.push({ tone: "info", message: "first", replaceKey: "family-a", pinned: true }, () => {});
  board.push({ tone: "success", message: "second", replaceKey: "family-a", pinned: true }, () => {});
  const entries = board.entries();
  expect(entries.length).toBe(1);
  expect(entries[0].message).toBe("second");
});

test("an action auto-pins and close runs the exit before removal", () => {
  const board = createNoticeBoard();
  board.push(
    { tone: "warning", message: "with action", action: { label: "open", run: () => {} } },
    () => {}
  );
  expect(board.entries()[0].pinned).toBe(true);
  const seq = board.entries()[0].seq;
  expect(board.close(seq, true)).toBe(true);
  expect(board.entries()[0].closing).toBe(true);
  expect(board.remove(seq)).toBe(true);
  expect(board.entries().length).toBe(0);
});

test("under reduce motion close removes immediately", () => {
  const board = createNoticeBoard();
  board.push({ tone: "error", message: "snap", pinned: true }, () => {});
  const seq = board.entries()[0].seq;
  board.close(seq, false);
  expect(board.entries().length).toBe(0);
});

test("unpinned notices schedule the ttl expiry", () => {
  const board = createNoticeBoard();
  const expiries: number[] = [];
  board.push({ tone: "info", message: "fleeting" }, (seq) => expiries.push(seq));
  expect(expiries.length).toBe(1);
});

// The resident window: closing it hides it, and the app keeps running. The
// platform must report the press instead of closing (nsh.json
// `window.closeOnPress: false`), which is what lets src/shell/residentWindow.ts
// hide it — from the Dock on macOS, the tray or a second launch on Windows.
test("closing the main window hides it and keeps the app running", async () => {
  // A desktop app: a phone-shaped headless run has no main window to close.
  // Windows, because that is the platform whose residency rests entirely on
  // this handler (macOS also has the platform's keep-alive behind it).
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H, platform: "windows" });
  const window = app.window("main");
  expect(window.state()?.visible).toBe(true);
  await window.pressClose();
  // Hidden, not gone: a closed window reports no state at all, while a hidden
  // one stays open (and not minimized), so the app keeps running behind it.
  const state = window.state();
  expect(state).not.toBeNull();
  expect(state?.visible).toBe(false);
  expect(state?.minimized).toBe(false);
});

test("top drag regions cover shell, sidebar, pagebars, and accounts without stealing interactive controls", async () => {
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  expect(app.shows("id:shell-top-drag")).toBe(true);
  expect(app.shows("id:sidebar-top-drag")).toBe(true);
  expect(app.shows("id:my-skills-toolbar-drag-drag")).toBe(true);
  expect(app.shows("id:sidebar-brand-mark")).toBe(true);
  await app.press("my-skills-scope-channels");
  await app.press("my-skills-scope-local");
  await app.press("mode-accounts");
  await app.waitUntil(() => app.shows("id:acct-top-drag"));
  expect(app.shows("id:acct-top-drag")).toBe(true);
  await app.press("mode-skills");
  await app.waitUntil(() => app.shows("id:page-my_skills"));
  await app.press("sidebar-collapse");
  expect(app.shows("id:sidebar-top-drag")).toBe(true);
  expect(app.shows("id:sidebar-brand-mark")).toBe(true);
  await app.press("sidebar-collapse");
});
