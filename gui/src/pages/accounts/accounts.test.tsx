// 账号页测试 —— 页头 agent 胶囊、工具栏（搜索/三态视图/筛选/排序/全部刷新/隐私）、
// 批量选择条、可关闭横幅、双栏 well 额度卡片与三标签添加对话框。
// 分两半：纯逻辑（logic.ts，直接单测）与页面 UI（accounts.test.host.tsx 假桥装置）。

import { expect, open, test } from "nsh/test";
import { locale } from "../../services/locale";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  chunkPairs,
  creditText,
  filterAndSortAccounts,
  formatCreditValue,
  formatSpan,
  formatStamp,
  isServing,
  lastUsedStamp,
  liveAccountId,
  loginLine,
  maskAccountText,
  overallQuota,
  percentText,
  planKind,
  remainingTone,
  resetState,
  sortAccounts,
  windowChip,
  windowLabel,
  windowOrder,
} from "./logic";
import type { AccountView, UsageWindow } from "./types";

const HOST = "./accounts.test.host.tsx";

function win(over: Partial<UsageWindow>): UsageWindow {
  return { key: "week", label: "Weekly", remainingPercent: 50, unit: "percent", ...over };
}

function account(over: Partial<AccountView>): AccountView {
  return {
    id: "x",
    agent: "codex",
    displayName: "x",
    createdAt: 0,
    requiresReauth: false,
    isActive: false,
    ...over,
  };
}

// ---- 纯逻辑（语义钉死） --------------------------------------------------------

test("plan badges classify ultra/plus/team/pro/enterprise/free and fall back to neutral", () => {
  expect(planKind("Ultra")).toBe("ultra");
  expect(planKind("Plus")).toBe("plus");
  expect(planKind(" pro 20x ")).toBe("pro");
  expect(planKind("Claude Team")).toBe("team");
  expect(planKind("enterprise pro")).toBe("enterprise");
  expect(planKind("free")).toBe("free");
  expect(planKind("custom")).toBe("none");
  expect(planKind(null)).toBe("none");
});

test("the remaining meter bands at 80/40/10", () => {
  expect(remainingTone(100)).toBe("ok");
  expect(remainingTone(80)).toBe("ok");
  expect(remainingTone(79)).toBe("warn");
  expect(remainingTone(40)).toBe("warn");
  expect(remainingTone(39)).toBe("amber");
  expect(remainingTone(10)).toBe("amber");
  expect(remainingTone(9)).toBe("danger");
  expect(remainingTone(0)).toBe("danger");
  expect(percentText(74.6)).toBe("75%");
  expect(percentText(140)).toBe("100%");
  expect(percentText(-4)).toBe("0%");
});

test("windows order five_hour -> week -> the rest, then credits stay apart", () => {
  const ordered = windowOrder([
    win({ key: "week", label: "Weekly" }),
    win({ key: "opus", label: "Opus" }),
    win({ key: "five_hour", label: "5h" }),
    win({ key: "sonnet", label: "Sonnet" }),
  ]);
  expect(ordered.map((entry) => entry.key)).toEqual(["five_hour", "week", "opus", "sonnet"]);
  expect(windowLabel(win({ key: "five_hour", label: "anything" }))).toBe("5h");
  expect(windowLabel(win({ key: "opus", label: "  " }))).toBe("opus");
  expect(windowChip(win({ used: 13, total: 100, unit: "count" }))).toBe("13/100");
  expect(windowChip(win({ used: 300, total: 1200, unit: "usd-cents" }))).toBe("$3.00/$12.00");
  expect(windowChip(win({ used: 13 }))).toBeNull();
  expect(chunkPairs([1, 2, 3])).toEqual([[1, 2], [3]]);
  expect(chunkPairs([])).toEqual([]);
});

test("credits read as a value, a dollar amount, or unlimited", () => {
  expect(creditText({ label: "Credits", remaining: 12.5, unlimited: false, unit: "count" })).toEqual({
    unlimited: false,
    text: "12.5",
  });
  expect(creditText({ label: "Credits", unlimited: true, unit: "count" })).toEqual({
    unlimited: true,
    text: "",
  });
  expect(creditText({ label: "Balance", remaining: 300, unlimited: false, unit: "usd-cents" })).toEqual({
    unlimited: false,
    text: "$3.00",
  });
});

test("the reset line counts down and flips to reset once the time passes", () => {
  const now = 1763200000;
  expect(formatSpan(3 * 86400 + 4 * 3600)).toBe("3d 4h");
  expect(formatSpan(31 * 60)).toBe("31m");
  expect(formatSpan(0)).toBe("0m");
  const at = now + 3 * 86400 + 4 * 3600;
  const live = resetState(at, now);
  expect(live.kind).toBe("in");
  if (live.kind === "in") {
    expect(live.span).toBe("3d 4h");
    expect(live.when).toBe(formatStamp(at));
  }
  expect(resetState(now - 1, now).kind).toBe("done");
  expect(resetState(null, now).kind).toBe("none");
  expect(resetState(0, now).kind).toBe("none");
});

test("the serving account sorts first and reconcile is the second reading", () => {
  const rows = [
    account({ id: "a", createdAt: 1 }),
    account({ id: "b", createdAt: 2 }),
    account({ id: "c", createdAt: 3 }),
  ];
  expect(sortAccounts(rows, "c").map((row) => row.id)).toEqual(["c", "a", "b"]);
  expect(sortAccounts(rows, null).map((row) => row.id)).toEqual(["a", "b", "c"]);
  expect(isServing(account({ id: "a", isActive: true }), null)).toBe(true);
  expect(isServing(account({ id: "a" }), "a")).toBe(true);
  expect(isServing(account({ id: "a" }), "b")).toBe(false);
  const live = [
    { agent: "codex", identity: null, accountId: "a", source: "cli" },
    { agent: "cursor", identity: null, accountId: null, source: "cli" },
  ];
  expect(liveAccountId(live, "codex")).toBe("a");
  expect(liveAccountId(live, "cursor")).toBeNull();
  expect(liveAccountId(live, "claude-code")).toBeNull();
});

test("privacy masking, overall quota, lastUsedStamp, and filter/sort work together", () => {
  expect(maskAccountText("primary@example.com", true)).toBe("prim***@example.com");
  expect(maskAccountText("primary@example.com", false)).toBe("primary@example.com");
  expect(maskAccountText("Primary Codex", true)).not.toBe("Primary Codex");

  expect(lastUsedStamp(account({ lastUsedAt: null }))).toBeNull();
  expect(lastUsedStamp(account({ lastUsedAt: 1763200000 }))).toBe(formatStamp(1763200000));

  const a = account({
    id: "a",
    displayName: "Alpha",
    email: "alpha@example.com",
    plan: "pro",
    createdAt: 1,
    lastUsedAt: 100,
    usage: {
      fetchedAt: 10,
      windows: [win({ key: "five_hour", remainingPercent: 90 }), win({ remainingPercent: 70 })],
    },
  });
  const b = account({
    id: "b",
    displayName: "Beta",
    email: "beta@example.com",
    plan: "ultra",
    createdAt: 3,
    lastUsedAt: 300,
    usage: {
      fetchedAt: 10,
      windows: [win({ key: "five_hour", remainingPercent: 30 })],
    },
  });
  const c = account({
    id: "c",
    displayName: "Gamma",
    requiresReauth: true,
    createdAt: 2,
    lastUsedAt: 200,
  });

  expect(overallQuota(a)).toBe(80);
  expect(overallQuota(c)).toBe(-1);

  const rows = [a, b, c];
  expect(
    filterAndSortAccounts(rows, {
      query: "beta",
      filter: "all",
      sortBy: "default",
      sortDir: "desc",
      servingId: "a",
    }).map((r) => r.id),
  ).toEqual(["b"]);

  expect(
    filterAndSortAccounts(rows, {
      query: "",
      filter: "reauth",
      sortBy: "default",
      sortDir: "desc",
      servingId: "a",
    }).map((r) => r.id),
  ).toEqual(["c"]);

  expect(
    filterAndSortAccounts(rows, {
      query: "",
      filter: "all",
      sortBy: "quota",
      sortDir: "desc",
      servingId: "b",
    }).map((r) => r.id),
  ).toEqual(["a", "b", "c"]);

  expect(
    filterAndSortAccounts(rows, {
      query: "",
      filter: "all",
      sortBy: "last_used",
      sortDir: "asc",
      servingId: "b",
    }).map((r) => r.id),
  ).toEqual(["a", "c", "b"]);

  expect(
    filterAndSortAccounts(rows, {
      query: "",
      filter: "all",
      sortBy: "created_at",
      sortDir: "desc",
      servingId: "a",
    }).map((r) => r.id),
  ).toEqual(["b", "c", "a"]);
});

test("the login line prefers the email and falls back to the account id", () => {
  expect(locale()).toBe("zh-CN");
  expect(loginLine(account({ id: "c1", email: "me@example.com" }))).toBe("me@example.com");
  expect(loginLine(account({ id: "c1", email: "  " }))).toBe("c1");
  expect(loginLine(account({ id: "c1" }))).toBe("c1");
});

test("formatCreditValue converts credits objects into display text without [object Object]", () => {
  expect(
    formatCreditValue({ label: "Credits", remaining: 12.5, unlimited: false, unit: "count" }, "无限"),
  ).toBe("12.5");
  expect(
    formatCreditValue({ label: "Credits", unlimited: true, unit: "count" }, "无限"),
  ).toBe("无限");
  expect(
    formatCreditValue({ label: "Balance", remaining: 300, unlimited: false, unit: "usd-cents" }, "Unlimited"),
  ).toBe("$3.00");
});

// ---- 页面 UI（假桥装置） --------------------------------------------------------

async function openHost(width = 1280, height = 900) {
  return open(HOST, { width, height });
}

test("left sidebar lists the three agents with counts without duplicate top pills", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-codex");
  expect(app.shows("id:acct-rail")).toBe(false);
  for (const agent of ["codex", "claude-code", "cursor"]) {
    expect(app.shows(`id:rail-${agent}`)).toBe(true);
    expect(app.shows(`id:rail-icon-${agent}`)).toBe(true);
  }
  expect(app.texts()).toContain("Codex");
  expect(app.texts()).toContain("Claude Code");
  expect(app.texts()).toContain("Cursor");
  expect(app.texts()).toContain("2");
  expect(app.texts()).toContain("0");
  expect(app.texts()).toContain("1");
  expect(app.texts()).toContain("Primary Codex");
  expect(app.texts()).toContain("Backup Ultra");
});

test("cards render active pill, ultra/pro badges, meter tones, and cockpit typography", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-card-codex-main");
  await app.waitFor("id:acct-card-codex-ultra");

  expect(app.shows("id:acct-current-codex-main")).toBe(true);
  expect(app.texts()).toContain(t("accountsPage.current"));
  expect(app.shows("id:acct-plan-codex-main")).toBe(true);
  expect(app.shows("id:acct-plan-codex-ultra")).toBe(true);
  expect(app.texts()).toContain("PRO");
  expect(app.texts()).toContain("ULTRA");

  expect(app.shows("id:acct-pct-five_hour")).toBe(true);
  expect(app.texts()).toContain("92%");
  expect(app.texts()).toContain("75%");
  expect(app.texts()).toContain("25%");
  expect(app.texts()).toContain("12.5");
  expect(app.texts()).toContain("primary@example.com");

  const name = app.spanStyle("text:Primary Codex");
  expect(name.fontSize).toBe(14);
  expect(name.fontWeight).toBe(600);

  const currentStyle = app.spanStyle("id:acct-current-codex-main >> text:当前");
  expect(currentStyle.fontSize).toBe(11);
  expect(currentStyle.fontWeight).toBe(600);
  expect(currentStyle.color.toLowerCase()).toBe((paletteToken("ok") + "ff").toLowerCase());

  const okPercent = app.spanStyle("text:92%");
  expect(okPercent.fontSize).toBe(11);
  expect(okPercent.fontWeight).toBe(700);
  expect(okPercent.color.toLowerCase()).toBe((paletteToken("ok") + "ff").toLowerCase());

  const warnPercent = app.spanStyle("text:75%");
  expect(warnPercent.color.toLowerCase()).toBe((paletteToken("warn") + "ff").toLowerCase());

  const amberPercent = app.spanStyle("text:25%");
  expect(amberPercent.color.toLowerCase()).toBe((paletteToken("amber") + "ff").toLowerCase());

  const plan = app.spanStyle("text:PRO");
  expect(plan.fontSize).toBe(11);
  expect(plan.fontWeight).toBe(600);
});

test("an expired login swaps the identity line for the re-authorize row", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-card-cursor-1");
  expect(app.texts()).toContain(t("accountsPage.reauthNeeded"));
  expect(app.shows("id:acct-reauth-cursor-1")).toBe(true);
  expect(app.shows("id:acct-quota-error-cursor-1")).toBe(true);
  expect(app.shows("id:acct-plan-cursor-1")).toBe(true);
  expect(app.texts()).toContain("38%");
});

test("switching activates, shows a dismissible switched banner, and errors stay recoverable", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-switch-cursor-1");
  await app.waitUntil(() => app.texts().some((s) => s.includes("list:1")));
  await app.press("acct-switch-cursor-1");
  await app.waitUntil(() => app.texts().some((s) => s.includes("activate:1")));
  await app.host.send("harness", { action: "activate-ok" });

  await app.waitUntil(() => app.texts().some((s) => s.includes("list:2")));
  await app.waitUntil(() => app.shows("id:acct-current-cursor-1"));
  await app.waitFor("id:acct-banner");
  expect(app.texts()).toContain(tf("accountsPage.switchedBanner", { name: "cursor-1" }));

  await app.press("acct-banner-close");
  await app.waitUntil(() => !app.shows("id:acct-banner"));
});

test("a refused switch keeps the old active mark and reports the domain error", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-switch-cursor-1");
  await app.press("acct-switch-cursor-1");
  await app.host.send("harness", { action: "activate-err" });
  await app.waitUntil(() => app.texts().some((s) => s.includes("switch refused by the client")));
  expect(app.shows("id:acct-current-cursor-1")).toBe(false);
});

test("claude-code empty state opens directly to local tab and shows oauth hint on switch", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-claude-code");
  await app.press("rail-claude-code");
  await app.waitFor("id:acct-add-claude-code");
  // 空态是一整块虚线面板（占满工具栏以下的窗口高度），不另设页内占位卡。
  expect(app.shows("id:acct-empty")).toBe(true);
  expect(app.texts()).toContain(tf("accountsPage.emptyTitle", { agent: "Claude Code" }));
  expect(app.texts()).toContain(t("accountsPage.emptyHint"));

  await app.press("acct-add-claude-code");
  await app.waitFor("id:acct-add-card");
  expect(app.shows("id:acct-tab-oauth")).toBe(true);
  expect(app.shows("id:acct-add-token")).toBe(true);
  expect(app.shows("id:acct-tab-local")).toBe(true);
  expect(app.shows("id:acct-add-local")).toBe(true);

  await app.press("acct-tab-oauth");
  await app.waitFor("id:acct-add-oauth");
  expect(app.texts()).toContain(tf("accountsPage.noOauthHint", { agent: "Claude Code" }));

  await app.press("acct-add-close");
  await app.waitUntil(() => !app.shows("id:acct-add-card"));
});

test("the oauth flow waits, takes a pasted callback, and lands the account", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-toolbar-add");
  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-add-card");
  expect(app.shows("id:acct-add-agent-icon")).toBe(true);
  await app.press("acct-add-oauth");

  await app.waitFor("id:acct-add-waiting");
  expect(app.shows("id:acct-add-open")).toBe(true);
  expect(app.shows("id:acct-add-copy-url")).toBe(true);
  expect(app.shows("id:acct-add-callback")).toBe(true);
  expect(app.texts()).toContain(t("accountsPage.waiting"));

  await app.press("acct-add-callback-submit");
  expect(app.texts()).toContain(t("accountsPage.needCallback"));

  await app.type("id:acct-add-callback", "http://127.0.0.1:1455/callback?code=abc");
  await app.press("acct-add-callback-submit");
  await app.waitUntil(() => app.texts().some((s) => s.includes("callback:1")));
  await app.host.send("harness", { action: "finish-ok" });

  await app.waitUntil(() => !app.shows("id:acct-add-card"));
  expect(app.texts()).toContain(t("accountsPage.added"));
  await app.waitUntil(() => app.shows("id:acct-card-codex-new"));
});

test("closing the oauth dialog cancels the pending login", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-toolbar-add");
  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-add-oauth");
  await app.press("acct-add-oauth");
  await app.waitFor("id:acct-add-waiting");
  await app.press("acct-add-cancel");
  await app.waitUntil(() => !app.shows("id:acct-add-card"));
  await app.waitUntil(() => app.texts().some((s) => s.includes("cancel:1")));
});

test("local import and token import tabs both add an account", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-toolbar-add");
  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-tab-local");
  await app.press("acct-tab-local");
  await app.waitFor("id:acct-add-local");
  await app.press("acct-add-local");
  await app.waitUntil(() => app.shows("id:acct-card-cursor-local"));
  expect(app.texts()).toContain(t("accountsPage.importedLocal"));

  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-add-token");
  await app.press("acct-add-token");
  await app.waitFor("id:acct-add-token-input");
  await app.press("acct-add-token-submit");
  expect(app.texts()).toContain(t("accountsPage.needToken"));
  await app.type("id:acct-add-token-input", "sk-token-1");
  await app.press("acct-add-token-submit");
  await app.waitUntil(() => app.shows("id:acct-card-cursor-token"));
  expect(app.texts()).toContain(t("accountsPage.importedToken"));
  await app.host.send("harness", { action: "last-call" });
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("token=cursor|sk-token-1")),
  );
});

test("toolbar search, clear filters, privacy masking, and 3-way view switcher work", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-card-codex-main");
  expect(app.shows("id:acct-grid-view")).toBe(true);

  await app.type("id:acct-search-input", "nonexistent");
  await app.waitFor("id:acct-no-match");
  await app.press("acct-clear-filters");
  await app.waitFor("id:acct-card-codex-main");

  await app.press("acct-toolbar-privacy");
  await app.waitUntil(() => app.texts().includes("prim***@example.com"));
  expect(app.texts()).not.toContain("Primary Codex");
  expect(app.texts()).not.toContain("primary@example.com");

  await app.press("acct-toolbar-privacy");
  await app.waitUntil(() => app.texts().includes("Primary Codex"));
  expect(app.texts()).toContain("primary@example.com");

  await app.press("acct-view-compact");
  await app.waitFor("id:acct-compact-view");
  await app.press("acct-view-list");
  await app.waitFor("id:acct-list-view");
  await app.press("acct-view-grid");
  await app.waitFor("id:acct-grid-view");
});

test("selection bar selects all, clears selection, and batch deletes after confirmation", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-selection-bar");
  await app.press("acct-select-all");
  await app.waitFor("id:acct-selected-count");
  expect(app.texts()).toContain(tf("accountsPage.selectedCount", { count: 2 }));

  await app.press("acct-select-clear");
  await app.waitUntil(() => !app.shows("id:acct-selected-count"));

  await app.press("acct-select-codex-ultra");
  await app.waitFor("id:acct-batch-delete");
  await app.press("acct-batch-delete");
  await app.waitFor("id:dialog-confirm-ok");
  await app.press("dialog-confirm-ok");

  await app.waitUntil(() => !app.shows("id:acct-card-codex-ultra"));
  expect(app.shows("id:acct-card-codex-main")).toBe(true);
});

test("capability-driven non-oauth agent shows brand icon, loading skeleton, and local source metadata", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-devin-desktop");
  await app.press("rail-devin-desktop");
  await app.waitFor("id:acct-add-devin-desktop");
  expect(app.shows("id:acct-empty")).toBe(true);
  expect(app.shows("id:acct-empty-icon-devin-desktop")).toBe(true);
  expect(app.texts()).toContain(tf("accountsPage.emptyTitle", { agent: "Devin Desktop" }));

  // 加载骨架（AccountsSkeletonGrid）与空态互斥切换
  await app.host.send("harness", { action: "loading-on" });
  await app.waitFor("id:acct-skeleton");
  expect(app.shows("id:acct-empty")).toBe(false);
  await app.host.send("harness", { action: "loading-off" });
  await app.waitFor("id:acct-empty");
  expect(app.shows("id:acct-skeleton")).toBe(false);

  // supportsOauth=false 的任何新增 agent 均直达 local Tab 且渲染 backend localSource
  await app.press("acct-add-devin-desktop");
  await app.waitFor("id:acct-add-card");
  expect(app.shows("id:acct-add-local")).toBe(true);
  expect(app.texts()).toContain(
    tf("accountsPage.localSourcePath", { source: "Devin Desktop state.vscdb" }),
  );

  await app.press("acct-tab-oauth");
  await app.waitFor("id:acct-add-oauth");
  expect(app.texts()).toContain(tf("accountsPage.noOauthHint", { agent: "Devin Desktop" }));
});

test("credits-only accounts format cleanly in compact and list views without [object Object]", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-kiro");
  await app.press("rail-kiro");
  await app.waitFor("id:acct-card-kiro-credits");

  await app.press("acct-view-compact");
  await app.waitFor("id:acct-compact-view");
  expect(app.texts()).toContain("Credits 12.5");
  expect(app.texts().some((line) => line.includes("[object Object]"))).toBe(false);

  await app.press("acct-view-list");
  await app.waitFor("id:acct-list-view");
  expect(app.texts()).toContain("Credits · 12.5");
  expect(app.texts().some((line) => line.includes("[object Object]"))).toBe(false);
});
