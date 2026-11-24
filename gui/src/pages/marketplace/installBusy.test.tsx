// 市场安装的忙碌记录 — 并发安装对位。症状：点了第二张卡的安装，第一张
// 卡的转圈停了。状态机部分直接建实例、可控 Promise 断言忙碌集的增删；
// 组件部分经 installBusy.test.host.tsx 的装置按真实卡面按钮驱动，断言
// 两张卡的转圈同时在画面上。发布者页的同款状态机断言在
// publisher/publisher.test.tsx。
import { expect, open, test } from "nsh/test";
import { HOLD_MS, TURN_MS } from "../../chrome/delayedFlag";
import { NO_BUSY, createReadSide, type MarketNotice } from "./readSide";

const HOST = "./installBusy.test.host.tsx";

/** 转圈松开后滑行到一圈结束才收起，最长 `TURN_MS` + `HOLD_MS`。要断言
 * 「还在转」，得等这段收尾过去：停了的此时已经不在画面上。 */
const COAST_MS = TURN_MS + HOLD_MS + 200;

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
  }
}

function harness() {
  const installCalls: {
    name: string;
    resolve: (name: string) => void;
    reject: (reason: unknown) => void;
  }[] = [];
  const notices: MarketNotice[] = [];
  const machine = createReadSide({
    loader: async () => ({
      kind: "skills" as const,
      payload: {
        data: ["alpha", "beta"].map((name) => ({ name, description: "", stars: 0 })),
        snapshotStatus: "fresh",
        snapshotUpdatedAt: null,
        error: null,
      },
    }),
    syncer: async () => {},
    installer: (_url, name) =>
      new Promise<string>((resolve, reject) => {
        installCalls.push({ name, resolve, reject });
      }),
    uninstaller: async (name) => name,
    schedule: () => () => {},
    notify: (notice) => notices.push(notice),
    confirmUninstall: () => {},
  });
  return { machine, installCalls, notices };
}

// ---- 状态机 -----------------------------------------------------------------

test("并发安装：各卡各自忙碌，先完成的只放开自己", async () => {
  const h = harness();
  h.machine.start();
  await settle();
  const [alpha, beta] = h.machine.skills();
  if (!alpha || !beta) throw new Error("fixture rows missing");
  expect(h.machine.busy()).toBe(NO_BUSY);

  h.machine.toggleInstalled(alpha);
  expect([...h.machine.busy()]).toEqual(["alpha"]);

  // 第二张卡开始：不顶掉第一张。
  h.machine.toggleInstalled(beta);
  expect([...h.machine.busy()]).toEqual(["alpha", "beta"]);
  expect(h.installCalls.map((call) => call.name)).toEqual(["alpha", "beta"]);

  // 先发起的先完成：只放开 alpha，beta 照旧在飞。
  h.installCalls[0]?.resolve("alpha");
  await settle();
  expect([...h.machine.busy()]).toEqual(["beta"]);
  expect(h.notices).toEqual([{ kind: "installed", name: "alpha" }]);
  expect(h.machine.skills().map((row) => row.installed)).toEqual([true, false]);

  h.installCalls[1]?.resolve("beta");
  await settle();
  expect(h.machine.busy()).toBe(NO_BUSY);
  expect(h.machine.skills().map((row) => row.installed)).toEqual([true, true]);
});

test("在飞的技能不重复提交；失败只放开自己，之后可以重试", async () => {
  const h = harness();
  h.machine.start();
  await settle();
  const [alpha, beta] = h.machine.skills();
  if (!alpha || !beta) throw new Error("fixture rows missing");

  h.machine.toggleInstalled(alpha);
  h.machine.toggleInstalled(beta);
  // 在飞的 beta 再点一次安装：不发第二次调用，忙碌集不变。
  h.machine.toggleInstalled(beta);
  expect(h.installCalls.length).toBe(2);
  expect([...h.machine.busy()]).toEqual(["alpha", "beta"]);

  h.installCalls[1]?.reject(new Error("boom"));
  await settle();
  expect([...h.machine.busy()]).toEqual(["alpha"]);
  expect(h.notices).toEqual([{ kind: "installFailed", name: "beta", error: "Error: boom" }]);
  expect(h.machine.skills().map((row) => row.installed)).toEqual([false, false]);

  // 不再在飞：重试放行。
  h.machine.toggleInstalled(beta);
  expect(h.installCalls.map((call) => call.name)).toEqual(["alpha", "beta", "beta"]);
  expect([...h.machine.busy()]).toEqual(["alpha", "beta"]);
});

// ---- 卡面 -------------------------------------------------------------------

test("两张卡的转圈同时在画面上：点第二张不停第一张，先完成的只收自己的", async () => {
  const app = await open(HOST, { width: 1280, height: 900 });
  await app.waitUntil(() => app.shows("id:mk-act-alpha"), { timeout: 5000 });

  await app.press("id:mk-act-alpha");
  await app.waitUntil(() => app.shows("id:mk-spin-alpha"), { timeout: 5000 });
  expect(app.shows("id:mk-spin-beta")).toBe(false);

  // 第二张卡开始安装：第一张的转圈不停。等滑行收尾的时间过去再断言，
  // 被停掉的转圈此时已经收起。
  await app.press("id:mk-act-beta");
  await app.waitUntil(() => app.shows("id:mk-spin-beta"), { timeout: 5000 });
  await app.wait(COAST_MS);
  expect(app.shows("id:mk-spin-alpha")).toBe(true);
  expect(app.shows("id:mk-spin-beta")).toBe(true);
  expect(app.texts()).toContain("calls:2");

  // 在飞的卡再点一次：不多发一次安装。
  await app.press("id:mk-act-beta");
  expect(app.texts()).toContain("calls:2");

  // 先发起的先完成：alpha 的转圈收起，beta 还在转。
  await app.press("id:host-resolve-first");
  await app.waitUntil(() => !app.shows("id:mk-spin-alpha"), { timeout: 5000 });
  expect(app.shows("id:mk-spin-beta")).toBe(true);
  expect(app.texts()).toContain("calls:1");

  await app.press("id:host-reject-first");
  await app.waitUntil(() => !app.shows("id:mk-spin-beta"), { timeout: 5000 });
  expect(app.texts()).toContain("calls:0");
});
