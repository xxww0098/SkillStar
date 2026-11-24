// Shared-layer self-checks: the native bridge proof, the shell (sidebar
// navigation, page switching, locale, rail collapse), the notice-band
// scheduling rules (unit level, the same rules notify.rs tests) and the
// layout invariants. `nsh test` runs the crate's `cargo test` first, then
// these files headless.
import { signal } from "nsh";
import { expect, open, test } from "nsh/test";
import { Settings } from "skillstar-gui/native";
import { normalizeLocale } from "./src/services/locale";
import {
  PAGE_PAD,
  PANEL_BORDER_X,
  RAIL_COLLAPSED_W,
  RAIL_W,
  SHELL_GAP,
  WINDOW_H,
  WINDOW_W,
  checkLayoutInvariants,
} from "./src/layout";
import {
  collapsed as navCollapsed,
  createPaneWidth,
  shellPanelInnerWidth,
  shellWidth,
} from "./src/shell/nav";
import { bandPadRight } from "./src/shell/caption";
import {
  createNoticeBoard,
  NOTICE_ACTION_TTL_MS,
  NOTICE_PROGRESS_TTL_MS,
  NOTICE_RESTORED_MS,
  NOTICE_TTL_MS,
} from "./src/shell/noticeBoard";
import { computeWindowProgress } from "./src/shell/desktopShell";
import { createJobBoard, JOB_DONE_TTL_MS } from "./src/shell/jobBoard";
import { subscribeNotify, toast } from "./src/shell/events";

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
  // The five skills-nav rows (footer buttons are icon-only, as in the GPUI
  // rail: collapse | theme | settings).
  expect(app.texts()).toContain("技能");
  expect(app.texts()).toContain("市场");
  expect(app.texts()).toContain("卡组");
  expect(app.texts()).toContain("项目");
  expect(app.texts()).toContain("MCP");
  // Brand wordmark + tagline + restored 32px icon (no "S" placeholder).
  expect(Settings.brandIconDataUri().startsWith("data:image/png;base64,")).toBe(true);
  expect(Settings.trayIconDataUri().startsWith("data:image/svg+xml;base64,")).toBe(true);
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

test("pressing the mcp nav opens the MCP workbench page", async () => {
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("nav-mcp");
  await app.waitUntil(() => app.shows("id:page-mcp"));
  expect(app.shows("id:mcp-workbench")).toBe(true);
});

test("pressing the marketplace entry surfaces the market toolbar", async () => {
  // Desktop window: the default 390pt phone width cannot lay the toolbar out.
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("nav-market");
  // The wave-2 market page base: four tabs, sort, view switch, refresh and
  // the inline search box shared with the skills page (⌘F focuses it; the
  // wave-1 bridge self-check card is gone with the placeholder page).
  await app.waitUntil(() => app.shows("id:mk-tabs"));
  expect(app.texts()).toContain("全部");
  expect(app.texts()).toContain("趋势");
  expect(app.texts()).toContain("热门");
  expect(app.texts()).toContain("官方");
  expect(app.shows("id:mk-search-input")).toBe(true);
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

test("one toast replaces the previous, and a pinned toast does not outlast it", () => {
  const board = createNoticeBoard();
  const first = board.push({ tone: "info", message: "first", ttlMs: 3000 }, 0);
  const second = board.push({ tone: "info", message: "second", ttlMs: 3000 }, 1000);
  expect(board.entries().length).toBe(1);
  expect(second.message).toBe("second");
  expect(second.seq).not.toBe(first.seq);
  board.push({ tone: "error", message: "pinned", pinned: true }, 0);
  board.push({ tone: "info", message: "next", ttlMs: 3000 }, 50);
  expect(board.current()?.message).toBe("next");
  expect(board.current()?.pinned).toBe(false);
});

test("notices sharing a replace key morph in place", () => {
  const board = createNoticeBoard();
  const first = board.push(
    { tone: "info", message: "first", replaceKey: "family-a", pinned: true },
    0,
  );
  const second = board.push(
    { tone: "success", message: "second", replaceKey: "family-a", ttlMs: 3000 },
    10,
  );
  expect(board.entries().length).toBe(1);
  expect(second.seq).toBe(first.seq);
  expect(second.generation).toBe(first.generation + 1);
  expect(second.message).toBe("second");
  expect(second.pinned).toBe(false);
  expect(second.deadline).toBe(3010);
});

test("an action does not pin and close still marks the exit", () => {
  const board = createNoticeBoard();
  const entry = board.push(
    { tone: "warning", message: "with action", action: { label: "open", run: () => {} } },
    0,
  );
  expect(entry.pinned).toBe(false);
  expect(entry.kind).toBe("action");
  expect(entry.sheen).toBe(false);
  expect(entry.deadline).toBe(NOTICE_ACTION_TTL_MS);
  expect(board.close(entry.seq, true)).toBe(true);
  expect(board.current()?.closing).toBe(true);
  expect(board.remove(entry.seq)).toBe(true);
  expect(board.entries().length).toBe(0);
  const longer = board.push(
    {
      tone: "info",
      message: "hold",
      ttlMs: 8000,
      action: { label: "open", run: () => {} },
    },
    0,
  );
  expect(longer.deadline).toBe(8000);
});

test("under reduce motion close removes immediately", () => {
  const board = createNoticeBoard();
  board.push({ tone: "error", message: "snap", pinned: true }, 0);
  const seq = board.current()?.seq ?? 0;
  board.close(seq, false);
  expect(board.entries().length).toBe(0);
});

test("the clock pauses while held and honors explicit ttl", () => {
  const board = createNoticeBoard();
  const fleeting = board.push({ tone: "info", message: "hi" }, 1000);
  expect(fleeting.deadline).toBe(1000 + 3000);
  expect(fleeting.sheen).toBe(true);
  expect(board.due(3999)).toBe(null);
  expect(board.due(4000)).toBe(fleeting.seq);
  board.hold(fleeting.seq, 2000);
  expect(board.due(9000)).toBe(null);
  board.hold(fleeting.seq, 2500);
  board.release(fleeting.seq, 2500);
  // Held at 2000 with 2000ms left, so releasing at 2500 is due at 4500.
  expect(board.current()?.deadline).toBe(4500);
  expect(board.due(4499)).toBe(null);
  expect(board.due(4500)).toBe(fleeting.seq);

  const pinned = createNoticeBoard().push({ tone: "error", message: "stay", pinned: true }, 0);
  expect(pinned.deadline).toBe(null);
  const warn = createNoticeBoard().push({ tone: "warning", message: "w" }, 0);
  expect(warn.deadline).toBe(NOTICE_TTL_MS);
  const given = createNoticeBoard().push(
    { tone: "success", message: "x".repeat(200), ttlMs: 3000 },
    0,
  );
  expect(given.deadline).toBe(3000);
  const long = createNoticeBoard().push({ tone: "info", message: "x".repeat(120) }, 0);
  expect(long.deadline).toBe(5000);
});

test("undo shows the restored line and progress stays until it settles", () => {
  const board = createNoticeBoard();
  const undo = board.push(
    {
      tone: "success",
      message: "deleted",
      undo: true,
      action: { label: "undo", run: () => {} },
    },
    0,
  );
  expect(undo.kind).toBe("action");
  expect(board.showRestored(undo.seq, 1000)).toBe(true);
  expect(board.current()?.restored).toBe(true);
  expect(board.current()?.sheen).toBe(true);
  expect(board.current()?.deadline).toBe(1000 + NOTICE_RESTORED_MS);
  expect(board.dismissReplaceKey("missing")).toBe(false);

  const progress = createNoticeBoard();
  const working = progress.push(
    { tone: "info", message: "working", kind: "progress", pending: true, replaceKey: "job" },
    0,
  );
  expect(working.deadline).toBe(null);
  expect(working.sheen).toBe(false);
  const done = progress.push(
    { tone: "success", message: "done", kind: "progress", pending: false, replaceKey: "job" },
    500,
  );
  expect(done.seq).toBe(working.seq);
  expect(done.deadline).toBe(500 + NOTICE_PROGRESS_TTL_MS);
  expect(done.sheen).toBe(true);
});

test("toast pins errors and shortens success", () => {
  const seen: { tone: string; pinned?: boolean; ttlMs?: number }[] = [];
  const stop = subscribeNotify((request) => {
    seen.push({ tone: request.tone, pinned: request.pinned, ttlMs: request.ttlMs });
  });
  toast("ok", "success");
  toast("heads up", "info");
  toast("careful", "warning");
  toast("nope", "error");
  stop();
  expect(seen).toEqual([
    { tone: "success", pinned: undefined, ttlMs: 3000 },
    { tone: "info", pinned: undefined, ttlMs: 3000 },
    { tone: "warning", pinned: undefined, ttlMs: undefined },
    { tone: "error", pinned: true, ttlMs: undefined },
  ]);
});

// The resident window: closing it hides it, and the app keeps running. The
// platform must report the press instead of closing (nsh.json
// `window.closeOnPress: false`), which is what lets src/shell/residentWindow.ts
// hide it — from the Dock on macOS, the tray or a second launch on Windows.
test("closing the main window hides it and keeps the app running", async () => {
  // A desktop app: a phone-shaped headless run has no main window to close.
  // Windows, because that is the platform whose residency rests entirely on
  // this handler (macOS also has the platform's keep-alive behind it).
  // Enable background_run in prefs so closePress hides rather than quitting (P1-PS3).
  const app = await open("./App.tsx", {
    width: WINDOW_W,
    height: WINDOW_H,
    platform: "windows",
    modules: {
      "skillstar-gui/native": {
        "Settings.loadPrefs": {
          language: "zh-CN",
          background_style: "current",
          background_run: true,
        },
      },
    },
  });
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
  expect(app.shows("id:sidebar-theme")).toBe(true);
  // The footer's account-quota entry is gone; quotas live in the tray window
  // and the application menu (⇧⌘U) only.
  expect(app.shows("id:sidebar-quota")).toBe(false);
  await app.press("sidebar-collapse");
  expect(app.shows("id:sidebar-top-drag")).toBe(true);
  expect(app.shows("id:sidebar-brand-mark")).toBe(true);
  expect(app.shows("id:sidebar-theme")).toBe(true);
  expect(app.shows("id:sidebar-quota")).toBe(false);
  await app.press("sidebar-collapse");
});

test("computeWindowProgress maps jobBoard jobs to Dock and taskbar WindowProgress", () => {
  expect(computeWindowProgress([])).toBeNull();
  expect(
    computeWindowProgress([
      { id: "done", title: "done", startedAt: 1, done: true, cancellable: false, fraction: 0.5 },
    ]),
  ).toBeNull();
  expect(
    computeWindowProgress([
      { id: "spin", title: "working", startedAt: 1, done: false, cancellable: false },
    ]),
  ).toEqual({ state: "indeterminate" });
  expect(
    computeWindowProgress([
      { id: "a", title: "a", startedAt: 1, done: false, cancellable: false, fraction: 0.25 },
      { id: "b", title: "b", startedAt: 1, done: false, cancellable: false, fraction: 0.75 },
      { id: "c", title: "c", startedAt: 1, done: true, cancellable: false, fraction: 1 },
    ]),
  ).toEqual({ state: "normal", value: 0.5 });
});

test("jobBoard endJob updates settled title and auto-closes finished job cards", () => {
  let now = 1000;
  let seq = 1;
  const timers = new Map<number, { at: number; fn: () => void }>();
  const board = createJobBoard({
    now: () => now,
    after: (ms, fn) => {
      const id = seq++;
      timers.set(id, { at: now + ms, fn });
      return () => timers.delete(id);
    },
  });
  const advance = (ms: number) => {
    const target = now + ms;
    for (;;) {
      let next: { id: number; at: number; fn: () => void } | null = null;
      for (const [id, entry] of timers) {
        if (entry.at <= target && (next === null || entry.at < next.at)) {
          next = { id, ...entry };
        }
      }
      if (!next) break;
      timers.delete(next.id);
      now = next.at;
      next.fn();
    }
    now = target;
  };

  board.putJob({ id: "test-batch", title: "正在刷新 0 / 2", fraction: 0 });
  advance(400);
  expect(board.jobList().find((j) => j.id === "test-batch")?.done).toBe(false);

  board.endJob("test-batch", "已刷新 2 个账号");
  const settled = board.jobList().find((j) => j.id === "test-batch");
  expect(settled?.done).toBe(true);
  expect(settled?.title).toBe("已刷新 2 个账号");
  expect(settled?.fraction).toBe(1);

  advance(JOB_DONE_TTL_MS + 50);
  expect(board.jobList().find((j) => j.id === "test-batch")).toBeUndefined();
});

test("desktop application menu shortcuts and lifecycle foreground resume work cleanly", async () => {
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H, platform: "macos" });
  await app.keyboard.press("mod+5");
  await app.waitUntil(() => app.shows("id:page-mcp"));
  await app.setLifecycle("background");
  await app.setLifecycle("active");
  await app.keyboard.press("mod+1");
  await app.waitUntil(() => app.shows("id:page-my_skills"));
});

test("shellPanelInnerWidth updates synchronously on collapse and createPaneWidth gates hidden pages", () => {
  const prevShellW = shellWidth();
  const prevCollapsed = navCollapsed();
  try {
    shellWidth.set(WINDOW_W);
    navCollapsed.set(false);
    const expandedW = WINDOW_W - SHELL_GAP * 3 - RAIL_W - PANEL_BORDER_X;
    const collapsedW = WINDOW_W - SHELL_GAP * 3 - RAIL_COLLAPSED_W - PANEL_BORDER_X;
    expect(shellPanelInnerWidth()).toBe(expandedW);

    const active = signal(true);
    const pane = createPaneWidth({
      active: () => active(),
      fromShell: (w) => w,
    });
    expect(pane.width()).toBe(expandedW);

    // Collapsing updates active page width synchronously without waiting for onLayout.
    navCollapsed.set(true);
    expect(pane.width()).toBe(collapsedW);

    // Hidden keep-alive pages freeze width while inactive and catch up when activated.
    active.set(false);
    navCollapsed.set(false);
    expect(pane.width()).toBe(collapsedW);
    active.set(true);
    expect(pane.width()).toBe(expandedW);

    // Standalone fallback (without Shell) gates onLayout by active().
    shellWidth.set(0);
    const standaloneActive = signal(true);
    const standalone = createPaneWidth({ active: () => standaloneActive() });
    standalone.onLayout({ bounds: { size: { width: 900 } } });
    expect(standalone.width()).toBe(900);
    standaloneActive.set(false);
    standalone.onLayout({ bounds: { size: { width: 1020 } } });
    expect(standalone.width()).toBe(900);
    standaloneActive.set(true);
    expect(standalone.width()).toBe(1020);
  } finally {
    shellWidth.set(prevShellW);
    navCollapsed.set(prevCollapsed);
  }
});

test("collapsing the sidebar preserves mode switching and theme toggling", async () => {
  const app = await open("./App.tsx", { width: WINDOW_W, height: WINDOW_H });
  // 等首装装载落地：本机有技能是卡面滚动容器，全新安装（空库）是空态。
  await app.waitUntil(
    () => app.shows("id:skills-scroll") || app.shows("id:my-skills-goto-marketplace")
  );
  await app.press("sidebar-collapse");
  await app.press("mode-accounts");
  await app.waitUntil(() => app.shows("id:acct-top-drag"));
  await app.press("sidebar-theme");
  await app.press("mode-skills");
  await app.waitUntil(() => app.shows("id:page-my_skills"));
  await app.press("sidebar-theme");
  await app.press("sidebar-collapse");
});
