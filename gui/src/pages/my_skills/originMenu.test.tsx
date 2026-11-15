// 来源/仓库菜单（my_skills/toolbar/origin_menu.rs 对位）的无头测试：仓库名单
// 超过八行时，弹层里是「明确像素高度的视口框 + <Scroll>」的滚动盒——只写
// 高度不挂 <Scroll> 时溢出内容被面板 overflow:hidden 裁掉，滚轮无处可滚
// （2026-11-16 回归，见 docs/errors.md）。八行及以内高度跟内容走，不出现滚动
// 盒。真实写盘（收藏/重装/移除）由 native crate 的 cargo test 覆盖，本装置只
// 驱动名单的渲染与选择。
import { expect, open, test, type App } from "nsh/test";

const HOST = "./originMenu.test.host.tsx";
// 桌面视口：面板 340px 宽、仓库视口 280px 高，挂在触发器下方要整块可见。
const VIEWPORT = { width: 720, height: 760 } as const;

/** 夹具在页面首次真实装载落地后才播种，等种子到位再开弹层。 */
async function openOriginMenu(app: App, repoSkills: number): Promise<void> {
  await app.waitUntil(() => app.texts().includes(`skills:${repoSkills}`));
  await app.press("id:my-skills-origin-trigger");
  await app.waitFor("id:my-skills-origin-panel");
}

test("a long repo list scrolls inside a fixed-height viewport box", async () => {
  const app = await open(HOST, VIEWPORT);
  await openOriginMenu(app, 12);
  // 13 行（全部仓库 + 12 仓）超出 8 行视口：视口框里是真滚动容器
  // （不是滚动容器时 scroll 步骤被拒绝，skillReader.test.tsx 同款判据）。
  await app.waitFor("id:origin-repo-scroll");
  expect(app.shows("id:origin-repo-scroll-list")).toBe(true);
  expect(app.count("id:origin-repo-row-repo-11")).toBe(1);
  // 滚到底：名单不在真滚动容器里时这一步被拒绝（skillReader.test.tsx 同款判据）。
  await app.scroll("id:origin-repo-scroll-list", 2000);
  // 滚到底后第九行起的仓库可点：选中即写 repo 筛选并收弹层。
  await app.press("id:origin-repo-row-repo-11");
  await app.waitUntil(() => app.texts().includes("repo:repo-11"));
  expect(app.shows("id:my-skills-origin-panel")).toBe(false);
});

test("a short repo list keeps its natural height without a scroll box", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.waitUntil(() => app.texts().includes("skills:12"));
  await app.press("id:host-few-repos");
  await openOriginMenu(app, 3);
  // 4 行（全部仓库 + 3 仓）不超过阈值：不出现视口框与滚动容器。
  expect(app.shows("id:origin-repo-scroll")).toBe(false);
  expect(app.shows("id:origin-repo-scroll-list")).toBe(false);
  await app.press("id:origin-repo-row-repo-02");
  await app.waitUntil(() => app.texts().includes("repo:repo-02"));
});
