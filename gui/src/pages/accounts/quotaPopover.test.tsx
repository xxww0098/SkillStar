// 全局账号额度下拉窗单测与交互测试：覆盖纯逻辑（分组过滤、备选排序、异常聚合、
// 多显示器边界夹紧定位）、偏好存储桥接，以及面板展开切号 / 隐私脱敏 / 全库刷新 / 空态引导。

import { expect, open, test } from "nsh/test";
import { t, tf } from "../../services/translation";
import {
  seedTrayQuotaPrefs,
  setTrayQuotaEnabled,
  trayQuotaEnabled,
  type TrayQuotaPrefsBridge,
  type TrayQuotaPrefsDto,
} from "../../shell/trayQuotaPrefs";
import {
  QUOTA_POPOVER_BODY_MAX_H,
  QUOTA_POPOVER_MAX_H,
  QUOTA_POPOVER_W,
  buildPopoverAgentGroups,
  computeTrayPopoverOrigin,
  countPopoverAlerts,
} from "./quotaPopoverLogic";
import type { AccountView, AgentLive, AgentRow } from "./types";

const HOST = "./quotaPopover.test.host.tsx";

function makeAccount(over: Partial<AccountView>): AccountView {
  return {
    id: "acct-1",
    agent: "codex",
    displayName: "Dev",
    email: "dev@example.com",
    plan: "pro",
    createdAt: 100,
    lastUsedAt: null,
    requiresReauth: false,
    isActive: false,
    usage: null,
    ...over,
  };
}

test("buildPopoverAgentGroups keeps configured or live agents only and sorts alternates by quota desc", () => {
  const agents: AgentRow[] = [
    { id: "codex", displayName: "Codex" },
    { id: "grok", displayName: "Grok" },
    { id: "cursor", displayName: "Cursor" },
  ];
  const accounts: AccountView[] = [
    makeAccount({
      id: "cx-serving",
      agent: "codex",
      isActive: true,
      usage: {
        fetchedAt: 1000,
        windows: [{ key: "five_hour", label: "5h", remainingPercent: 55, unit: "percent" }],
      },
    }),
    makeAccount({
      id: "cx-alt-low",
      agent: "codex",
      createdAt: 1,
      usage: {
        fetchedAt: 1000,
        windows: [{ key: "five_hour", label: "5h", remainingPercent: 25, unit: "percent" }],
      },
    }),
    makeAccount({
      id: "cx-alt-high",
      agent: "codex",
      createdAt: 2,
      usage: {
        fetchedAt: 1000,
        windows: [{ key: "five_hour", label: "5h", remainingPercent: 95, unit: "percent" }],
      },
    }),
  ];
  const live: AgentLive[] = [
    { agent: "codex", identity: "dev@example.com", accountId: "cx-serving", source: "cli" },
    { agent: "grok", identity: null, accountId: null, source: "cli" },
    { agent: "cursor", identity: "local@cursor.sh", accountId: null, source: "vscdb" },
  ];

  const groups = buildPopoverAgentGroups(agents, accounts, live);
  // Grok 既无入库账号也无本机登录，自动隐藏不产生空行。
  expect(groups.map((g) => g.agentId)).toEqual(["codex", "cursor"]);
  expect(groups[0].primary?.id).toBe("cx-serving");
  expect(groups[0].primaryServing).toBe(true);
  expect(groups[0].alternates.map((a) => a.id)).toEqual(["cx-alt-high", "cx-alt-low"]);
  expect(groups[1].primary).toBeNull();
  expect(groups[1].untrackedIdentity).toBe("local@cursor.sh");
});

test("countPopoverAlerts counts groups needing reauth, error recovery, or <10% quota", () => {
  const agents: AgentRow[] = [
    { id: "codex", displayName: "Codex" },
    { id: "claude-code", displayName: "Claude Code" },
  ];
  const healthyGroups = buildPopoverAgentGroups(
    agents,
    [
      makeAccount({
        id: "cx",
        agent: "codex",
        isActive: true,
        usage: {
          fetchedAt: 100,
          windows: [
            { key: "five_hour", label: "5h", remainingPercent: 88, unit: "percent" },
            { key: "week", label: "Weekly", remainingPercent: 18, unit: "percent" },
          ],
        },
      }),
    ],
    [],
  );
  expect(countPopoverAlerts(healthyGroups)).toBe(0);

  const reauthGroups = buildPopoverAgentGroups(
    agents,
    [
      makeAccount({
        id: "cc",
        agent: "claude-code",
        isActive: true,
        requiresReauth: true,
      }),
    ],
    [],
  );
  expect(countPopoverAlerts(reauthGroups)).toBe(1);
});

test("computeTrayPopoverOrigin centers below macOS menu bar and above Windows taskbar within display bounds", () => {
  const display = { origin: { x: 0, y: 25 }, size: { width: 1440, height: 875 } };
  // macOS 右侧靠近屏幕边缘的图标：自动向左夹紧防溢出右边界。
  const macOrigin = computeTrayPopoverOrigin({
    trayBounds: { origin: { x: 1380, y: 0 }, size: { width: 28, height: 24 } },
    visibleBounds: display,
    panelWidth: QUOTA_POPOVER_W,
    panelHeight: 400,
    platform: "macos",
  });
  expect(macOrigin.x).toBe(1440 - QUOTA_POPOVER_W - 8);
  expect(macOrigin.y).toBe(30);

  // Windows 右下角通知区托盘图标：弹出窗向上展开，底边停在屏幕安全边距 8pt 处（1040 - 8 = 1032）。
  const winDisplay = { origin: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } };
  const winOrigin = computeTrayPopoverOrigin({
    trayBounds: { origin: { x: 1760, y: 1044 }, size: { width: 24, height: 24 } },
    visibleBounds: winDisplay,
    panelWidth: QUOTA_POPOVER_W,
    panelHeight: 400,
    platform: "windows",
  });
  expect(winOrigin.x).toBe(1920 - QUOTA_POPOVER_W - 8);
  expect(winOrigin.y).toBe(1040 - 400 - 8);
  expect(winOrigin.y + 400).toBe(1032);
});

test("trayQuotaPrefs bridge persists enabled switch without side effects", () => {
  let saved: TrayQuotaPrefsDto = { enabled: false };
  const bridge: TrayQuotaPrefsBridge = {
    load: () => ({ ...saved }),
    save: (next) => {
      saved = { ...next };
    },
  };
  seedTrayQuotaPrefs(bridge);
  expect(trayQuotaEnabled()).toBe(false);

  setTrayQuotaEnabled(true, bridge);
  expect(saved).toEqual({ enabled: true });
});

test("popover renders active groups, expands alternates sorted by quota, switches, masks, and refreshes", async () => {
  const app = await open(HOST, { width: 420, height: 640 });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));
  // Grok 无账号且未登录，不应渲染空卡片。
  expect(app.shows("id:quota-group-grok")).toBe(false);
  // Cursor 本机已登录但未入库，展示纳管引导。
  expect(app.shows("id:quota-untracked-cursor")).toBe(true);
  // Claude Code 登录失效，顶部汇总 1 项需关注，且原位渲染重授权按钮。
  expect(app.shows("id:quota-popover-alerts")).toBe(true);
  expect(app.shows("id:quota-reauth-cc-expired")).toBe(true);

  // 点击 Codex 标题栏展开 2 个备选账号，且配额高的 Backup Ultra 排在 Backup Low 之前；
  // 卡片头行在图标右侧直接显示用户名，不显示 Agent 名称。
  await app.press("id:quota-group-head-codex");
  await app.waitUntil(() => app.shows("id:quota-alternates-codex"));
  const textsBefore = app.texts();
  expect(textsBefore).not.toContain("Codex");
  expect(textsBefore).not.toContain("Claude Code");
  expect(textsBefore).not.toContain("Cursor");
  expect(textsBefore.indexOf("Alice Dev")).toBeGreaterThanOrEqual(0);
  expect(textsBefore.indexOf("Backup Ultra")).toBeLessThan(textsBefore.indexOf("Backup Low"));

  // 点击切换至 Backup Ultra：主槽立即切换为新在役账号。
  await app.press("id:quota-switch-cx-full");
  await app.waitUntil(() => app.texts().some((s) => s.includes("switched:cx-full")));
  expect(app.texts()).toContain("Backup Ultra");

  // 开启隐私脱敏：用户名与邮箱同步打码。
  await app.press("id:quota-popover-privacy");
  await app.waitUntil(() => !app.texts().includes("Backup Ultra"));

  // 点击全库刷新：刷新全部 4 张入库卡。
  await app.press("id:quota-popover-refresh-all");
  await app.waitUntil(() => app.texts().some((s) => s.includes("refreshes:4")));

  // 底栏点击可快捷切换自动刷新周期（2m -> 5m -> 15m -> 关闭）。
  await app.press("id:quota-popover-auto-cycle");
  await app.waitUntil(() => app.texts().some((s) => s.includes("自动每 5 分钟")));
  await app.press("id:quota-popover-auto-cycle");
  await app.waitUntil(() => app.texts().some((s) => s.includes("自动每 15 分钟")));
  await app.press("id:quota-popover-auto-cycle");
  await app.waitUntil(() => app.texts().some((s) => s.includes("自动刷新未开启")));

  // 重置为旧时间戳快照后触发展开补刷：自动静默刷新 3 张可用卡（跳过需重授权的 cc-expired），再次触发不再重复请求。
  await app.host.send("popover-harness", { cmd: "reset" });
  await app.waitUntil(() => app.texts().some((s) => s.includes("refreshes:0")));
  await app.host.send("popover-harness", { cmd: "refresh-stale" });
  await app.waitUntil(() => app.texts().some((s) => s.includes("refreshes:3")));
  await app.host.send("popover-harness", { cmd: "refresh-stale" });
  expect(app.texts().some((s) => s.includes("refreshes:3"))).toBe(true);

  // 清空账号后切换至空态引导并可跳转工作台。
  await app.host.send("popover-harness", { cmd: "clear-all" });
  await app.waitUntil(() => app.shows("id:quota-popover-empty"));
  await app.press("id:quota-popover-empty-cta");
  await app.waitUntil(() => app.texts().some((s) => s.includes("workbench:all")));
});

/** 面板高度：测试装置用 ref 量面板外层包裹框，回填到 popover-panel-h 文本。 */
async function panelHeight(
  app: Awaited<ReturnType<typeof open>>,
  tag: string,
): Promise<number> {
  await app.host.send("popover-harness", { cmd: "measure", tag });
  const prefix = `h:${tag}:`;
  let text = "";
  await app.waitUntil(() => {
    text = app.texts().find((line) => line.startsWith(prefix)) ?? "";
    return text.length > 0;
  });
  return Number(text.slice(prefix.length));
}

test("popover body is a scroll box for every group count and never clips the card list", async () => {
  const app = await open(HOST, { width: 420, height: 640 });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));

  // 单组（内容远低于视口上限）：照样挂 <Scroll>，而视口收缩到内容高度，面板不留空白。
  await app.host.send("popover-harness", { cmd: "set-single" });
  await app.waitUntil(
    () => app.shows("id:quota-group-codex") && !app.shows("id:quota-group-antigravity"),
  );
  expect(app.shows("id:quota-popover-scroll")).toBe(true);
  const single = await panelHeight(app, "single");
  expect(single).toBeGreaterThan(0);
  expect(single).toBeLessThan(QUOTA_POPOVER_BODY_MAX_H);

  // 五组长卡片（与真机下拉窗同构）：面板停在上限内，列表交给 <Scroll> 滚动，
  // 不再出现「按组数阈值决定挂不挂滚动容器」时被 overflow:hidden 裁掉且滚轮无处可滚。
  await app.host.send("popover-harness", { cmd: "set-tall" });
  await app.waitUntil(() => app.shows("id:quota-group-zcode"));
  expect(app.shows("id:quota-popover-scroll")).toBe(true);
  const tall = await panelHeight(app, "tall");
  expect(tall).toBe(QUOTA_POPOVER_MAX_H);
  // 滚轮、触控板与拖动共用同一段滚动范围：列表能拖动滚到底，滚轮就有处可滚。
  await app.scroll("id:quota-popover-scroll", 200);
});

test("right-clicking a quota card on desktop opens the native context menu and copies identity", async () => {
  const app = await open(HOST, { width: 420, height: 640, platform: "macos" });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));

  await app.rightClick("id:quota-group-codex");
  const menu = app.menu();
  expect(menu).not.toBeNull();
  const labels = (menu ?? []).map((entry) => entry.label);
  expect(labels).toContain("当前");
  expect(labels).toContain("刷新");
  expect(labels).toContain("复制账号标识");
  expect(labels).toContain("打开账号工作台");

  await app.chooseMenuItem("复制账号标识");
  await app.waitUntil(() => app.clipboard() === "alice@openai.example");
});

test("hidden popover pauses shared-state tracking while preserving keep-alive until reopened", async () => {
  const app = await open(HOST, { width: 420, height: 640 });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));

  await app.host.send("popover-harness", { cmd: "set-active", active: false });
  await app.host.send("popover-harness", { cmd: "set-tall" });
  await new Promise((resolve) => setTimeout(resolve, 40));

  expect(app.shows("id:quota-group-codex")).toBe(true);
  expect(app.shows("id:quota-group-zcode")).toBe(false);

  await app.host.send("popover-harness", { cmd: "set-active", active: true });
  await app.waitUntil(() => app.shows("id:quota-group-zcode"));
});

test("popover displays codex reset card count when present", async () => {
  const app = await open(HOST, { width: 420, height: 640 });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));
  expect(app.shows("id:quota-reset-cards")).toBe(true);
  expect(app.texts()).toContain(t("accountsPage.resetCards"));
  expect(app.texts()).toContain(tf("accountsPage.resetCardsCount", { count: 2 }));
});

test("popover renders dismissible error feedback on switch and refresh failures without exceeding max height", async () => {
  const app = await open(HOST, { width: 420, height: 640 });
  await app.waitUntil(() => app.shows("id:quota-group-codex"));

  // Switch failure inside popover shows error banner and dismisses cleanly.
  await app.press("id:quota-group-head-codex");
  await app.waitUntil(() => app.shows("id:quota-alternates-codex"));
  await app.host.send("popover-harness", { cmd: "fail-activate" });
  await app.press("id:quota-switch-cx-full");
  await app.waitFor("id:quota-popover-error");
  expect(app.texts().some((s) => s.includes("switch denied by client"))).toBe(true);
  await app.press("id:quota-popover-error-close");
  await app.waitUntil(() => !app.shows("id:quota-popover-error"));

  // Refresh failure shows retryable error banner and dismisses cleanly.
  await app.host.send("popover-harness", { cmd: "fail-refresh" });
  await app.press("id:quota-refresh-expanded-cx-active");
  await app.waitFor("id:quota-popover-error");
  expect(app.texts().some((s) => s.includes("quota endpoint 503"))).toBe(true);
  expect(app.shows("id:quota-popover-error-retry")).toBe(true);
  await app.press("id:quota-popover-error-close");
  await app.waitUntil(() => !app.shows("id:quota-popover-error"));

  // Tall list with active error stays within QUOTA_POPOVER_MAX_H and keeps footer visible.
  await app.host.send("popover-harness", { cmd: "set-tall" });
  await app.waitUntil(() => app.shows("id:quota-group-zcode"));
  await app.host.send("popover-harness", { cmd: "set-error" });
  await app.waitFor("id:quota-popover-error");
  expect(app.shows("id:quota-popover-footer")).toBe(true);
  const tallWithError = await panelHeight(app, "tall-err");
  expect(tallWithError).toBeLessThanOrEqual(QUOTA_POPOVER_MAX_H);
});
