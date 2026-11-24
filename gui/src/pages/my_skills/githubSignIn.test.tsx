// GitHub 登录对话框测试 — my_skills/github_sign_in.rs 的 nsh 对位。退避
// 时序（Pending/SlowDown 的 delay 数列、cancel 后不再 poll）在 native crate
// 的 cargo test 断言（my_skills_github.rs 的脚本化 transport）；本文件断言
// 对话框行为：Starting/Waiting/Failed 三相、复制、Denied/域错误的文案分派、
// Connected 写回会话并自动关窗、关闭即取消（veil/Escape）与过期进度不落屏。
// 装置：githubSignIn.test.host.tsx（假桥注入，不打真网）。
import { expect, open, test } from "nsh/test";

const HOST = "./githubSignIn.test.host.tsx";

// 产品默认 locale 是 zh-CN（services/locale.ts），断言用中文文案。

test("opening paints the starting phase while the device flow starts", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:github-sign-in-card");
  await app.waitUntil(() => app.texts().includes("加载中..."));
  // 标题（render_header 的 sharedChannels.signInTitle）。
  expect(app.texts()).toContain("登录 GitHub 以管理共享频道");
});

test("the waiting event paints code, steps, countdown, copy and open actions", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:github-sign-in-card");
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  expect(app.texts()).toContain("ABCD-1234");
  // 两步说明 + 等待与剩余有效期（900s → 15:00 快照，render_waiting）。
  expect(app.texts()).toContain("复制设备码并打开 GitHub");
  expect(app.texts()).toContain("在 GitHub 页面粘贴并确认授权");
  expect(app.texts()).toContain("等待你在 GitHub 完成授权…");
  expect(app.texts()).toContain("15:00 后过期");
  expect(app.shows("id:github-sign-in-copy")).toBe(true);
  expect(app.shows("id:github-sign-in-open")).toBe(true);
});

test("copy puts the code on the clipboard and flips the label", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-copy");
  expect(app.texts()).toContain("复制设备码");
  await app.press("github-sign-in-copy");
  expect(app.clipboard()).toBe("ABCD-1234");
  // copied 反馈（render_waiting :314-318）。
  await app.waitUntil(() => app.texts().includes("已复制"));
});

test("denied paints the localized message and retry restarts the flow", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  await app.host.send("harness", { action: "resolve-denied" });
  await app.waitFor("id:github-sign-in-retry");
  // Denied 以 locale key reject，TSX 侧 t() 转译文（github_sign_in.rs:170）。
  expect(app.texts()).toContain("你拒绝了本次 GitHub 授权。");
  expect(app.shows("id:github-sign-in-code")).toBe(false);

  // 重试：弃旧链、新 signIn（calls 计数 +1）、相位回 Starting。
  await app.press("github-sign-in-retry");
  await app.waitFor("id:github-sign-in-card");
  await app.waitUntil(() => app.texts().some((t) => t.includes("calls:2")));
  await app.waitUntil(() => app.texts().includes("加载中..."));
});

test("a domain error shows verbatim, not as a key", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "resolve-error" });
  await app.waitFor("id:github-sign-in-retry");
  // 域错误消息原样显示（error.to_string() 对位，github_sign_in.rs:176）。
  expect(app.texts()).toContain("Unable to reach GitHub; check the network and proxy settings");
});

test("connected writes the page session and closes the dialog", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  await app.host.send("harness", { action: "resolve-connected" });
  // 卡片自动关闭（close_pending 对位）。
  await app.waitUntil(() => !app.shows("id:github-sign-in-card"));
  // 页面会话写回（page.github = Some(connection)）：装置计数行读 github()。
  await app.waitUntil(() => app.texts().some((t) => t.includes("session:connected@octocat")));
});

test("closing on the veil hides the dialog while keeping polling alive", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  await app.press("github-sign-in-veil");
  await app.waitUntil(() => !app.shows("id:github-sign-in-card"));
  // 关闭只隐藏弹层，后台设备授权继续轮询。
  await app.host.send("harness", { action: "cancelled" });
  await app.waitUntil(() => app.texts().some((t) => t.includes("cancelled:false")));
  await app.waitUntil(() => app.texts().some((t) => t.includes("polling:true")));

  // 重开后直接续显既有设备码，不新开第二轮请求。
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:github-sign-in-code");
  expect(app.texts()).toContain("ABCD-1234");
  expect(app.texts().some((t) => t.includes("calls:1"))).toBe(true);

  // 后台完成授权依然写回会话。
  await app.press("github-sign-in-veil");
  await app.waitUntil(() => !app.shows("id:github-sign-in-card"));
  await app.host.send("harness", { action: "resolve-connected" });
  await app.waitUntil(() => app.texts().some((t) => t.includes("session:connected@octocat")));
});

test("escape closes without cancelling", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:github-sign-in-card");
  await app.keyboard.press("escape");
  await app.waitUntil(() => !app.shows("id:github-sign-in-card"));
  await app.host.send("harness", { action: "cancelled" });
  await app.waitUntil(() => app.texts().some((t) => t.includes("cancelled:false")));
  await app.waitUntil(() => app.texts().some((t) => t.includes("polling:true")));
});

test("explicit cancel button stops the job and blocks late events; reopening starts fresh", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  // 显式点击「取消授权」按钮才中止设备流。
  await app.press("github-sign-in-cancel");
  await app.waitUntil(() => !app.shows("id:github-sign-in-card"));
  await app.host.send("harness", { action: "cancelled" });
  await app.waitUntil(() => app.texts().some((t) => t.includes("cancelled:true")));
  await app.waitUntil(() => app.texts().some((t) => t.includes("polling:false")));

  // 迟到的 waiting 事件 post 进已停的 run。
  await app.host.send("harness", { action: "late-waiting" });
  await app.waitUntil(() => app.texts().some((t) => t.includes("late:1")));
  expect(app.shows("id:github-sign-in-card")).toBe(false);

  // 重开：新任务从 Starting 开始，旧 code 不落屏。
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:github-sign-in-card");
  await app.waitUntil(() => app.texts().includes("加载中..."));
  expect(app.texts()).not.toContain("ABCD-1234");
  // 新任务自己的 waiting 事件正常落屏。
  await app.host.send("harness", { action: "deliver-waiting" });
  await app.waitFor("id:github-sign-in-code");
  expect(app.texts()).toContain("ABCD-1234");
});

test("a cancelled settle after retry does not paint over the fresh flow", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open" });
  await app.host.send("harness", { action: "resolve-denied" });
  await app.waitFor("id:github-sign-in-retry");
  await app.press("github-sign-in-retry");
  await app.waitUntil(() => app.texts().some((t) => t.includes("calls:2")));
  // 第一笔（已过期）结算若迟到也不得改相位——新任务停在 Starting 等待
  // 自己的 waiting 事件。
  await app.waitUntil(() => app.texts().includes("加载中..."));
  expect(app.shows("id:github-sign-in-retry")).toBe(false);
});
