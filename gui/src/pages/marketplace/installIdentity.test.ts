// 市场安装的身份回归 — 安装器遇到重名会把技能并存改名为 <name>-<owner>
// （library.json 的 called 记原名），桥返回的是落盘名而不是市场行名。
// 状态机必须翻用户操作的那一行；卸载必须带上行的来源，域侧据此解析真正
// 的落盘目录。2026-11-24：只按返回名匹配时，重名安装完成后卡片停在
// 「安装」，从卡片卸载则卸掉了同名的另一条记录。
import { expect, test } from "nsh/test";
import { createReadSide, type MarketNotice } from "./readSide";
import { createPublisherSide } from "./publisher/readSide";

const SOURCE = "nextlevelbuilder/ui-ux-pro-max-skill";
/** 市场行名，也是安装器记进 called 的原名。 */
const ROW = "ui-ux-pro-max";
/** 安装器并存改名后的落盘名。 */
const FOLDER = "ui-ux-pro-max-nextlevelbuilder";

/** 不排任何计时器：这些用例不看延迟出现的任务卡片。 */
function noTimers(): (fn: () => void, ms: number) => () => void {
  return () => () => {};
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
}

type Payload = { data: unknown; snapshotStatus: string; snapshotUpdatedAt: null; error: null };

function skillsPayload(rows: { name: string; installed?: boolean; source?: string | null }[]): Payload {
  return {
    data: rows.map((row) => ({
      name: row.name,
      description: `${row.name} desc`,
      stars: 1,
      installed: row.installed === true,
      source: row.source === null ? undefined : (row.source ?? SOURCE),
      gitUrl: `https://github.com/${SOURCE}`,
    })),
    snapshotStatus: "fresh",
    snapshotUpdatedAt: null,
    error: null,
  };
}

function reposPayload(): Payload {
  return {
    data: [{ repo: "ui-ux-pro-max-skill", source: SOURCE, skillCount: 1, installsLabel: "", installs: 0 }],
    snapshotStatus: "fresh",
    snapshotUpdatedAt: null,
    error: null,
  };
}

function rowOf<T extends { name: string }>(rows: T[], name: string): T {
  const row = rows.find((candidate) => candidate.name === name);
  if (!row) throw new Error(`fixture row missing: ${name}`);
  return row;
}

// ---- 市场页状态机 -----------------------------------------------------------

function market(options: { folder: string } = { folder: FOLDER }) {
  const loads: ((payload: unknown) => void)[] = [];
  const uninstalls: { name: string; source: string | null | undefined }[] = [];
  const notices: MarketNotice[] = [];
  const machine = createReadSide({
    loader: () =>
      new Promise<{ kind: "skills"; payload: unknown }>((resolve) => {
        loads.push((payload) => resolve({ kind: "skills", payload }));
      }),
    syncer: () => new Promise<void>(() => {}),
    installer: (_url, name) => Promise.resolve(options.folder === "" ? name : options.folder),
    uninstaller: (name, source) => {
      uninstalls.push({ name, source });
      return Promise.resolve(FOLDER);
    },
    schedule: noTimers(),
    notify: (notice) => notices.push(notice),
    confirmUninstall: (_name, commit) => commit(),
  });
  return { machine, loads, uninstalls, notices };
}

async function loadedMarket(options: { folder?: string; rows?: { name: string; installed?: boolean; source?: string | null }[] } = {}) {
  const h = market({ folder: options.folder ?? FOLDER });
  h.machine.start();
  h.loads[0](skillsPayload(options.rows ?? [{ name: ROW }]));
  await settle();
  return h;
}

test("并存改名：安装返回落盘名，被操作的行照样翻成已安装", async () => {
  const h = await loadedMarket();
  h.machine.toggleInstalled(rowOf(h.machine.skills(), ROW));
  await settle();
  expect(rowOf(h.machine.skills(), ROW).installed).toBe(true);
  expect(h.machine.busy().size).toBe(0);
  expect(h.notices).toEqual([{ kind: "installed", name: FOLDER }]);
});

test("落盘名本身也是列表里的一行时，两行一起翻", async () => {
  const h = await loadedMarket({ rows: [{ name: ROW }, { name: FOLDER }] });
  h.machine.toggleInstalled(rowOf(h.machine.skills(), ROW));
  await settle();
  expect(rowOf(h.machine.skills(), ROW).installed).toBe(true);
  expect(rowOf(h.machine.skills(), FOLDER).installed).toBe(true);
});

test("未改名的安装只翻被操作的那一行", async () => {
  const h = await loadedMarket({ folder: "", rows: [{ name: ROW }, { name: FOLDER }] });
  h.machine.toggleInstalled(rowOf(h.machine.skills(), ROW));
  await settle();
  expect(rowOf(h.machine.skills(), ROW).installed).toBe(true);
  expect(rowOf(h.machine.skills(), FOLDER).installed).toBe(false);
});

test("卸载带上行的来源；返回落盘名时翻回被操作的行", async () => {
  const h = await loadedMarket({ rows: [{ name: ROW, installed: true }] });
  h.machine.toggleInstalled(rowOf(h.machine.skills(), ROW));
  await settle();
  expect(h.uninstalls).toEqual([{ name: ROW, source: SOURCE }]);
  expect(rowOf(h.machine.skills(), ROW).installed).toBe(false);
  expect(h.notices).toEqual([{ kind: "uninstalled", name: FOLDER }]);
});

test("没有来源的行，卸载时不带来源", async () => {
  const h = await loadedMarket({ rows: [{ name: ROW, installed: true, source: null }] });
  h.machine.toggleInstalled(rowOf(h.machine.skills(), ROW));
  await settle();
  expect(h.uninstalls).toEqual([{ name: ROW, source: null }]);
  expect(rowOf(h.machine.skills(), ROW).installed).toBe(false);
});

// ---- 发布者仓库技能卡 -------------------------------------------------------

test("发布者仓库技能卡：并存改名后照样翻，卸载带来源", async () => {
  const repos: ((payload: unknown) => void)[] = [];
  const skills: ((payload: unknown) => void)[] = [];
  const uninstalls: { name: string; source: string | null | undefined }[] = [];
  const notices: MarketNotice[] = [];
  const side = createPublisherSide({
    reposLoader: () =>
      new Promise<unknown>((resolve) => {
        repos.push(resolve);
      }),
    skillsLoader: () =>
      new Promise<unknown>((resolve) => {
        skills.push(resolve);
      }),
    rereader: () => new Promise<unknown>(() => {}),
    descriptionState: () => ({ progress: 0, running: false }),
    syncer: () => new Promise<void>(() => {}),
    installer: (_url, name) => Promise.resolve(`${name}-nextlevelbuilder`),
    uninstaller: (name, source) => {
      uninstalls.push({ name, source });
      return Promise.resolve(FOLDER);
    },
    schedule: noTimers(),
    notify: (notice) => notices.push(notice),
    confirmUninstall: (_name, commit) => commit(),
    leaveMarket: () => {},
  });
  side.open({ name: "nextlevelbuilder" });
  repos[0](reposPayload());
  await settle();
  side.openRepo(SOURCE);
  skills[0](skillsPayload([{ name: ROW }]));
  await settle();

  side.toggleInstalled(rowOf(side.skills(), ROW));
  await settle();
  expect(rowOf(side.skills(), ROW).installed).toBe(true);

  side.toggleInstalled(rowOf(side.skills(), ROW));
  await settle();
  expect(uninstalls).toEqual([{ name: ROW, source: SOURCE }]);
  expect(rowOf(side.skills(), ROW).installed).toBe(false);
  expect(notices).toEqual([
    { kind: "installed", name: FOLDER },
    { kind: "uninstalled", name: FOLDER },
  ]);
});
