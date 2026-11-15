// 波 4 异步桥基座的机制证明 — 真实 native DomainJob（new/token/cancelled/
// cancel/dispose）+ services/domainJob.ts（startJob + epoch 门）。
// 全部经真实 native module 在 app realm 内驱动，不触文件系统不写 $HOME。
import { expect, open, test, type App } from "nsh/test";

const HOST = "./domainJob.test.host.tsx";

function progressText(app: App): string {
  return app.texts().find((t) => t.startsWith("count:")) ?? "";
}

test("new DomainJob issues unique non-zero tokens and starts uncancelled", async () => {
  const app = await open(HOST, { width: 900, height: 400, reducedMotion: true });
  await app.press("id:host-tokens");
  await app.waitUntil(
    () => app.texts().some((t) => t.startsWith("tokens:") && t !== "tokens:idle"),
    { timeout: 2000 },
  );
  const status = app.texts().find((t) => t.startsWith("tokens:")) ?? "";
  expect(status).toContain("unique:true");
  expect(status).toContain("initCancelled:false");
});

test("startJob streams latest/count events and settles normally before stop", async () => {
  const app = await open(HOST, { width: 900, height: 400, reducedMotion: true });
  await app.press("id:host-start");
  await app.waitUntil(() => /token:[1-9]\d*/.test(progressText(app)), { timeout: 2000 });
  const initial = progressText(app);
  expect(initial).toContain("count:0");
  expect(initial).toContain("stopped:false");
  expect(initial).toContain("cancelled:false");

  await app.press("id:host-emit");
  await app.press("id:host-emit");
  await app.waitUntil(() => progressText(app).includes("count:2"), { timeout: 2000 });
  const mid = progressText(app);
  expect(mid).toContain("latest:2");
  expect(mid).toContain("stopped:false");
  expect(mid).toContain("cancelled:false");

  await app.press("id:host-settle");
  await app.waitUntil(() => app.texts().includes("steps:2 cancelled:false"), { timeout: 2000 });
});

test("stop flips native cancelled and gates late events and late promise settlements", async () => {
  const app = await open(HOST, { width: 900, height: 400, reducedMotion: true });
  await app.press("id:host-start");
  await app.press("id:host-emit");
  await app.waitUntil(() => progressText(app).includes("count:1"), { timeout: 2000 });
  expect(progressText(app)).toContain("cancelled:false");

  await app.press("id:host-stop");
  await app.waitUntil(() => progressText(app).includes("stopped:true"), { timeout: 2000 });
  expect(progressText(app)).toContain("cancelled:true");

  // epoch 门：stop() 之后的迟到事件和迟到 promise 结算都不落屏。
  await app.press("id:host-late-emit");
  await app.press("id:host-late-settle");
  await app.wait(100);
  const after = progressText(app);
  expect(after).toContain("count:1");
  expect(after).toContain("latest:1");
  expect(after).toContain("stopped:true");
  expect(after).toContain("cancelled:true");
  expect(app.texts()).toContain("pending");
  expect(app.texts().some((t) => t.startsWith("steps:999"))).toBe(false);
});

test("dispose drops the native DomainJob instance", async () => {
  const app = await open(HOST, { width: 900, height: 400, reducedMotion: true });
  await app.press("id:host-start");
  await app.waitUntil(() => /token:[1-9]\d*/.test(progressText(app)), { timeout: 2000 });

  await app.press("id:host-dispose");
  await app.waitUntil(
    () => app.texts().some((t) => t.startsWith("dispose:before=")),
    { timeout: 2000 },
  );
  const status = app.texts().find((t) => t.startsWith("dispose:")) ?? "";
  expect(status).toBe("dispose:before=false throwsAfter=true");
});
