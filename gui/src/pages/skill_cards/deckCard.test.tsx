import { expect, open, test } from "nsh/test";

const HOST = "./deckCard.test.host.tsx";

// 卡组卡卡面不写保存状态行：夹具已把 `deck:` 保存 id 置成进行中，卡身
// 仍然只有身份、成员 chips 和底栏，不多出一行「正在保存」或「已保存」。
// 失败由钉住的通知带说明，再点同一个图标就是重试。
test("the deck card face carries no inline save status line", async () => {
  const app = await open(HOST, { width: 420, height: 260 });
  await app.waitUntil(() => app.shows("id:deck-card-deck-fixture"));

  // 底栏开关仍在（卡身没有为了去掉保存行而缩掉交互面）。
  expect(app.shows("id:deck-slot-deck-fixture-claude")).toBe(true);
  expect(app.texts()).not.toContain("正在保存");
  expect(app.texts()).not.toContain("已保存");
  expect(app.shows("id:save-retry-deck:deck-fixture")).toBe(false);
});

test("a rail slot press still reaches the deck command", async () => {
  const app = await open(HOST, { width: 420, height: 260 });
  await app.waitUntil(() => app.shows("id:deck-slot-deck-fixture-claude"));

  await app.press("id:deck-slot-deck-fixture-claude");
  await app.waitUntil(() => app.texts().includes("deck-fixture:claude"));
});
