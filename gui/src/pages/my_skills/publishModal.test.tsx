import { expect, open, test } from "nsh/test";

const HOST = "./publishModal.test.host.tsx";

test("publish modal renders git missing preflight state with install command", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "set-mode", mode: "not_installed" });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:publish-phase-not-installed");
  expect(app.texts()).toContain("需要先安装 Git");
  expect(app.texts()).toContain("brew install git");
  await app.host.send("harness", { action: "close" });
});

test("publish modal renders expired auth state and triggers sign-in flow", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "set-mode", mode: "not_authenticated" });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:publish-phase-not-authenticated");
  expect(app.texts()).toContain("需要 GitHub 授权");
  await app.press("publish-sign-in");
  await app.waitUntil(() => app.texts().some((t) => t.includes("signIn:1")));
  await app.host.send("harness", { action: "close" });
});

test("publish modal submits ready form and shows published repo url", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "set-mode", mode: "ready" });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:publish-phase-ready");
  expect(app.texts()).toContain("已连接 @octocat");

  await app.press("publish-submit");
  await app.waitFor("id:publish-phase-done");
  expect(app.texts()).toContain("https://github.com/octocat/my-local-skill");
  await app.press("publish-done-close");
  await app.waitUntil(() => !app.shows("id:publish-modal-card"));
});

test("publish modal surfaces error and lets user return to edit form", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "set-mode", mode: "ready" });
  await app.host.send("harness", { action: "set-fail", fail: true });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:publish-phase-ready");
  await app.press("publish-submit");
  await app.waitFor("id:publish-phase-error");
  expect(app.texts().some((t) => t.includes("Repository name already exists"))).toBe(true);
  await app.press("publish-retry");
  await app.waitFor("id:publish-phase-ready");
  await app.host.send("harness", { action: "close" });
});
