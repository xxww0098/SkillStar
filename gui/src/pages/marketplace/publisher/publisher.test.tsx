// 发布者页切片测试 — marketplace/publisher/（mod.rs + board.rs + card.rs +
// hero.rs）对位。状态机部分（readSide.ts）覆盖验收红线：epoch 丢弃迟到响
// 应（快速换发布者/换仓库/返回不出现旧数据）、go_up 两级返回、同步 scope
// 文法、描述回填 watch（水位推进才重读、只填空白行）、安装/卸载确认与通
// 知——用可控 Promise + 手动时钟确定性断言（marketplace.test.tsx 同款约
// 定）。UI 部分走 open()+press 断言结构行为（选中框留市场项是验收红线，
// railPage 的映射已由 marketplace.test.tsx 锁死，这里锁 UI 同屏）。
import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../../layout";
import { railPage } from "../../../shell/nav";
import { Marketplace, MarketplacePublisher } from "skillstar-gui/native";
import { parseSkill, type MarketSkill } from "../types";
import type { MarketNotice } from "../readSide";
import {
  DESCRIPTION_POLL_MS,
  createPublisherSide,
  fillEmptyDescriptions,
  heroCounts,
  type PublisherSide,
  type PublisherSideDeps,
} from "./readSide";

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
    runAll() {
      for (const entry of timers.splice(0)) {
        if (entry.live) entry.fn();
      }
    },
    runFirst() {
      const index = timers.findIndex((entry) => entry.live);
      if (index >= 0) {
        const [entry] = timers.splice(index, 1);
        entry.fn();
      }
    },
    pendingCount() {
      return timers.filter((entry) => entry.live).length;
    },
  };
}

/** 可控异步读取：每次调用记一笔，测试决定 resolve 顺序。 */
function deferredCalls<A extends unknown[], R>() {
  const calls: { args: A; resolve: (value: R) => void; reject: (reason: unknown) => void }[] = [];
  const fn = (...args: A) =>
    new Promise<R>((resolve, reject) => {
      calls.push({ args, resolve, reject });
    });
  return { fn, calls };
}

interface Harness {
  side: PublisherSide;
  clock: ReturnType<typeof manualClock>;
  repos: ReturnType<typeof deferredCalls<[string], unknown>>;
  skills: ReturnType<typeof deferredCalls<[string], unknown>>;
  rereads: ReturnType<typeof deferredCalls<[string], unknown>>;
  /** descriptionState 的可控状态序列（同步查询）；每查一笔出队，空了用末值。 */
  states: { progress: number; running: boolean }[];
  stateQueries: { source: string }[];
  syncs: { scope: string; resolve: (v: void) => void; reject: (e: unknown) => void }[];
  installs: { url: string; name: string }[];
  uninstalls: { name: string }[];
  notices: MarketNotice[];
  confirmations: { name: string; commit: () => void }[];
  leaves: number;
}

function harness(options: { installFails?: boolean } = {}): Harness {
  const clock = manualClock();
  const repos = deferredCalls<[string], unknown>();
  const skills = deferredCalls<[string], unknown>();
  const rereads = deferredCalls<[string], unknown>();
  const syncs: Harness["syncs"] = [];
  const states: Harness["states"] = [];
  const stateQueries: Harness["stateQueries"] = [];
  const installs: Harness["installs"] = [];
  const uninstalls: Harness["uninstalls"] = [];
  const notices: MarketNotice[] = [];
  const confirmations: Harness["confirmations"] = [];
  let leaves = 0;
  const deps: PublisherSideDeps = {
    reposLoader: repos.fn,
    skillsLoader: skills.fn,
    rereader: rereads.fn,
    descriptionState: (source) => {
      stateQueries.push({ source });
      if (states.length > 1) return states.shift() as { progress: number; running: boolean };
      return states[0] ?? { progress: 0, running: false };
    },
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
    leaveMarket: () => {
      leaves += 1;
    },
  };
  return {
    side: createPublisherSide(deps),
    clock,
    repos,
    skills,
    rereads,
    states,
    stateQueries,
    syncs,
    installs,
    uninstalls,
    notices,
    confirmations,
    // 闭包计数器：getter 现读，避免对象字面量的值快照。
    get leaves() {
      return leaves;
    },
  };
}

function reposPayload(rows: { repo: string; source: string; skillCount?: number; installs?: number; installsLabel?: string }[]) {
  return {
    data: rows.map((row) => ({
      repo: row.repo,
      source: row.source,
      skillCount: row.skillCount ?? 3,
      installsLabel: row.installsLabel ?? "",
      installs: row.installs ?? 0,
    })),
    snapshotStatus: "fresh",
    snapshotUpdatedAt: null,
    error: null,
  };
}

function skillsPayload(rows: { name: string; description?: string; installed?: boolean }[], status = "fresh") {
  return {
    data: rows.map((row) => ({
      name: row.name,
      description: row.description ?? "",
      stars: 1,
      installed: row.installed === true,
      source: "nvidia/skills",
      gitUrl: `https://github.com/${row.name}`,
    })),
    snapshotStatus: status,
    snapshotUpdatedAt: null,
    error: null,
  };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await Promise.resolve();
}

function skillFixture(name: string, overrides: Partial<MarketSkill> = {}): MarketSkill {
  return {
    ...parseSkill({ name, description: `${name} desc`, stars: 0, source: "nvidia/skills" }),
    ...overrides,
  };
}

// ---- 验收红线：epoch 丢弃迟到响应 ------------------------------------------

test("换发布者：旧发布者的仓库清单迟到被丢，界面只认新发布者", async () => {
  const h = harness();
  h.side.open({ name: "nvidia" }); // repos call 0
  h.side.open({ name: "anthropic" }); // repos call 1（重置并重读）
  expect(h.repos.calls.map((call) => call.args[0])).toEqual(["nvidia", "anthropic"]);

  // 新发布者先回：清单落地。
  h.repos.calls[1].resolve(reposPayload([{ repo: "skills", source: "anthropic/skills" }]));
  await settle();
  expect(h.side.repos().map((repo) => repo.source)).toEqual(["anthropic/skills"]);

  // 旧发布者的迟到响应：绝不能覆盖。
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  expect(h.side.repos().map((repo) => repo.source)).toEqual(["anthropic/skills"]);
  expect(h.side.publisherName()).toBe("anthropic");
});

test("快速连开两仓库：旧仓库的技能迟到被丢，网格只认最后打开的仓库", async () => {
  const h = harness();
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();

  h.side.openRepo("nvidia/skills"); // skills call 0
  h.side.openRepo("nvidia/agents"); // skills call 1（换仓库）
  expect(h.skills.calls.map((call) => call.args[0])).toEqual(["nvidia/skills", "nvidia/agents"]);

  // 最后打开的仓库先回。
  h.skills.calls[1].resolve(skillsPayload([{ name: "agents-one" }]));
  await settle();
  expect(h.side.skills().map((skill) => skill.name)).toEqual(["agents-one"]);
  expect(h.side.activeRepo()).toBe("nvidia/agents");

  // 旧仓库的迟到响应（连同它的描述回填 watch）：丢弃，不重读。
  const rereadsBefore = h.rereads.calls.length;
  h.skills.calls[0].resolve(skillsPayload([{ name: "skills-one", description: "" }]));
  await settle();
  expect(h.side.skills().map((skill) => skill.name)).toEqual(["agents-one"]);
  expect(h.rereads.calls.length).toBe(rereadsBefore);
});

test("返回仓库清单后，在途的技能响应与描述回填都被丢弃", async () => {
  const h = harness();
  h.states.push({ progress: 0, running: false });
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();

  h.side.openRepo("nvidia/skills");
  // 先返回（go_up 仓库内 → 退回清单），技能读取还在途。
  h.side.goUp();
  expect(h.side.activeRepo()).toBe(null);
  expect(h.leaves).toBe(0); // 仓库内 go_up 不离开发布者

  h.skills.calls[0].resolve(skillsPayload([{ name: "late", description: "" }]));
  await settle();
  expect(h.side.skills()).toEqual([]); // 迟到响应丢弃
  expect(h.rereads.calls.length).toBe(0); // 描述回填 watch 也未启动
});

// ---- go_up 两级返回 --------------------------------------------------------

test("go_up：仓库清单态离开发布者回市场（leaveMarket）", () => {
  const h = harness();
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([]));
  h.side.goUp();
  expect(h.leaves).toBe(1);
});

// ---- 同步（sync_current_scope）---------------------------------------------

test("同步范围：仓库清单态 publisher_repos 小写、仓库内 repo_skills", async () => {
  const h = harness();
  h.side.open({ name: "NVIDIA" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();

  h.side.syncSnapshot();
  expect(h.syncs.map((sync) => sync.scope)).toEqual(["publisher_repos:nvidia"]);
  h.syncs[0].resolve(undefined);
  await settle();
  // 成功后重读仓库清单。
  expect(h.repos.calls.length).toBe(2);

  h.repos.calls[1].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(skillsPayload([{ name: "a" }]));
  await settle();

  h.side.syncSnapshot();
  expect(h.syncs[1].scope).toBe("repo_skills:nvidia/skills");
});

test("同步成功后重载当前仓库并亮状态文案；失败进错误位且不重载", async () => {
  const h = harness();
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(skillsPayload([{ name: "v1" }]));
  await settle();

  h.side.syncSnapshot();
  h.syncs[0].resolve(undefined);
  await settle();
  expect(h.side.refreshedFlash()).toBe(true);
  expect(h.skills.calls.length).toBe(2); // 当前仓库重读

  h.skills.calls[1].resolve(skillsPayload([{ name: "v2" }]));
  await settle();
  expect(h.side.skills().map((skill) => skill.name)).toEqual(["v2"]);

  // 失败路径：错误进横幅位，不再触发读取。
  const callsBefore = h.skills.calls.length;
  h.side.syncSnapshot();
  h.syncs[1].reject(new Error("offline"));
  await settle();
  expect(h.side.snapshotError()).toContain("offline");
  expect(h.skills.calls.length).toBe(callsBefore);
});

// ---- 描述回填（watch_repo_descriptions + fill_empty_descriptions）----------

test("fillEmptyDescriptions 只填空白行，不覆盖已有文字（GPUI 同名测试对位）", () => {
  const current = [
    skillFixture("a", { description: "" }),
    skillFixture("b", { description: "kept" }),
  ];
  const fresh = [
    skillFixture("a", { description: " now " }),
    skillFixture("b", { description: "other" }),
  ];
  const merged = fillEmptyDescriptions(current, fresh);
  expect(merged).not.toBe(null);
  expect(merged?.[0]?.description).toBe("now");
  expect(merged?.[1]?.description).toBe("kept");
  // 再来一遍：没有可填行 → null（不写回）。
  expect(fillEmptyDescriptions(merged as MarketSkill[], fresh)).toBe(null);
});

test("描述回填：水位推进才重读，空描述被补上而列表不翻回骨架", async () => {
  const h = harness();
  // 轮询序列：先 running=true 水位 0，再 running=true 水位 2，最后收工。
  h.states.push(
    { progress: 0, running: true },
    { progress: 2, running: true },
    { progress: 2, running: false }
  );
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(
    skillsPayload([
      { name: "blank", description: "" },
      { name: "kept", description: "kept" },
    ])
  );
  await settle();

  // 首查水位 0：与 seen 相同 → 不重读，排 100ms 下一查。
  expect(h.rereads.calls.length).toBe(0);
  h.clock.runFirst(); // 第二查：水位 2 → 重读 call 0
  expect(h.rereads.calls.length).toBe(1);
  h.rereads.calls[0].resolve(
    skillsPayload([
      { name: "blank", description: "now filled" },
      { name: "kept", description: "other" },
    ])
  );
  await settle();
  expect(h.side.skills().find((skill) => skill.name === "blank")?.description).toBe("now filled");
  expect(h.side.skills().find((skill) => skill.name === "kept")?.description).toBe("kept");
  // 全程不翻骨架。
  expect(h.side.loadingSkills()).toBe(false);

  // 第三查收工（running=false）：重读一次后轮询停止，不再排计时器。
  h.clock.runFirst();
  expect(h.rereads.calls.length).toBe(2);
  h.rereads.calls[1].resolve(
    skillsPayload([{ name: "blank", description: "now filled" }])
  );
  await settle();
  expect(h.clock.pendingCount()).toBe(0);
});

test("描述回填的迟到 reread 落地前对身份：换仓库后丢弃", async () => {
  const h = harness();
  h.states.push({ progress: 1, running: false });
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(
    reposPayload([
      { repo: "skills", source: "nvidia/skills" },
      { repo: "agents", source: "nvidia/agents" },
    ])
  );
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(skillsPayload([{ name: "blank", description: "" }]));
  await settle();
  // 首查水位 1（≠seen 0）→ reread call 0 在途。
  expect(h.rereads.calls.length).toBe(1);

  // reread 在途时换仓库：旧 reread 落地必须丢。
  h.side.openRepo("nvidia/agents"); // skills call 1
  h.rereads.calls[0].resolve(skillsPayload([{ name: "blank", description: "late fill" }]));
  h.skills.calls[1].resolve(skillsPayload([{ name: "agents-one", description: "agents desc" }]));
  await settle();
  expect(h.side.skills().map((skill) => skill.name)).toEqual(["agents-one"]);
  expect(h.side.skills()[0]?.description).not.toBe("late fill");
});

// ---- 安装/卸载（确认与通知）-------------------------------------------------

test("安装立即提交；卸载先确认，确认前不动域；成功后就地翻 installed", async () => {
  const h = harness();
  h.states.push({ progress: 0, running: false });
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(
    skillsPayload([
      { name: "loose-one", installed: false },
      { name: "installed-one", installed: true },
    ])
  );
  await settle();
  const loose = h.side.skills().find((skill) => skill.name === "loose-one");
  const installed = h.side.skills().find((skill) => skill.name === "installed-one");
  if (!loose || !installed) throw new Error("fixture rows missing");

  // 未装技能：点击 = 安装，立即提交（无确认）。
  h.side.toggleInstalled(loose);
  expect(h.confirmations.length).toBe(0);
  expect(h.installs.map((call) => call.name)).toEqual(["loose-one"]);
  await settle();
  expect(h.notices).toEqual([{ kind: "installed", name: "loose-one" }]);
  expect(h.side.skills().find((skill) => skill.name === "loose-one")?.installed).toBe(true);

  // 已装技能：点击 = 卸载，先弹确认；确认之前域不动。
  h.side.toggleInstalled(installed);
  expect(h.uninstalls.length).toBe(0);
  expect(h.confirmations.map((entry) => entry.name)).toEqual(["installed-one"]);
  h.confirmations[0]?.commit();
  expect(h.uninstalls.map((call) => call.name)).toEqual(["installed-one"]);
  await settle();
  expect(h.side.skills().find((skill) => skill.name === "installed-one")?.installed).toBe(false);
});

test("失败路径：错误进通知带，状态不翻", async () => {
  const h = harness({ installFails: true });
  h.states.push({ progress: 0, running: false });
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(skillsPayload([{ name: "demo" }]));
  await settle();
  const row = h.side.skills()[0];
  if (!row) throw new Error("fixture row missing");
  h.side.toggleInstalled(row);
  await settle();
  const notice = h.notices[0];
  if (!notice || notice.kind !== "installFailed") throw new Error("expected installFailed");
  expect(notice.error).toContain("boom");
  expect(h.side.skills()[0]?.installed).toBe(false);
});

// ---- 选择（详情列）----------------------------------------------------------

test("再点同一张卡收起详情；换仓库清选择；列表重载 retain", async () => {
  const h = harness();
  h.states.push({ progress: 0, running: false });
  h.side.open({ name: "nvidia" });
  h.repos.calls[0].resolve(reposPayload([{ repo: "skills", source: "nvidia/skills" }]));
  await settle();
  h.side.openRepo("nvidia/skills");
  h.skills.calls[0].resolve(skillsPayload([{ name: "alpha" }]));
  await settle();
  const alpha = h.side.skills()[0];
  if (!alpha) throw new Error("fixture row missing");

  h.side.openSkill(alpha);
  expect(h.side.selected()?.name).toBe("alpha");
  h.side.openSkill(alpha);
  expect(h.side.selected()).toBe(null);

  // 选中后换仓库：选择清空（load_repo_skills_with 的 detail.clear）。
  h.side.openSkill(alpha);
  h.side.openRepo("nvidia/agents");
  expect(h.side.selected()).toBe(null);
});

// ---- hero 计数（hero.rs total_* 的本地推导）---------------------------------

test("heroCounts：仓库表非空用行内和，空表回退发布者行计数", () => {
  const rows = [
    { repo: "a", source: "o/a", skillCount: 4, installsLabel: "", installs: 1_200_000 },
    { repo: "b", source: "o/b", skillCount: 6, installsLabel: "4.2M", installs: 4_200_000 },
  ];
  expect(heroCounts(null, rows)).toEqual({ repos: 2, skills: 10, installs: 5_400_000 });
  expect(heroCounts({ repoCount: 9, skillCount: 40 }, rows)).toEqual({
    repos: 2,
    skills: 10,
    installs: 5_400_000,
  });
  expect(heroCounts({ repoCount: 9, skillCount: 40 }, [])).toEqual({
    repos: 9,
    skills: 40,
    installs: 0,
  });
  expect(heroCounts(null, [])).toEqual({ repos: 0, skills: 0, installs: 0 });
});

// ---- 状态口径 ---------------------------------------------------------------

test("轮询间隔常量与 GPUI 的 100ms 对齐", () => {
  expect(DESCRIPTION_POLL_MS).toBe(100);
});

// ---- UI（app realm；桥侧同库数据驱动，双分支容错）---------------------------

interface PublisherFixture {
  name: string;
  firstRepo: string | null;
  firstRepoSkill: string | null;
}

/** 打开应用并从市场官方 tab 进入第一个发布者（空库/离线返回 null 跳过）。 */
async function openFirstPublisher(): Promise<{ app: Awaited<ReturnType<typeof open>>; fixture: PublisherFixture | null }> {
  const app = await open("../../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.press("id:nav-market");
  await app.press("id:mk-tab-official");
  await app.waitUntil(
    () => app.shows("id:mk-publishers-count") || app.shows("id:mk-empty") || app.shows("id:mk-snapshot-banner"),
    { timeout: 15000 }
  );
  if (!app.shows("id:mk-publishers-count")) return { app, fixture: null };
  try {
    const payload = (await Marketplace.publishers()) as { data?: { name?: string }[] };
    const rows = Array.isArray(payload.data) ? payload.data : [];
    const first = rows.find((row) => typeof row?.name === "string");
    if (!first?.name) return { app, fixture: null };
    await app.waitUntil(() => app.shows(`id:pub-card-${first.name}`), { timeout: 15000 });
    await app.press(`id:pub-card-${first.name}`);
    await app.waitUntil(() => app.shows("id:page-publisher_detail"), { timeout: 5000 });
    // 桥侧同库预读：第一个仓库行与（若点开）第一个技能名，用于确定性 testID。
    let firstRepo: string | null = null;
    let firstRepoSkill: string | null = null;
    try {
      const repos = (await MarketplacePublisher.publisherRepos(first.name)) as {
        data?: { repo?: string; source?: string }[];
      };
      const repoRows = Array.isArray(repos.data) ? repos.data : [];
      const repo = repoRows.find((row) => typeof row?.repo === "string" && typeof row?.source === "string");
      if (repo?.repo && repo.source) {
        firstRepo = repo.repo;
        const skills = (await MarketplacePublisher.repoSkills(repo.source)) as {
          data?: { name?: string }[];
        };
        const skillRows = Array.isArray(skills.data) ? skills.data : [];
        const skill = skillRows.find((row) => typeof row?.name === "string");
        firstRepoSkill = skill?.name ?? null;
      }
    } catch {
      firstRepo = null;
    }
    return {
      app,
      fixture: { name: first.name, firstRepo, firstRepoSkill },
    };
  } catch {
    return { app, fixture: null };
  }
}

test("发布者页顶栏齐备；侧栏市场行同屏（选中框留市场的红线）", async () => {
  const { app, fixture } = await openFirstPublisher();
  if (fixture === null) return; // 空库/离线分支：跳过
  await app.waitUntil(() => app.shows("id:pd-page"), { timeout: 5000 });
  expect(app.shows("id:pd-back-btn")).toBe(true);
  expect(app.shows("id:pd-search")).toBe(true);
  expect(app.shows("id:pd-sync-btn")).toBe(true);
  await app.waitUntil(() => app.shows("id:pd-hero"), { timeout: 15000 });
  // 侧栏市场行仍在（railPage 映射的市场下钻语义在 UI 上成立）。
  expect(app.shows("id:nav-market")).toBe(true);
  expect(railPage("publisher_detail")).toBe("marketplace");
});

test("发布者页：仓库行 → 技能网格 → 两级返回（先回仓库清单，再回市场）", async () => {
  const { app, fixture } = await openFirstPublisher();
  if (fixture === null || fixture.firstRepo === null) return;
  const repoRow = `id:pub-repo-row-${fixture.firstRepo}`;
  await app.waitUntil(() => app.shows(repoRow), { timeout: 15000 });

  // 点开仓库：技能网格（或空态）替换仓库清单，标题出现仓库名。
  await app.press(repoRow);
  await app.waitUntil(
    () => app.shows("id:pd-empty") || (fixture.firstRepoSkill !== null && app.shows(`id:mk-card-${fixture.firstRepoSkill}`)),
    { timeout: 20000 }
  );
  // 返回一级：回仓库清单（仓库行重现，技能网格让位）。
  await app.press("id:pd-back-btn");
  await app.waitUntil(() => app.shows(repoRow), { timeout: 15000 });
  // 返回二级：离开发布者回市场页。
  await app.press("id:pd-back-btn");
  await app.waitUntil(() => app.shows("id:mk-tabs") && !app.shows("id:pd-hero"), { timeout: 5000 });
});
