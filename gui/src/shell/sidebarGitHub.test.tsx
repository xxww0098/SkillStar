import { expect, open, test } from "nsh/test";

const HOST = "./sidebarGitHub.test.host.tsx";

function callsLine(app: { texts: () => string[] }): string {
  return app.texts().find((t) => t.startsWith("signIn:")) ?? "";
}

test("sidebar GitHub entry displays all 4 auth states and triggers expected actions", async () => {
  const app = await open(HOST, { width: 420, height: 400 });
  await app.waitUntil(() => app.shows("id:sidebar-github-entry"));

  // 1. signed_out -> 未登录 -> click opens device sign-in
  expect(app.texts()).toContain("未登录");
  expect(app.shows("id:sidebar-github-refresh")).toBe(false);
  await app.press("id:sidebar-github-entry");
  await app.waitUntil(() => callsLine(app).includes("signIn:1"));

  // 2. polling -> 等待授权 -> click re-opens dialog
  await app.press("id:host-polling");
  await app.waitUntil(() => app.texts().includes("等待授权"));
  await app.press("id:sidebar-github-entry");
  await app.waitUntil(() => callsLine(app).includes("signIn:2"));

  // 3. connected -> @octocat -> click opens channels; refresh/logout work
  await app.press("id:host-connected");
  await app.waitUntil(() => app.texts().includes("@octocat"));
  expect(app.shows("id:sidebar-github-refresh")).toBe(true);
  expect(app.shows("id:sidebar-github-logout")).toBe(true);
  await app.press("id:sidebar-github-entry");
  await app.waitUntil(() => callsLine(app).includes("ch:1"));
  await app.press("id:sidebar-github-refresh");
  await app.waitUntil(() => callsLine(app).includes("refresh:1"));

  // 4. expired -> 已过期 -> click opens sign-in; refresh restores
  await app.press("id:host-expired");
  await app.waitUntil(() => app.texts().includes("已过期"));
  expect(app.shows("id:sidebar-github-refresh")).toBe(true);
  await app.press("id:sidebar-github-entry");
  await app.waitUntil(() => callsLine(app).includes("signIn:3"));
  await app.press("id:sidebar-github-refresh");
  await app.waitUntil(() => app.texts().includes("@octocat"));

  // 5. logout resets back to signed_out
  await app.press("id:sidebar-github-logout");
  await app.waitUntil(() => app.texts().includes("未登录"));

  // 6. collapsed hides text label while keeping icon button visible
  await app.press("id:host-collapse");
  await app.waitUntil(() => !app.shows("id:sidebar-github-status"));
  expect(app.shows("id:sidebar-github-entry")).toBe(true);
});
