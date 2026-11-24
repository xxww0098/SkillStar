import { expect, open, test } from "nsh/test";

const HOST = "./skillCard.test.host.tsx";

test("the source chip opens the repo page outside the app, never the detail drawer", async () => {
  const app = await open(HOST, { width: 420, height: 320 });
  await app.waitUntil(() => app.shows("id:skill-card-source-chief-of-staff"));

  // 副标题是外链：点击只记一次外部打开，卡身不动。
  await app.press("id:skill-card-source-chief-of-staff");
  await app.waitUntil(() => app.openedUrls().length > 0);
  expect(app.openedUrls()).toEqual(["https://github.com/mattpocock/skills"]);
  expect(app.texts()).toContain("selected:none");

  // 卡身其他位置仍然打开详情抽屉，且不再外开。
  await app.press("id:skill-card-desc-chief-of-staff");
  await app.waitUntil(() => app.texts().includes("selected:chief-of-staff"));
  expect(app.openedUrls()).toEqual(["https://github.com/mattpocock/skills"]);
});

// 卡面不写保存状态行：夹具已把 `update:` / `link:` 两个保存 id 置成进行中，
// 卡身仍然只有身份、描述和底栏，不多出一行「正在保存」或「已保存」。
test("the card face carries no inline save status line", async () => {
  const app = await open(HOST, { width: 420, height: 320 });
  await app.waitUntil(() => app.shows("id:skill-card-chief-of-staff"));

  expect(app.texts()).not.toContain("正在保存");
  expect(app.texts()).not.toContain("已保存");
});
