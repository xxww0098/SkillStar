// 账号页测试 —— 页头 agent 胶囊、工具栏（搜索/三态视图/筛选/排序/全部刷新/隐私）、
// 批量选择条、可关闭横幅、双栏 well 额度卡片与三标签添加对话框。
// 分两半：纯逻辑（logic.ts，直接单测）与页面 UI（accounts.test.host.tsx 假桥装置）。

import { expect, open, test } from "nsh/test";
import { locale } from "../../services/locale";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  accountName,
  chunkPairs,
  creditText,
  filterAndSortAccounts,
  formatCreditValue,
  formatSpan,
  formatStamp,
  hasQuota,
  isServing,
  lastUsedStamp,
  liveAccountId,
  loginLine,
  maskAccountText,
  mergeAccountSnapshots,
  overallQuota,
  percentText,
  planKind,
  planLabel,
  remainingTone,
  resetCardCount,
  resetState,
  sortAccounts,
  windowChip,
  windowLabel,
  windowOrder,
  windowRows,
  type PlanKind,
} from "./logic";
import { planColors } from "./card";
import {
  classifyRefreshError,
  computeBackoffMs,
  computeStaggeredDelayMs,
  createAutoRefreshScheduler,
  nextQuickAutoRefreshMs,
  normalizeAutoRefreshMs,
  pickNextLruAccount,
  pickStaleAccounts,
  type AutoRefreshSlot,
} from "./autoRefresh";
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

test("plan badges classify every agent's level vocabulary and fall back to neutral", () => {
  // 顶档：Claude Max、Cursor / Antigravity Ultra、Grok Heavy 同档同色。
  expect(planKind("Ultra")).toBe("ultra");
  expect(planKind("MAX")).toBe("ultra");
  expect(planKind("GLM Coding Max")).toBe("ultra");
  expect(planKind("SuperGrok Heavy")).toBe("ultra");
  expect(planKind("G1-ULTRA-TIER")).toBe("ultra");
  expect(planKind("ULTRA 20X")).toBe("ultra");
  // 次顶：顶档的低倍率版，或 pro 的加量版。
  expect(planKind("G1-ULTRA-LITE-TIER")).toBe("ultra_lite");
  expect(planKind("ULTRA 5X")).toBe("ultra_lite");
  expect(planKind("MAX 5X")).toBe("ultra_lite");
  expect(planKind("KIRO PRO+")).toBe("ultra_lite");
  expect(planKind("PRO_PLUS")).toBe("ultra_lite");
  // pro / plus / lite 家族；Codex 的 Pro 两档 slug 分开上色。
  expect(planKind(" pro 20x ")).toBe("pro");
  expect(planKind("pro")).toBe("pro");
  expect(planKind("SuperGrok")).toBe("pro");
  expect(planKind("Kiro Pro")).toBe("pro");
  expect(planKind("prolite")).toBe("pro_lite");
  expect(planKind("PRO_LITE")).toBe("pro_lite");
  expect(planKind("pro lite")).toBe("pro_lite");
  expect(planKind("pro 5x")).toBe("pro_lite");
  expect(planKind("Plus")).toBe("plus");
  expect(planKind("X Premium+")).toBe("plus");
  expect(planKind("lite")).toBe("lite");
  expect(planKind("GLM Coding Lite")).toBe("lite");
  expect(planKind("standard-tier")).toBe("lite");
  expect(planKind("Go")).toBe("lite");
  // 组织档与免费档。
  expect(planKind("Claude Team")).toBe("team");
  expect(planKind("Business")).toBe("team");
  expect(planKind("enterprise pro")).toBe("enterprise");
  expect(planKind("ENTERPRISE")).toBe("enterprise");
  expect(planKind("free")).toBe("free");
  expect(planKind("custom")).toBe("none");
  expect(planKind("elite")).toBe("none");
  expect(planKind(null)).toBe("none");
});

test("antigravity tier codes share one badge vocabulary and other plans stay verbatim", () => {
  const antigravity = (plan: string) => account({ agent: "antigravity", plan });
  expect(planLabel(antigravity("G1-ULTRA-TIER"))).toBe("ULTRA 20X");
  expect(planLabel(antigravity("G1-ULTRA-LITE-TIER"))).toBe("ULTRA 5X");
  // 代号大小写不敏感，表外的代号退回去前缀/后缀的通用归一大写。
  expect(planLabel(antigravity("g1-ultra-lite-tier"))).toBe("ULTRA 5X");
  expect(planLabel(antigravity("G1-PRO-TIER"))).toBe("PRO");
  expect(planLabel(antigravity("standard-tier"))).toBe("STANDARD");
  expect(planLabel(antigravity("FREE"))).toBe("FREE");
  // Codex 的拼接 slug 补空格，其他 Agent 的套餐名不动。
  expect(planLabel(account({ agent: "codex", plan: "prolite" }))).toBe("PRO LITE");
  expect(planLabel(account({ agent: "codex", plan: "pro" }))).toBe("pro");
  expect(planLabel(antigravity(""))).toBe("");
  expect(planLabel(account({ agent: "cursor", plan: "G1-ULTRA-LITE-TIER" }))).toBe(
    "G1-ULTRA-LITE-TIER",
  );
  expect(
    planLabel(
      account({
        agent: "antigravity",
        plan: "G1-ULTRA-TIER",
        usage: { fetchedAt: 0, windows: [], plan: "G1-ULTRA-LITE-TIER" },
      }),
    ),
  ).toBe("ULTRA 5X");
  // 归一后的两档文案各自落一个色档：20X 与 5X 不再共用一套颜色。
  expect(planKind(planLabel(antigravity("G1-ULTRA-TIER")))).toBe("ultra");
  expect(planKind(planLabel(antigravity("G1-ULTRA-LITE-TIER")))).toBe("ultra_lite");
});

test("every plan tier paints its own color pair and the Ultra filter keeps both", () => {
  const kinds: PlanKind[] = [
    "ultra",
    "ultra_lite",
    "pro",
    "pro_lite",
    "plus",
    "lite",
    "team",
    "enterprise",
    "free",
  ];
  const pairs = kinds.map((kind) => {
    const colors = planColors(kind);
    return `${colors.bg}/${colors.fg}/${colors.border ?? ""}`;
  });
  expect(new Set(pairs).size).toBe(kinds.length);
  // 无套餐不上色，与 FREE 同档。
  expect(planColors("none")).toEqual(planColors("free"));

  const ultra = account({ id: "ag-20x", agent: "antigravity", plan: "G1-ULTRA-TIER" });
  const lite = account({ id: "ag-5x", agent: "antigravity", plan: "G1-ULTRA-LITE-TIER" });
  const filterBy = (filter: "ultra" | "all") =>
    filterAndSortAccounts([ultra, lite], {
      query: "",
      filter,
      sortBy: "default",
      sortDir: "desc",
      servingId: null,
    });
  expect(filterBy("ultra").map((row) => row.id)).toEqual([ultra.id, lite.id]);
  expect(filterBy("all").length).toBe(2);
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

test("long grouped window labels compact limit suffixes and avoid cramped 2-column pairing", () => {
  expect(
    windowLabel(win({ key: "gemini-weekly", label: "Gemini Models · Weekly Limit" })),
  ).toBe("Gemini Models · Weekly");
  expect(
    windowLabel(win({ key: "claude-5h", label: "Claude and GPT models · Five Hour Limit" })),
  ).toBe("Claude and GPT models · 5h");
  expect(windowLabel(win({ key: "prompt", label: "User Prompt credits" }))).toBe(
    "User Prompt credits",
  );
  expect(windowLabel(win({ key: "auto", label: "Auto + Composer" }))).toBe("Auto + Composer");

  const compactRows = windowRows([
    win({ key: "week", label: "Weekly" }),
    win({ key: "five_hour", label: "5h" }),
  ]);
  expect(compactRows.map((row) => row.map((entry) => entry.key))).toEqual([["five_hour", "week"]]);

  const groupedRows = windowRows([
    win({ key: "gemini-weekly", label: "Gemini Models · Weekly Limit" }),
    win({ key: "gemini-5h", label: "Gemini Models · Five Hour Limit" }),
    win({ key: "claude-weekly", label: "Claude and GPT models · Weekly Limit" }),
    win({ key: "claude-5h", label: "Claude and GPT models · Five Hour Limit" }),
  ]);
  expect(groupedRows.map((row) => row.map((entry) => entry.key))).toEqual([
    ["gemini-weekly"],
    ["gemini-5h"],
    ["claude-weekly"],
    ["claude-5h"],
  ]);
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

test("username display excludes email and ID placeholders", () => {
  expect(accountName(account({ displayName: "Google User", email: "user@example.com" }))).toBe("Google User");
  expect(accountName(account({ displayName: " USER@example.com ", email: "user@example.com" }))).toBe("");
  expect(accountName(account({ displayName: "Codex" }))).toBe("");
  expect(accountName(account({ displayName: "u-99", accountId: "u-99" }))).toBe("");
});

test("the login line used by actions prefers email, then provider accountId, then the record id", () => {
  expect(locale()).toBe("zh-CN");
  expect(loginLine(account({ id: "c1", email: "me@example.com", accountId: "u-99" }))).toBe(
    "me@example.com",
  );
  expect(loginLine(account({ id: "c1", email: "  ", accountId: "27741789126905948" }))).toBe(
    "27741789126905948",
  );
  expect(loginLine(account({ id: "c1", email: "  " }))).toBe("c1");
  expect(loginLine(account({ id: "c1" }))).toBe("c1");

  const withAccountId = account({
    id: "c2",
    displayName: "xxww0098",
    accountId: "27741789126905948",
  });
  expect(
    filterAndSortAccounts([withAccountId], {
      query: "27741789",
      filter: "all",
      sortBy: "default",
      sortDir: "desc",
      servingId: null,
    }).map((r) => r.id),
  ).toEqual(["c2"]);
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

test("an expired login retains its email line and shows the re-authorize row", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-card-cursor-1");
  expect(app.texts()).toContain(t("accountsPage.reauthNeeded"));
  expect(app.texts()).toContain("Cursor Work");
  expect(app.texts()).toContain(t("accountsPage.noEmail"));
  expect(app.shows("id:acct-email-cursor-1")).toBe(true);
  expect(app.shows("id:acct-reauth-cursor-1")).toBe(true);
  expect(app.shows("id:acct-quota-error-cursor-1")).toBe(true);
  expect(app.shows("id:acct-plan-cursor-1")).toBe(true);
  expect(app.texts()).toContain("38%");
});

test("switching activates, toasts the switch, and adds no banner", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-cursor");
  await app.press("rail-cursor");
  await app.waitFor("id:acct-switch-cursor-1");
  await app.waitUntil(() => app.texts().some((s) => s.includes("list:1")));
  await app.press("acct-switch-cursor-1");
  await app.waitUntil(() => app.texts().some((s) => s.includes("activate:1")));
  await app.host.send("harness", { action: "activate-ok" });

  await app.waitUntil(() => app.shows("id:acct-current-cursor-1"));
  // 切换成功后状态就地更新，不再冗余重拉 list；正文不插横幅，仅发 toast。
  expect(app.texts().some((s) => s.includes("list:1"))).toBe(true);
  expect(app.texts()).toContain(tf("accountsPage.switchedBanner", { name: "cursor-1" }));
  expect(app.shows("id:acct-banner")).toBe(false);
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
  // 失败留在可关闭的横幅上。
  await app.waitFor("id:acct-banner");
  await app.press("acct-banner-close");
  await app.waitUntil(() => !app.shows("id:acct-banner"));
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
  expect(app.openedUrls()).toEqual([
    "https://auth.example/oauth?redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fcallback",
  ]);
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
  // 成功反馈只走 toast，不再插一条页面横幅把列表顶下去。
  expect(app.shows("id:acct-banner")).toBe(false);
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
  expect(app.shows("id:acct-banner")).toBe(false);

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
  expect(app.shows("id:acct-banner")).toBe(false);
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
  expect(app.texts()).toContain("Primary Codex");
  expect(app.texts()).toContain("primary@example.com");
  await app.press("acct-view-list");
  await app.waitFor("id:acct-list-view");
  expect(app.texts()).toContain("Primary Codex");
  expect(app.texts()).toContain("primary@example.com");
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
  // 单卡（这里只勾了一张）成功同样是 toast，正文不插横幅。
  expect(app.texts()).toContain(`${t("common.delete")} ${t("common.done")}`);
  expect(app.shows("id:acct-banner")).toBe(false);
});

test("batch delete reports its count in a toast, not a page banner", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-selection-bar");
  await app.press("acct-select-all");
  await app.waitFor("id:acct-selected-count");
  await app.press("acct-batch-delete");
  await app.waitFor("id:dialog-confirm-ok");
  await app.press("dialog-confirm-ok");
  await app.waitUntil(() => app.texts().includes(tf("accountsPage.batchDeleted", { count: 2 })));
  expect(app.shows("id:acct-banner")).toBe(false);
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
  await app.waitUntil(() => !app.shows("id:acct-skeleton"), { timeout: 2000 });
  expect(app.shows("id:acct-empty")).toBe(true);

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

test("antigravity grouped quota windows render compacted labels cleanly across grid, compact, and list views", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-antigravity");
  await app.press("rail-antigravity");
  await app.waitFor("id:acct-card-ag-ultra");

  const expectedLabels = [
    "Gemini Models · Weekly",
    "Gemini Models · 5h",
    "Claude and GPT models · Weekly",
    "Claude and GPT models · 5h",
  ];
  const expectedPercents = ["63%", "84%", "99%", "100%"];

  expect(app.shows("id:acct-plan-ag-ultra")).toBe(true);
  expect(app.texts()).toContain("Example User");
  expect(app.texts()).toContain("user@example.com");
  expect(app.texts()).toContain("ULTRA 20X");
  expect(app.texts()).not.toContain("G1-ULTRA-TIER");
  for (const label of expectedLabels) {
    expect(app.texts()).toContain(label);
  }
  for (const pct of expectedPercents) {
    expect(app.texts()).toContain(pct);
  }

  await app.press("acct-view-compact");
  await app.waitFor("id:acct-compact-view");
  expect(app.texts()).toContain("Example User");
  expect(app.texts()).toContain("user@example.com");
  expect(app.texts()).toContain("ULTRA 20X");
  for (const label of expectedLabels) {
    expect(app.texts()).toContain(label);
  }
  for (const pct of expectedPercents) {
    expect(app.texts()).toContain(pct);
  }

  await app.press("acct-view-list");
  await app.waitFor("id:acct-list-view");
  expect(app.texts()).toContain("Example User");
  expect(app.texts()).toContain("user@example.com");
  expect(app.texts()).toContain("ULTRA 20X");
  for (const label of expectedLabels) {
    expect(app.texts()).toContain(label);
  }
  for (const pct of expectedPercents) {
    expect(app.texts()).toContain(pct);
  }
});

test("staggered auto-refresh computes dynamic step with min-gap floor and jitter", () => {
  expect(computeStaggeredDelayMs(30_000, 2, { jitterRatio: 0 })).toBe(15_000);
  expect(computeStaggeredDelayMs(30_000, 20, { minGapMs: 2_000, jitterRatio: 0 })).toBe(2_000);
  expect(computeStaggeredDelayMs(30_000, 2, { jitterRatio: 0.15, random: () => 1 })).toBe(17_250);
  expect(computeStaggeredDelayMs(30_000, 2, { jitterRatio: 0.15, random: () => 0 })).toBe(12_750);
});

test("only the offered cycles are valid saved auto-refresh intervals", () => {
  expect(normalizeAutoRefreshMs(null)).toBe(null);
  expect(normalizeAutoRefreshMs(30_000)).toBe(30_000);
  expect(normalizeAutoRefreshMs(900_000)).toBe(900_000);
  // 手改过文件、或旧版本存下的选项表里没有的周期：按关闭处理，不猜。
  expect(normalizeAutoRefreshMs(45_000)).toBe(null);
  expect(normalizeAutoRefreshMs(0)).toBe(null);
});

test("staggered auto-refresh picks LRU eligible account and skips cooling/reauth/in-flight", () => {
  const now = 1_000_000;
  const rows = [
    account({ id: "a" }),
    account({ id: "b" }),
    account({ id: "c", requiresReauth: true }),
  ];
  const slots: Record<string, AutoRefreshSlot> = {
    a: { lastRefreshedAt: now - 5_000, cooldownUntil: null, consecutiveErrors: 0, isForbidden: false },
    b: { lastRefreshedAt: now - 10_000, cooldownUntil: null, consecutiveErrors: 0, isForbidden: false },
  };
  expect(pickNextLruAccount(rows, slots, now, new Set())).toBe("b");
  expect(pickNextLruAccount(rows, slots, now, new Set(["b"]))).toBe("a");
  slots.b.cooldownUntil = now + 30_000;
  expect(pickNextLruAccount(rows, slots, now, new Set())).toBe("a");
  expect(classifyRefreshError("HTTP 429 Too Many Requests")).toBe("rate_limited");
  expect(classifyRefreshError("403 Forbidden")).toBe("transient");
  expect(classifyRefreshError("login required: credential rejected: HTTP 401")).toBe("auth_required");
  expect(classifyRefreshError("401 Unauthorized")).toBe("auth_required");
  expect(classifyRefreshError("network timeout")).toBe("transient");
});

test("pickStaleAccounts selects expired or unfetched accounts and nextQuickAutoRefreshMs cycles cleanly", () => {
  const nowMs = 2_000_000_000;
  const nowSecs = Math.floor(nowMs / 1000);
  const rows = [
    account({ id: "fresh", usage: { fetchedAt: nowSecs - 20, windows: [] } }),
    account({ id: "stale-snap", usage: { fetchedAt: nowSecs - 120, windows: [] } }),
    account({ id: "unfetched", usage: null }),
    account({ id: "reauth", requiresReauth: true, usage: null }),
  ];
  const slots: Record<string, AutoRefreshSlot> = {
    "stale-snap": {
      lastRefreshedAt: nowMs - 10_000,
      cooldownUntil: null,
      consecutiveErrors: 0,
      isForbidden: false,
    },
  };
  // stale-snap 内存刷新时刻仅过 10s，不再重复补刷；unfetched 尚无快照需补刷。
  expect(pickStaleAccounts(rows, slots, nowMs, new Set(), 60)).toEqual(["unfetched"]);
  expect(pickStaleAccounts(rows, {}, nowMs, new Set(), 60)).toEqual(["stale-snap", "unfetched"]);

  expect(nextQuickAutoRefreshMs(null)).toBe(300_000);
  expect(nextQuickAutoRefreshMs(60_000)).toBe(300_000);
  expect(nextQuickAutoRefreshMs(300_000)).toBe(900_000);
  expect(nextQuickAutoRefreshMs(900_000)).toBe(null);
});

test("computeBackoffMs applies exponential backoff capped at 10m for both transient and rate-limited errors", () => {
  expect([1, 2, 3, 4, 5, 6, 7, 8].map((n) => computeBackoffMs("transient", n))).toEqual([
    15_000,
    30_000,
    60_000,
    120_000,
    240_000,
    480_000,
    600_000,
    600_000,
  ]);
  expect([1, 2, 3, 4, 5].map((n) => computeBackoffMs("rate_limited", n))).toEqual([
    60_000,
    120_000,
    240_000,
    480_000,
    600_000,
  ]);
});

test("pickNextLruAccount advances past failed attempts using lastAttemptedAt so healthy accounts are not starved", () => {
  const nowMs = 2_000_000_000;
  const rows = [
    account({ id: "a", usage: { fetchedAt: 1_000, windows: [] } }),
    account({ id: "b", usage: { fetchedAt: 1_100, windows: [] } }),
  ];
  const slots: Record<string, AutoRefreshSlot> = {
    a: {
      lastRefreshedAt: null,
      lastAttemptedAt: 1_500_000,
      cooldownUntil: nowMs - 1_000,
      consecutiveErrors: 1,
      isForbidden: false,
    },
  };
  expect(pickNextLruAccount(rows, slots, nowMs, new Set())).toBe("b");

  slots.b = {
    lastRefreshedAt: 1_600_000,
    lastAttemptedAt: 1_600_000,
    cooldownUntil: null,
    consecutiveErrors: 0,
    isForbidden: false,
  };
  expect(pickNextLruAccount(rows, slots, nowMs, new Set())).toBe("a");
});

test("auto-refresh scheduler survives single-account in-flight and schedules wakeups across full cooldown", () => {
  let savedMs: number | null = 30_000;
  let inFlight: string[] = [];
  const rows = [account({ id: "solo", usage: { fetchedAt: 1_000, windows: [] } })];
  const scheduler = createAutoRefreshScheduler({
    prefs: {
      load: () => savedMs,
      save: (ms) => {
        savedMs = ms;
      },
    },
    getCandidates: () => rows,
    getInFlightIds: () => inFlight,
    onTickAccount: async () => {},
    jitterRatio: 0,
  });

  try {
    scheduler.reschedule();
    const initialTick = scheduler.nextTickAt();
    expect(initialTick).not.toBeNull();

    inFlight = ["solo"];
    scheduler.reschedule();
    expect(scheduler.nextTickAt()).toBe(initialTick);

    inFlight = [];
    const t0 = Date.now();
    scheduler.markError("solo", "network timeout", t0);
    const slot1 = scheduler.slots().solo;
    expect(slot1.consecutiveErrors).toBe(1);
    expect(slot1.cooldownUntil).toBe(t0 + 15_000);
    expect(slot1.lastAttemptedAt).toBe(t0);
    expect(scheduler.nextTickAt()).not.toBeNull();

    const t1 = t0 + 15_000;
    scheduler.markError("solo", "network timeout", t1);
    scheduler.markError("solo", "network timeout", t1 + 1);
    const slot3 = scheduler.slots().solo;
    expect(slot3.consecutiveErrors).toBe(3);
    expect(slot3.cooldownUntil).toBe(t1 + 1 + 60_000);
    expect(scheduler.nextTickAt()).not.toBeNull();
    expect(scheduler.nextTickAt()!).toBeGreaterThanOrEqual(slot3.cooldownUntil!);

    scheduler.markRefreshed("solo", t1 + 2);
    const recovered = scheduler.slots().solo;
    expect(recovered.consecutiveErrors).toBe(0);
    expect(recovered.cooldownUntil).toBeNull();
    expect(scheduler.nextTickAt()).not.toBeNull();
  } finally {
    scheduler.dispose();
  }
});

test("auto-refresh toolbar menu toggles interval and shows global multi-agent staggered preview", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-auto-refresh-trigger");
  expect(app.texts()).toContain(t("accountsPage.autoRefreshOff"));
  // 装置初始未设周期 = 关闭；选了周期后保存。
  expect(app.texts()).toContain("prefs:off");

  await app.press("acct-auto-refresh-trigger");
  await app.waitFor("id:acct-auto-refresh-panel");
  await app.press("acct-auto-refresh-opt-30000");
  await app.waitUntil(() => !app.shows("id:acct-auto-refresh-panel"));
  expect(app.texts()).toContain(tf("accountsPage.autoRefreshLabel", { label: "30 秒" }));
  expect(app.texts()).toContain("prefs:30000");

  await app.press("acct-auto-refresh-trigger");
  await app.waitFor("id:acct-auto-refresh-preview");
  // 全库 5 张卡里除 cursor-1 (requiresReauth) 外共 4 张可用，30s / 4 ≈ 8s 步进。
  expect(app.texts()).toContain(tf("accountsPage.autoRefreshPreview", { count: 4, step: 8 }));

  await app.press("acct-auto-refresh-opt-off");
  await app.waitUntil(() => !app.shows("id:acct-auto-refresh-panel"));
  expect(app.texts()).toContain(t("accountsPage.autoRefreshOff"));
  expect(app.texts()).toContain("prefs:off");
});

test("codex reset card count normalizes in logic and renders across grid, compact, and list views", async () => {
  expect(resetCardCount(null)).toBeNull();
  expect(resetCardCount({ fetchedAt: 1, windows: [], resetCredits: -1 })).toBeNull();
  expect(resetCardCount({ fetchedAt: 1, windows: [], resetCredits: 0 })).toBe(0);
  expect(resetCardCount({ fetchedAt: 1, windows: [], resetCredits: 2.9 })).toBe(2);
  expect(hasQuota({ fetchedAt: 1, windows: [], resetCredits: 2 })).toBe(true);
  expect(hasQuota({ fetchedAt: 1, windows: [], resetCredits: null })).toBe(false);

  const prev = [
    account({ id: "c1", usage: { fetchedAt: 10, windows: [], resetCredits: 2 } }),
  ];
  const same = [
    account({ id: "c1", usage: { fetchedAt: 10, windows: [], resetCredits: 2 } }),
  ];
  const changed = [
    account({ id: "c1", usage: { fetchedAt: 10, windows: [], resetCredits: 1 } }),
  ];
  expect(mergeAccountSnapshots(prev, same)[0]).toBe(prev[0]);
  expect(mergeAccountSnapshots(prev, changed)[0]).not.toBe(prev[0]);

  const app = await openHost();
  await app.waitFor("id:acct-card-codex-main");
  expect(app.shows("id:acct-reset-cards")).toBe(true);
  expect(app.texts()).toContain(`${t("accountsPage.resetCards")}:`);
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 2 }));
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 0 }));

  await app.press("acct-view-compact");
  await app.waitFor("id:acct-compact-view");
  expect(app.shows("id:acct-reset-cards")).toBe(true);
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 2 }));
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 0 }));

  await app.press("acct-view-list");
  await app.waitFor("id:acct-list-view");
  expect(app.shows("id:acct-reset-cards")).toBe(true);
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 2 }));
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 0 }));
});

test("kiro idc and zcode bigmodel login options are forwarded and supportsOauth=false button is disabled", async () => {
  const app = await openHost();
  await app.waitFor("id:rail-kiro");
  await app.press("rail-kiro");
  await app.waitFor("id:acct-toolbar-add");
  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-oauth-modes");

  await app.press("acct-oauth-mode-idc");
  await app.waitFor("id:acct-idc-region");
  await app.type("id:acct-idc-region", "eu-west-1");
  await app.type("id:acct-idc-start-url", "https://d-123.awsapps.com/start");
  await app.press("acct-add-oauth");
  await app.waitFor("id:acct-add-waiting");
  await app.host.send("harness", { action: "last-call" });
  await app.waitUntil(() =>
    app
      .texts()
      .some((s) =>
        s.includes('start=kiro|{"leg":"idc","idcRegion":"eu-west-1","idcStartUrl":"https://d-123.awsapps.com/start"}'),
      ),
  );
  await app.press("acct-add-cancel");
  await app.waitUntil(() => !app.shows("id:acct-add-card"));

  await app.press("rail-zcode");
  await app.waitFor("id:acct-add-zcode");
  await app.press("acct-add-zcode");
  await app.waitFor("id:acct-oauth-modes");
  await app.press("acct-oauth-mode-bigmodel");
  await app.press("acct-add-oauth");
  await app.waitFor("id:acct-add-waiting");
  await app.host.send("harness", { action: "last-call" });
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes('start=zcode|{"provider":"bigmodel"}')),
  );
  await app.press("acct-add-cancel");
  await app.waitUntil(() => !app.shows("id:acct-add-card"));

  await app.press("rail-claude-code");
  await app.waitFor("id:acct-add-claude-code");
  await app.press("acct-add-claude-code");
  await app.waitFor("id:acct-tab-oauth");
  await app.press("acct-tab-oauth");
  await app.waitFor("id:acct-add-oauth");
  await expect(app.press("acct-add-oauth")).rejects.toThrow(/disabled/i);
  expect(app.shows("id:acct-add-waiting")).toBe(false);
  expect(app.texts().some((s) => s.includes("start:2"))).toBe(true);
});

test("oauth finishLogin rejection clears waiting state and ErrorBanner dismiss clears error", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-toolbar-add");
  await app.press("acct-toolbar-add");
  await app.waitFor("id:acct-add-oauth");
  await app.press("acct-add-oauth");
  await app.waitFor("id:acct-add-waiting");

  await app.host.send("harness", { action: "finish-err" });
  await app.waitUntil(() => !app.shows("id:acct-add-waiting"));
  expect(app.shows("id:acct-add-oauth")).toBe(true);
  expect(app.texts()).toContain("login was rejected by the provider");

  await app.press("acct-add-close");
  await app.waitUntil(() => !app.shows("id:acct-add-card"));

  await app.host.send("harness", { action: "set-error" });
  await app.waitFor("id:acct-error");
  expect(app.texts()).toContain("list load failed from domain");
  await app.press("acct-error-close");
  await app.waitUntil(() => !app.shows("id:acct-error"));
});

test("manual card refresh marks reauth on AuthRequired 401 and keeps 403 non-reauth", async () => {
  const app = await openHost();
  await app.waitFor("id:acct-card-codex-main");

  await app.host.send("harness", { action: "refresh-err-403" });
  await app.press("acct-refresh-codex-main");
  await app.waitFor("id:acct-banner");
  expect(app.texts()).toContain(`${t("common.refresh")}: usage unavailable: HTTP 403 Forbidden`);
  expect(app.shows("id:acct-reauth-codex-main")).toBe(false);

  await app.host.send("harness", { action: "refresh-err-401" });
  await app.press("acct-refresh-codex-main");
  await app.waitFor("id:acct-reauth-codex-main");
  expect(app.shows("id:acct-reauth-codex-main")).toBe(true);
});
