// chrome/feedback 的结构面：翻译按钮的激光扫描（icon_sweep 对位）与转圈
// 字形必须跟着 props 起停。回归的是「组件体只执行一次」这条纪律——首帧写
// 死静态字形，之后再翻开关就永远不扫（GPUI 的 icon_sweep/icon_spin 是按
// 每次绘制读 flag 的，nsh 侧只能落在 <If> 表达式里）。
import { expect, open, test } from "nsh/test";

const HOST = "./feedback.test.host.tsx";

test("the sweep mounts with its flag after the first paint, and leaves with it", async () => {
  const app = await open(HOST);
  await app.waitUntil(() => app.shows("id:host-toggle-sweep"));
  // 静止态：只有字形，没有激光。
  expect(app.shows("id:sweep-beam")).toBe(false);
  await app.press("id:host-toggle-sweep");
  await app.waitFor("id:sweep-beam");
  await app.press("id:host-toggle-sweep");
  await app.waitUntil(() => !app.shows("id:sweep-beam"));
});

test("the spinner starts turning after the first paint, and stops with it", async () => {
  const app = await open(HOST);
  await app.waitUntil(() => app.shows("id:host-toggle-spin"));
  expect(app.shows("id:spin-frame")).toBe(false);
  await app.press("id:host-toggle-spin");
  await app.waitFor("id:spin-frame");
  await app.press("id:host-toggle-spin");
  await app.waitUntil(() => !app.shows("id:spin-frame"));
});
