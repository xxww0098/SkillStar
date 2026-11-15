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
