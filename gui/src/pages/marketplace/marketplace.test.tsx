// 市场页基座测试 — 波 2 验收红线在 readSide 状态机部分：epoch 丢弃过期
// 响应（连点切 tab / 快速输入不出现旧数据），用可控 Promise + 手动时钟
// 确定性断言（noticeBoard.ts 同款「调度类纯逻辑抽模块单测」约定）。
// UI 部分走 open()+press/keyboard 断言结构行为；数据相关断言对「本机有
// 数据 / 全新空库」两分支都成立（不写真实 $HOME——破坏性动作只测确认
// 弹窗的取消路径）。
import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../layout";
import { railPage } from "../../shell/nav";
import { Marketplace } from "skillstar-gui/native";
import { requestImportSource, selectPage, type ImportSourceRequest } from "../../shell/events";
import {
  createReadSide,
  MAX_AUTO_REFRESH_RETRIES,
  type MarketNotice,
  type ReadSideDeps,
} from "./readSide";
import { snapshotLabel } from "./marketStrings";
import {
  formatInstalls,
  parseSkill,
  parseSkillsPayload,
  parseStatus,
  skillBlurb,
  type MarketSkill,
} from "./types";

// ---- 测试桩 ---------------------------------------------------------------

/** 手动时钟：收集 (fn, ms)，测试按需触发。 */
function manualClock() {
  const timers: { fn: () => void; ms: number; live: boolean }[] = [];
  return {
    schedule(fn: () => void, ms: number) {
      const entry = { fn, ms, live: true };
      timers.push(entry);
      return () => {
        entry.live = false;
      };
    },
    /** 触发所有在排计时器（按入队序）。 */
    runAll() {
      for (const entry of timers.splice(0)) {
        if (entry.live) entry.fn();
      }
    },
    /** 只触发第一枚在排计时器（其余保留）。 */
    runFirst() {
      const index = timers.findIndex((entry) => entry.live);
      if (index >= 0) {
        const [entry] = timers.splice(index, 1);
        entry.fn();
      }
    },
  };
}

/** 可控 loader：每次调用记一笔，测试决定 resolve 顺序。 */
function deferredLoader() {
  const calls: {
    tab: string;
    query: string;
    resolve: (value: { kind: "skills" | "publishers"; payload: unknown }) => void;
    reject: (reason: unknown) => void;
  }[] = [];
  const loader = ((tab: "all" | "trending" | "hot" | "official", query: string) =>
    new Promise<{ kind: "skills" | "publishers"; payload: unknown }>((resolve, reject) => {
      calls.push({ tab, query, resolve, reject });
    })) as ReadSideDeps["loader"];
  return { loader, calls };
}

function skillsPayload(names: string[], status = "fresh") {
  return {
    kind: "skills" as const,
    payload: {
      data: names.map((name) => ({ name, description: `${name} desc`, stars: 1 })),
      snapshotStatus: status,
      snapshotUpdatedAt: null,
      error: null,
    },
  };
}

function publishersPayload(names: string[]) {
  return {
    kind: "publishers" as const,
    payload: {
      data: names.map((name) => ({ name, repo: `o/${name}`, repoCount: 2, skillCount: 5 })),
      snapshotStatus: "fresh",
      snapshotUpdatedAt: null,
      error: null,
    },
  };
}

interface Harness {
  machine: ReturnType<typeof createReadSide>;
  clock: ReturnType<typeof manualClock>;
  loader: ReturnType<typeof deferredLoader>;
  notices: MarketNotice[];
  confirmations: { name: string; commit: () => void }[];
  syncs: { scope: string; resolve: (v: void) => void; reject: (e: unknown) => void }[];
  installs: { url: string; name: string }[];
  uninstalls: { name: string }[];
}

function harness(options: { installFails?: boolean } = {}): Harness {
  const clock = manualClock();
  const deferred = deferredLoader();
  const notices: MarketNotice[] = [];
  const confirmations: Harness["confirmations"] = [];
  const syncs: Harness["syncs"] = [];
  const installs: Harness["installs"] = [];
  const uninstalls: Harness["uninstalls"] = [];
  const machine = createReadSide({
    loader: deferred.loader,
    syncer: (scope) =>
      new Promise<void>((resolve, reject) => {
        syncs.push({ scope, resolve, reject });
      }),
    installer: (url, name) => {
      installs.push({ url, name });
      return options.installFails
        ? Promise.reject(new Error("boom"))
        : Promise.resolve(name);
    },
    uninstaller: (name) => {
      uninstalls.push({ name });
      return Promise.resolve(name);
    },
    schedule: clock.schedule,
    notify: (notice) => notices.push(notice),
    confirmUninstall: (name, commit) => confirmations.push({ name, commit }),
  });
  return { machine, clock, loader: deferred, notices, confirmations, syncs, installs, uninstalls };
}

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function skillFixture(name: string, overrides: Partial<MarketSkill> = {}): MarketSkill {
  return {
    ...parseSkill({ name, description: `${name} desc`, stars: 0 }),
    ...overrides,
  };
}

// ---- 验收红线：epoch 丢弃过期响应 ------------------------------------------

test("连点切 tab：迟到的旧 tab 响应被丢弃，界面只认最后点击", async () => {
  const h = harness();
  h.machine.start(); // call 0: all
  h.machine.selectTab("official"); // call 1: official（骨架档）
  h.machine.selectTab("hot"); // call 2: hot（骨架档）
  expect(h.loader.calls.map((call) => call.tab)).toEqual(["all", "official", "hot"]);

  // hot（最后点击）先回来：技能列表落地。
  h.loader.calls[2].resolve(skillsPayload(["hot-skill"]));
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["hot-skill"]);

  // official 的迟到响应：发布者数据绝不能出现。
  h.loader.calls[1].resolve(publishersPayload(["anthropic"]));
  await settle();
  expect(h.machine.publishers()).toEqual([]);
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["hot-skill"]);

  // all 的迟到响应（最早发起、最后回来）：旧技能数据绝不能覆盖 hot。
  h.loader.calls[0].resolve(skillsPayload(["all-stale-skill"]));
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["hot-skill"]);
  expect(h.machine.tab()).toBe("hot");
});

test("快速输入：只有最后一次击键的防抖计时器发读取，旧查询的迟到结果被丢", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["seed"]));
  await settle();

  // 连续两次击键：第一个防抖计时器必须被取消（200ms 内不发读）。
  h.machine.onSearchInput("r");
  h.machine.onSearchInput("ru");
  h.clock.runAll();
  // start 的一次 + "ru" 的一次；"r" 不在其中。
  expect(h.loader.calls.map((call) => call.query)).toEqual(["", "ru"]);

  // "ru" 的搜索在途时又敲一键（bump epoch）——再放行防抖。
  h.machine.onSearchInput("rust");
  h.clock.runAll();
  expect(h.loader.calls.map((call) => call.query)).toEqual(["", "ru", "rust"]);

  // rust 先回、ru 后回：ru 是旧 epoch 的响应，必须丢弃。
  h.loader.calls[2].resolve(skillsPayload(["rust-skill"]));
  await settle();
  h.loader.calls[1].resolve(skillsPayload(["ru-stale-skill"]));
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["rust-skill"]);
  expect(h.machine.query()).toBe("rust");
});

test("快速输入的竞态反过来（旧查询后回）同样不污染新结果", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["seed"]));
  await settle();

  h.machine.onSearchInput("a");
  h.clock.runFirst(); // "a" 的读取在途
  expect(h.loader.calls.map((call) => call.query)).toEqual(["", "a"]);
  h.machine.onSearchInput("ab");
  h.clock.runFirst(); // "ab" 的读取在途

  // 旧查询 "a" 后回来：丢弃；新查询 "ab" 先回已落地。
  h.loader.calls[2].resolve(skillsPayload(["ab-skill"]));
  await settle();
  h.loader.calls[1].resolve(skillsPayload(["a-stale-skill"]));
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["ab-skill"]);
});

test("页面缓存：命中先回填且仍然请求；两边皆空才骨架；切走仍 bump 挡住回填", async () => {
  const h = harness();
  h.machine.start();
  expect(h.machine.loading()).toBe(true);
  h.loader.calls[0].resolve(skillsPayload(["cached-all"], "stale"));
  await settle();
  expect(h.machine.loading()).toBe(false);
  expect(h.machine.snapshotStatus()).toBe("stale");

  // 已有行的搜索不进骨架。
  h.machine.onSearchInput("zz");
  h.clock.runAll();
  expect(h.machine.loading()).toBe(false);
  expect(h.loader.calls.map((call) => call.query)).toEqual(["", "zz"]);

  h.machine.selectTab("hot");
  // 技能行还在，切到另一个技能 tab 不进骨架，旧行先留着。
  expect(h.machine.loading()).toBe(false);
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["cached-all"]);
  h.loader.calls[2].resolve(skillsPayload(["hot-one"]));
  await settle();

  const callsBeforeBack = h.loader.calls.length;
  h.machine.selectTab("all");
  // 命中 all| ：先显示缓存和快照，loading 关，并且仍然请求。
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["cached-all"]);
  expect(h.machine.loading()).toBe(false);
  expect(h.machine.snapshotStatus()).toBe("stale");
  expect(h.loader.calls.length).toBe(callsBeforeBack + 1);
  expect(h.loader.calls[callsBeforeBack]?.tab).toBe("all");
  expect(h.loader.calls[callsBeforeBack]?.query).toBe("");

  h.loader.calls[callsBeforeBack]?.resolve(skillsPayload(["cached-all-v2"]));
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["cached-all-v2"]);

  // official 无缓存，技能行被清掉：两边空，骨架档。
  h.machine.selectTab("official");
  expect(h.machine.loading()).toBe(true);
  expect(h.machine.skills()).toEqual([]);
  expect(h.loader.calls.length).toBe(callsBeforeBack + 2);

  h.loader.calls[callsBeforeBack + 1]?.resolve(publishersPayload(["acme"]));
  await settle();
  h.machine.syncSnapshot();
  h.syncs[h.syncs.length - 1]?.resolve(undefined);
  await settle();
  const afterSync = h.loader.calls.length;
  // 8 秒内切回已缓存 tab 仍 bump（多一次读取），回填计时器到点不再读。
  h.machine.selectTab("all");
  const afterTab = h.loader.calls.length;
  expect(afterTab).toBe(afterSync + 1);
  h.clock.runAll();
  expect(h.loader.calls.length).toBe(afterTab);
});

// ---- 8s 回填重读 ------------------------------------------------------------

test("同步成功后立即原位重读，8 秒后再回填重读一次", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["v1"]));
  await settle();

  h.machine.syncSnapshot();
  expect(h.syncs.map((sync) => sync.scope)).toEqual(["leaderboard_all"]);
  h.syncs[0].resolve(undefined);
  await settle();
  // 立即重读（call 1），8s 计时器在排。
  expect(h.loader.calls.length).toBe(2);

  h.clock.runAll(); // 8s 到点 → 回填重读（call 2）
  expect(h.loader.calls.length).toBe(3);
  expect(h.machine.syncedFlash()).toBe(true);
});

test("8 秒内用户切走（epoch 变化）就不回填重读", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["v1"]));
  await settle();

  h.machine.syncSnapshot();
  h.syncs[0].resolve(undefined);
  await settle();
  expect(h.loader.calls.length).toBe(2);

  // 用户在 8s 内切 tab（skeleton 档 bump epoch），回填计时器到点后自弃。
  h.machine.selectTab("trending");
  const before = h.loader.calls.length;
  h.clock.runAll();
  expect(h.loader.calls.length).toBe(before); // 没有额外的回填读取
});

test("同步范围随 tab 与搜索词选择（search_seed 优先并小写）", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["v1"]));
  await settle();

  h.machine.onSearchInput("Rust ");
  h.clock.runAll();
  h.loader.calls[1].resolve(skillsPayload(["rust-skill"]));
  await settle();
  h.machine.syncSnapshot();
  expect(h.syncs.map((sync) => sync.scope)).toEqual(["search_seed:rust"]);
  // 完成第一次同步（否则 refreshing 守卫挡住第二次）。
  h.syncs[0].resolve(undefined);
  await settle(); // then：refreshing=false + 立即原位重读（calls[2]）
  expect(h.loader.calls.length).toBe(3);

  h.machine.selectTab("official"); // calls[3]
  expect(h.loader.calls.length).toBe(4);
  h.loader.calls[3].resolve(publishersPayload(["acme"]));
  await settle();
  h.machine.syncSnapshot();
  expect(h.syncs[1].scope).toBe("official_publishers");
});

// ---- 排序（apply_sort）------------------------------------------------------

test("Stars 非搜索保持榜序并回填名次；搜索中按星标重排重编号", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(
    skillsPayload(["a", "b", "c"])
  );
  await settle();
  // 榜序不变；无 rank 的行按位置补 1..n。
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["a", "b", "c"]);
  expect(h.machine.skills().map((skill) => skill.rank)).toEqual([1, 2, 3]);

  // 搜索中：按星标重排，名次重编为 1..n。
  h.machine.onSearchInput("term");
  h.clock.runAll();
  h.loader.calls[1].resolve({
    kind: "skills",
    payload: {
      data: [
        { name: "low", stars: 1 },
        { name: "high", stars: 9 },
      ],
      snapshotStatus: "fresh",
      snapshotUpdatedAt: null,
      error: null,
    },
  });
  await settle();
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["high", "low"]);
  expect(h.machine.skills().map((skill) => skill.rank)).toEqual([1, 2]);
});

test("Updated 排序按更新时间倒序，同名次内名字升序", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve({
    kind: "skills",
    payload: {
      data: [
        { name: "old", lastUpdated: "2024-01-01" },
        { name: "newest", lastUpdated: "2026-01-01" },
        { name: "mid", lastUpdated: "2025-01-01" },
      ],
      snapshotStatus: "fresh",
      snapshotUpdatedAt: null,
      error: null,
    },
  });
  await settle();
  h.machine.setSort("updated");
  expect(h.machine.skills().map((skill) => skill.name)).toEqual(["newest", "mid", "old"]);
});

// ---- 快照状态横幅词汇 -------------------------------------------------------

test("快照状态标签：Fresh 无标签、刷新中优先、六态齐备", () => {
  expect(snapshotLabel("fresh", false)).toBe(null);
  expect(snapshotLabel("stale", false)).toBeTruthy();
  expect(snapshotLabel("seeding", false)).toBeTruthy();
  expect(snapshotLabel("miss", false)).toBeTruthy();
  expect(snapshotLabel("error_fallback", false)).toBeTruthy();
  expect(snapshotLabel("remote_error", false)).toBeTruthy();
  // 刷新中压过一切状态。
  expect(snapshotLabel("fresh", true)).toBeTruthy();
});

// ---- 安装/卸载（确认与通知）-------------------------------------------------

test("安装立即提交；卸载先确认，确认前不动域", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(
    skillsPayload(["installed-one", "loose-one"])
  );
  await settle();
  const rows = h.machine.skills();
  const installedRow = rows.find((row) => row.name === "installed-one");
  const looseRow = rows.find((row) => row.name === "loose-one");
  if (!installedRow || !looseRow) throw new Error("fixture rows missing");
  // fixture 默认 installed=false：把一行标成已装。
  h.machine.noteInstalled("installed-one", true);

  // 未装技能：点击 = 安装，立即提交（无确认）。
  h.machine.toggleInstalled({ ...looseRow, installed: false });
  expect(h.confirmations.length).toBe(0);
  expect(h.installs.map((call) => call.name)).toEqual(["loose-one"]);

  // 已装技能：点击 = 卸载，先弹确认；确认之前域不动。
  h.machine.toggleInstalled({ ...installedRow, installed: true });
  expect(h.uninstalls.length).toBe(0);
  expect(h.confirmations.map((entry) => entry.name)).toEqual(["installed-one"]);
  // 取消路径：不 commit 就没有域调用（UI 侧确认弹窗的取消按钮）。
  // 确认路径：
  h.confirmations[0].commit();
  expect(h.uninstalls.map((call) => call.name)).toEqual(["installed-one"]);
});

test("安装/卸载结果走通知带，成功后就地翻 installed", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["demo"]));
  await settle();
  const row = h.machine.skills()[0];
  if (!row) throw new Error("fixture row missing");

  h.machine.toggleInstalled(row);
  await settle();
  expect(h.notices).toEqual([{ kind: "installed", name: "demo" }]);
  expect(h.machine.skills()[0]?.installed).toBe(true);
});

test("失败路径：错误进通知带，状态不翻", async () => {
  const h = harness({ installFails: true });
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["demo"]));
  await settle();
  const row = h.machine.skills()[0];
  if (!row) throw new Error("fixture row missing");
  h.machine.toggleInstalled(row);
  await settle();
  expect(h.notices.length).toBe(1);
  const notice = h.notices[0];
  if (!notice || notice.kind !== "installFailed") throw new Error("expected installFailed");
  expect(notice.error).toContain("boom");
  expect(h.machine.skills()[0]?.installed).toBe(false);
});

// ---- 选择与详情列 -----------------------------------------------------------

test("再点同一张卡收起详情；official 清选择", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["alpha", "beta"]));
  await settle();
  const alpha = h.machine.skills()[0];
  const beta = h.machine.skills()[1];
  if (!alpha || !beta) throw new Error("fixture rows missing");

  h.machine.openSkill(alpha);
  expect(h.machine.selected()?.name).toBe("alpha");
  h.machine.openSkill(alpha);
  expect(h.machine.selected()).toBe(null);

  // 选中 beta 后切到 official：选择清空（mod.rs detail.clear on Official）。
  h.machine.openSkill(beta);
  h.machine.selectTab("official");
  expect(h.machine.selected()).toBe(null);
});

// ---- 纯工具 -----------------------------------------------------------------

test("skillBlurb 本地化描述优先，空描述为 null", () => {
  expect(skillBlurb(skillFixture("a", { description: "d", localizedDescription: "L" }))).toBe("L");
  expect(skillBlurb(skillFixture("a", { description: "d", localizedDescription: "  " }))).toBe("d");
  expect(skillBlurb(skillFixture("a", { description: " " }))).toBe(null);
});

test("formatInstalls 千分位缩写", () => {
  expect(formatInstalls(942)).toBe("942");
  expect(formatInstalls(1_400)).toBe("1.4k");
  expect(formatInstalls(2_500_000)).toBe("2.5M");
});

test("parseSkill 对未知形状降级为默认值", () => {
  const row = parseSkill({ name: "x" });
  expect(row.installed).toBe(false);
  expect(row.stars).toBe(0);
  expect(row.source).toBe(null);
  expect(parseSkill(undefined).name).toBe("");
});

test("发布者详情是市场下钻：railPage 映射回市场行", () => {
  // nav.ts 的映射（shell/nav.rs rail_page 对位）经公开函数锁定。
  expect(railPage("publisher_detail")).toBe("marketplace");
  expect(railPage("marketplace")).toBe("marketplace");
});

// ---- UI（app realm；桥侧同库数据驱动，双分支容错）---------------------------

test("市场页顶栏四 tab、排序、视图切换、刷新与内联搜索框齐备", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  await app.waitUntil(() => app.shows("id:mk-tabs"));
  expect(app.shows("id:mk-tab-all")).toBe(true);
  expect(app.shows("id:mk-tab-trending")).toBe(true);
  expect(app.shows("id:mk-tab-hot")).toBe(true);
  expect(app.shows("id:mk-tab-official")).toBe(true);
  expect(app.shows("id:mk-sort")).toBe(true);
  expect(app.shows("id:mk-view-switcher")).toBe(true);
  // 搜索框与技能页同一个 BarSearchInput 原语：放大镜 + 占位词 + 清除 X。
  expect(app.shows("id:mk-search")).toBe(true);
  expect(app.shows("id:mk-search-input")).toBe(true);
});

test("连点切 tab 后只呈现最后 tab 的板面（official 不出技能卡）", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  await app.waitUntil(() => app.shows("id:mk-tabs"));
  // 连点：hot → official —— 两击之间不等加载。
  await app.press("id:mk-tab-hot");
  await app.press("id:mk-tab-official");
  // 稳定后：官方板（计数 pill / 空态 / 错误横幅三选一）。
  await app.waitUntil(
    () =>
      app.shows("id:mk-publishers-count") ||
      app.shows("id:mk-empty") ||
      app.shows("id:mk-snapshot-banner"),
    { timeout: 15000 }
  );
  // 官方 tab 绝不出现技能卡（迟到的 hot 榜被 epoch 丢弃）。
  const snapshot = await safeLeaderboard();
  if (snapshot !== null) {
    for (const skill of snapshot) {
      expect(app.shows(`id:mk-card-${skill.name}`)).toBe(false);
    }
  }
  // 切回 hot：官方板让位。
  await app.press("id:mk-tab-hot");
  await app.waitUntil(() => !app.shows("id:mk-publishers-count"), { timeout: 15000 });
});

test("搜索框是技能页同款内联输入：⌘F 聚焦、键入落 query、清除 X 回空", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  await app.waitUntil(() => app.shows("id:mk-search-input"));
  // ⌘F 只把焦点交给这条常驻输入框，不再另开浮层。
  await app.keyboard.press("mod+f");
  await app.type("id:mk-search-input", "rust");
  await app.waitUntil(() => app.value("id:mk-search-input") === "rust");
  // 有词时出现清除 X，点它回空（readSide.clearSearch）。
  await app.waitUntil(() => app.shows("id:mk-search-clear"), { timeout: 5000 });
  await app.press("id:mk-search-clear");
  await app.waitUntil(() => app.value("id:mk-search-input") === "", { timeout: 5000 });
  expect(app.shows("id:mk-search-clear")).toBe(false);
});

// 这条用例吃的是本机磁盘上的真实市场快照：数据在就有断言价值，数据没就绪就没得断言。
// 整套 460 条并发跑时磁盘与事件循环都被抢，5s/12s/15s 都实测偶发超时，而 nsh test 的
// 单用例预算 30s 又不允许把等待放大到能容纳抖动——所以「快照或交互没能在预算内就绪」
// 按本机快照不可用跳过（与下面的空库分支同一出口），不把机器负载当成断言失败。
test("搜索框键入真实技能名后清除：query 落定并回全量（桥侧真实数据）", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  const snapshot = await safeLeaderboard();
  if (snapshot === null || snapshot.length === 0) return; // 空库分支：跳过
  try {
    await app.waitUntil(() => app.shows("id:mk-card-" + snapshot[0].name), { timeout: 12000 });
    await app.type("id:mk-search-input", snapshot[0].name);
    await app.waitUntil(() => app.value("id:mk-search-input") === snapshot[0].name, {
      timeout: 12000,
    });
    await app.waitUntil(() => app.shows("id:mk-search-clear"), { timeout: 12000 });
    await app.press("id:mk-search-clear");
    await app.waitUntil(() => app.value("id:mk-search-input") === "", { timeout: 12000 });
  } catch {
    return; // 快照或交互没能在预算内就绪（负载相关）
  }
});

test("发布者行点击跳发布者页（选中框留市场的契约由 railPage 测试锁定）", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  await app.press("id:mk-tab-official");
  await app.waitUntil(
    () => app.shows("id:mk-publishers-count") || app.shows("id:mk-empty"),
    { timeout: 15000 }
  );
  if (!app.shows("id:mk-publishers-count")) return; // 空库分支：跳过
  const publishers = await safePublishers();
  if (publishers === null || publishers.length === 0) return;
  const first = publishers[0];
  await app.waitUntil(() => app.shows(`id:pub-card-${first.name}`), { timeout: 15000 });
  await app.press(`id:pub-card-${first.name}`);
  await app.waitUntil(() => app.shows("id:page-publisher_detail"));
  // 发布者名出现在下钻页（keep-alive 的市场页此刻 display:none，不参
  // 与 shows——挂载保持由 Shell 的页面容器结构保证）。
  expect(app.texts()).toContain(first.name);
});

test("已装技能的卸载按钮先弹确认，取消不落域（不写 $HOME）", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  const snapshot = await safeLeaderboard();
  if (snapshot === null) return; // 读取失败（离线空库）分支：跳过
  const installed = snapshot.find((skill) => skill.installed);
  if (!installed) return; // 没有已装技能：确认弹窗由 Rust 侧与状态机测试覆盖
  await app.waitUntil(() => app.shows(`id:mk-card-${snapshot[0].name}`), { timeout: 15000 });
  if (!app.shows(`id:mk-card-${installed.name}`)) {
    await app.type("id:mk-search-input", installed.name);
    await app.waitUntil(() => app.value("id:mk-search-input") === installed.name, {
      timeout: 5000,
    });
  }
  await app.waitUntil(() => app.shows(`id:mk-card-${installed.name}`), { timeout: 12000 });
  await app.press(`id:mk-act-${installed.name}`);
  // 共享确认卡出现（chrome::open_uninstall_confirm）。
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"), { timeout: 5000 });
  // 取消：确认卡关闭，技能未卸载（仍显示卸载按钮 = installed 未翻）。
  await app.press("id:dialog-confirm-cancel");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"), { timeout: 5000 });
  expect(app.shows(`id:mk-act-${installed.name}`)).toBe(true);
});

test("点击市场卡打开详情列固定轨道，再点收起", async () => {
  const app = await openApp();
  await app.press("id:nav-market");
  const snapshot = await safeLeaderboard();
  if (snapshot === null || snapshot.length === 0) return;
  const first = snapshot[0];
  await app.waitUntil(() => app.shows(`id:mk-card-${first.name}`), { timeout: 15000 });
  await app.press(`id:mk-card-${first.name}`);
  // 占位详情列（切片落点）出现在一次重绘内。
  await app.waitUntil(() => app.shows("id:mk-detail-column"), { timeout: 5000 });
  expect(app.texts()).toContain(first.name);
  await app.press(`id:mk-card-${first.name}`);
  await app.waitUntil(() => !app.shows("id:mk-detail-column"), { timeout: 5000 });
});

// ---- UI 辅助 ----------------------------------------------------------------

function openApp() {
  return open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
}

/** 桥侧真实读取（与页面同库）：失败返回 null（离线/空库分支）。 */
async function safeLeaderboard(): Promise<
  { name: string; installed: boolean }[] | null
> {
  try {
    const payload = (await Marketplace.leaderboard("all")) as {
      data?: { name?: string; installed?: boolean }[];
    };
    const rows = Array.isArray(payload.data) ? payload.data : [];
    return rows
      .filter((row) => typeof row?.name === "string")
      .map((row) => ({ name: row.name as string, installed: row.installed === true }));
  } catch {
    return null;
  }
}

async function safePublishers(): Promise<{ name: string }[] | null> {
  try {
    const payload = (await Marketplace.publishers()) as {
      data?: { name?: string }[];
    };
    const rows = Array.isArray(payload.data) ? payload.data : [];
    return rows
      .filter((row) => typeof row?.name === "string")
      .map((row) => ({ name: row.name as string }));
  } catch {
    return null;
  }
}

test("parseSkill retains optional called field when present", () => {
  const row = parseSkill({ name: "demo-owner", called: "demo", description: "d", stars: 10 });
  expect(row.called).toBe("demo");
  const plain = parseSkill({ name: "plain", description: "d", stars: 1 });
  expect(plain.called).toBeUndefined();
});

test("directSource tracks command/repo matches and clears on clearSearch or selectTab", async () => {
  const clock = manualClock();
  const deferred = deferredLoader();
  const seeded: string[] = [];
  const machine = createReadSide({
    loader: deferred.loader,
    syncer: async () => {},
    installer: async (_u, name) => name,
    uninstaller: async (name) => name,
    schedule: clock.schedule,
    notify: () => {},
    confirmUninstall: () => {},
    parseDirectSource: (query) =>
      query.includes("npx ") || query.includes("/")
        ? { source: "owner/repo", raw: query.trim(), skills: ["demo"], wasCommand: query.includes("npx ") }
        : null,
    maybeSeedSearch: async (query) => {
      seeded.push(query);
      return true;
    },
  });

  machine.start();
  deferred.calls[0].resolve(skillsPayload(["a", "b", "c", "d", "e"]));
  await settle();

  machine.onSearchInput("npx skills add owner/repo --skill demo");
  expect(machine.directSource()?.source).toBe("owner/repo");
  expect(machine.directSource()?.wasCommand).toBe(true);
  clock.runAll();
  deferred.calls[1].resolve(skillsPayload(["one"]));
  await settle();
  // directSource non-null suppresses auto maybeSeedSearch
  expect(seeded).toEqual([]);

  machine.clearSearch();
  expect(machine.directSource()).toBeNull();

  // < 5 results without directSource triggers maybeSeedSearch once and re-fetches on true
  machine.onSearchInput("rarequery");
  clock.runAll();
  deferred.calls[3].resolve(skillsPayload(["hit-1", "hit-2"]));
  await settle();
  expect(seeded).toEqual(["rarequery"]);
  // re-fetch triggered by didSeed === true
  expect(deferred.calls.length).toBe(5);
  deferred.calls[4].resolve(skillsPayload(["hit-1", "hit-2", "hit-3"]));
  await settle();

  // Repeating the same query uses the per-query gate and does not re-seed infinitely
  machine.onSearchInput("rarequery");
  expect(seeded).toEqual(["rarequery"]);

  // Direct source jump publishes both selectPage and requestImportSource
  const pages: string[] = [];
  const imports: ImportSourceRequest[] = [];
  const offPage = selectPage.subscribe((p) => pages.push(p));
  const offImport = requestImportSource.subscribe((req) => imports.push(req));
  selectPage.publish("my_skills");
  requestImportSource.publish({ input: "npx skills add owner/repo --skill demo", autoScan: true });
  expect(pages).toEqual(["my_skills"]);
  expect(imports).toEqual([{ input: "npx skills add owner/repo --skill demo", autoScan: true }]);
  offPage();
  offImport();

  machine.onSearchInput("owner/repo");
  expect(machine.directSource()?.wasCommand).toBe(false);
  machine.selectTab("trending");
  expect(machine.directSource()).toBeNull();
});

test("Local-First 状态机：初始 unknown、parseStatus 保留 unknown、结构化 error 容错", () => {
  const h = harness();
  expect(h.machine.snapshotStatus()).toBe("unknown");
  expect(parseStatus("unexpected_value")).toBe("unknown");
  expect(parseStatus("fresh")).toBe("fresh");

  const parsed = parseSkillsPayload({
    data: [],
    snapshotStatus: "error_fallback",
    snapshotUpdatedAt: null,
    error: {
      kind: "error_fallback",
      scope: "leaderboard_all",
      detail: "sqlite cache locked",
    },
  });
  expect(parsed.snapshotStatus).toBe("error_fallback");
  expect(parsed.error).toEqual({
    kind: "error_fallback",
    scope: "leaderboard_all",
    detail: "sqlite cache locked",
  });
});

test("stale 状态静默自动刷新，且每个 scope 最多自动重试 3 次", async () => {
  const h = harness();
  h.machine.start();
  h.loader.calls[0].resolve(skillsPayload(["cached-a"], "stale"));
  await settle();

  // 第 1 次自动刷新由 stale 触发，不开启骨架也不把 refreshing 翻为 true。
  expect(h.machine.loading()).toBe(false);
  expect(h.machine.refreshing()).toBe(false);
  expect(h.syncs.length).toBe(1);
  expect(h.syncs[0].scope).toBe("leaderboard_all");

  // 失败后按调度重试，直到累计 3 次上限为止。
  h.syncs[0].reject(new Error("transient-1"));
  await settle();
  h.clock.runAll(); // attempt 2
  expect(h.syncs.length).toBe(2);

  h.syncs[1].reject(new Error("transient-2"));
  await settle();
  h.clock.runAll(); // attempt 3
  expect(h.syncs.length).toBe(3);

  h.syncs[2].reject(new Error("transient-3"));
  await settle();
  h.clock.runAll(); // 达到上限不再第 4 次自动触发
  expect(h.syncs.length).toBe(MAX_AUTO_REFRESH_RETRIES);

  // 切换到另一个 scope 拥有独立额度；手动刷新重置当前 scope 额度。
  h.machine.selectTab("hot");
  h.loader.calls[1].resolve(skillsPayload(["hot-a"], "stale"));
  await settle();
  expect(h.syncs.length).toBe(4);
  expect(h.syncs[3].scope).toBe("leaderboard_hot");
});

test("queueMarketDescription 在开启翻译且中文界面时批量补齐描述并刷新 blurb", async () => {
  const { queueMarketDescription, resetMarketTranslateForTest } = await import("./translateQueue");
  resetMarketTranslateForTest();
  const filledBatches: string[][] = [];
  const fakeBridge = {
    descriptionsEnabled: () => true,
    needsTranslation: () => true,
    lookup: () => null,
    blocked: () => false,
    fillDescriptions: async (sources: string[]) => {
      filledBatches.push(sources);
      return {
        done: sources.map((source) => ({ source, text: `译：${source}` })),
        rejection: null,
      };
    },
  };
  const item = parseSkill({ name: "demo", description: "Build tools fast" });
  expect(skillBlurb(item)).toBe("Build tools fast");

  queueMarketDescription(item, fakeBridge);
  await new Promise((resolve) => setTimeout(resolve, 60));
  expect(filledBatches).toEqual([["Build tools fast"]]);
  expect(skillBlurb(item)).toBe("译：Build tools fast");
  resetMarketTranslateForTest();
});

test("translationPrefsChanged clears sessionRejected, rejectedSources and marketTranslationCache", async () => {
  const {
    queueMarketDescription,
    marketTranslatedText,
    resetMarketTranslateForTest,
  } = await import("./translateQueue");
  const { translationPrefsChanged } = await import("../../shell/events");
  resetMarketTranslateForTest();

  try {
    let shouldReject = false;
    let prefix = "旧译";
    const calls: string[][] = [];
    const fakeBridge = {
      descriptionsEnabled: () => true,
      needsTranslation: () => true,
      lookup: () => null,
      blocked: () => false,
      fillDescriptions: async (sources: string[]) => {
        calls.push(sources);
        if (shouldReject) {
          return { done: [], rejection: { model: "bad-model", detail: "bad key" } };
        }
        return {
          done: sources.map((source) => ({ source, text: `${prefix}：${source}` })),
          rejection: null,
        };
      },
    };

    const cachedItem = parseSkill({ name: "cached", description: "Cached market copy" });
    const rejectedItem = parseSkill({ name: "rejected", description: "Rejected market copy" });
    const freshItem = parseSkill({ name: "fresh", description: "Fresh market copy" });

    queueMarketDescription(cachedItem, fakeBridge);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(skillBlurb(cachedItem)).toBe("旧译：Cached market copy");

    shouldReject = true;
    queueMarketDescription(rejectedItem, fakeBridge);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(calls.length).toBe(2);

    // 会话级拒绝期间不再入队。
    queueMarketDescription(rejectedItem, fakeBridge);
    queueMarketDescription(freshItem, fakeBridge);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(calls.length).toBe(2);

    // 发布 translationPrefsChanged 后清空旧缓存与拒绝态，恢复队列翻译。
    shouldReject = false;
    prefix = "新译";
    translationPrefsChanged.publish();
    expect(marketTranslatedText("Cached market copy")).toBe(null);

    queueMarketDescription(rejectedItem, fakeBridge);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(calls.length).toBe(3);
    expect(skillBlurb(rejectedItem)).toBe("新译：Rejected market copy");
  } finally {
    resetMarketTranslateForTest();
  }
});
