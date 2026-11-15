// createGroup（skill_cards/create_group.rs 对位）的无头测试：纯逻辑直测
// （createGroup.logic.ts，样板 noticeBoard.ts）+ 装置入口上的 UI 交互断言
// （open + press/type/scroll + texts）。真实域写入由 native crate 的
// cargo test（facades/skill_cards_create_group.rs，隔离数据目录）覆盖。
import { expect, open, test } from "nsh/test";
import {
  EMOJI_OPTIONS,
  PICKER_VIEW_H,
  PICKER_VISIBLE,
  PILLS_VIEW_H,
  PILLS_VISIBLE,
  allMarked,
  canCreate,
  commitMembers,
  filterRows,
  isDuplicateName,
  sortedPillMembers,
  toggleAll,
  toggleMember,
  type DeckSkillRow,
} from "./createGroup.logic";

const HOST = "./createGroup.test.host.tsx";
// 桌面视口：对话框卡 512px 宽，默认 390pt 的手机视口会把它裁出画面。
const VIEWPORT = { width: 1200, height: 800 } as const;

const ROWS: DeckSkillRow[] = [
  { name: "alpha", description: "writing helper" },
  { name: "beta", description: "" },
  { name: "code-review", description: "Doc tools" },
];

// ── 纯逻辑（对位 create_group.rs 的 render 内计算）──────────────────────

test("the picker scroll-box constants stay at the GPUI contract", () => {
  // create_group.rs:31-40 —— 超过阈值才滚动，滚动用固定像素视口。
  expect(PICKER_VISIBLE).toBe(5);
  expect(PICKER_VIEW_H).toBe(160);
  expect(PILLS_VISIBLE).toBe(20);
  expect(PILLS_VIEW_H).toBe(140);
  expect(EMOJI_OPTIONS[0]).toBe("💻");
  expect(EMOJI_OPTIONS).toHaveLength(16);
});

test("filtering matches name or description, case-insensitively", () => {
  expect(filterRows(ROWS, "")).toHaveLength(3);
  expect(filterRows(ROWS, "alp")).toEqual([ROWS[0]]);
  // 描述大小写不敏感（create_group.rs:288-298 的 to_lowercase 双侧）。
  expect(filterRows(ROWS, "DOC")).toEqual([ROWS[2]]);
  expect(filterRows(ROWS, "  ")).toHaveLength(3);
  expect(filterRows(ROWS, "zzz")).toEqual([]);
});

test("the duplicate guard only fires on a non-empty taken name", () => {
  expect(isDuplicateName("taken", ["taken"])).toBe(true);
  expect(isDuplicateName("demo", ["taken"])).toBe(false);
  expect(isDuplicateName("", ["taken"])).toBe(false);
  expect(isDuplicateName("  ", ["taken"])).toBe(false);
});

test("create stays disabled while busy, unnamed, memberless or duplicated", () => {
  // create_group.rs:287。
  expect(canCreate("demo", ["alpha"], false, false)).toBe(true);
  expect(canCreate("demo", ["alpha"], false, true)).toBe(false);
  expect(canCreate("  ", ["alpha"], false, false)).toBe(false);
  expect(canCreate("demo", [], false, false)).toBe(false);
  expect(canCreate("taken", ["alpha"], true, false)).toBe(false);
});

test("toggle-all only touches the filtered scope", () => {
  const filtered = filterRows(ROWS, "a"); // alpha + beta
  expect(allMarked(filtered, [])).toBe(false);
  expect(allMarked([], ["alpha"])).toBe(false); // 筛选结果为空不算全选
  // 全选：加入筛出的、保留未筛出的。
  expect(toggleAll(filtered, ["code-review"], false)).toEqual(["code-review", "alpha", "beta"]);
  // 取消全选：只移除筛出的。
  expect(toggleAll(filtered, ["alpha", "beta", "code-review"], true)).toEqual(["code-review"]);
  expect(allMarked(filtered, ["alpha", "beta", "code-review"])).toBe(true);
});

test("member toggle keeps the set semantics with stable order", () => {
  expect(toggleMember(["alpha"], "beta")).toEqual(["alpha", "beta"]);
  expect(toggleMember(["alpha", "beta"], "alpha")).toEqual(["beta"]);
});

test("pills sort installed members first and orphans last", () => {
  // create_group.rs:386-387 —— 排序稳定，同类内保持成员序。
  const installed = new Set(["alpha", "code-review"]);
  expect(sortedPillMembers(["missing", "alpha", "ghost", "code-review"], installed)).toEqual([
    "alpha",
    "code-review",
    "missing",
    "ghost",
  ]);
});

test("commit members are name-sorted", () => {
  // create_group.rs:175-176 的 members.sort()。
  expect(commitMembers(["beta", "alpha", "code-review"])).toEqual([
    "alpha",
    "beta",
    "code-review",
  ]);
});

// ── UI 交互（装置：createGroup.test.host.tsx）───────────────────────────

test("the new-deck dialog shows the picker and blocks creating empty decks", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  expect(app.texts()).toContain("新建卡组");
  // 后台加载的 12 个候选进入列表；虚拟化视口先显示靠前的行。
  await app.waitFor("text:skill-number-00");
  expect(app.texts()).toContain("创建");
  // 空名/空成员：Create 禁用（ERR_NSH_TEST_DISABLED），未提交。
  await expect(app.press("id:pack-create")).rejects.toMatchObject({
    code: "ERR_NSH_TEST_DISABLED",
  });
  expect(app.texts()).toContain("created:0");
});

test("naming, checking a member and creating commits through the bridge", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-05");
  await app.type("id:pack-name", "demo");
  // 勾选后 pill 出现、创建可用。
  await app.press("id:pack-row-skill-number-05");
  await app.press("id:pack-create");
  await app.waitFor("text:created:1");
  // 提交参数：id=null（新建）、名、空描述、默认 emoji、字典序成员。
  expect(app.texts().join(" ")).toContain("new|demo||💻|skill-number-05");
  // 成功后对话框收起（标题消失）。
  expect(app.texts()).not.toContain("新建卡组");
});

test("a duplicate name warns inline and refuses to create", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-00");
  await app.type("id:pack-name", "taken");
  await app.waitFor("text:名称已存在");
  await app.press("id:pack-row-skill-number-00");
  await expect(app.press("id:pack-create")).rejects.toMatchObject({
    code: "ERR_NSH_TEST_DISABLED",
  });
  expect(app.texts()).not.toContain("created:1");
});

test("return in the name field submits the deck", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-01");
  await app.type("id:pack-name", "by-enter");
  await app.press("id:pack-row-skill-number-01");
  await app.submit("id:pack-name");
  await app.waitFor("text:created:1");
  expect(app.texts().join(" ")).toContain("new|by-enter||💻|skill-number-01");
});

test("the filter narrows the picker and reports nothing found", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-00");
  await app.type("id:pack-filter", "07");
  await app.waitFor("text:skill-number-07");
  expect(app.texts()).not.toContain("skill-number-00");
  await app.type("id:pack-filter", "no-such-skill");
  await app.waitFor("text:未找到技能");
});

test("select-all marks the filtered scope, then clears it", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-few");
  await app.waitFor("text:alpha");
  await app.press("id:pack-select-all");
  // 三枚 pill 都在条上。
  await app.waitFor("text:gamma");
  expect(app.texts().join(" ")).toContain("取消全选");
  await app.press("id:pack-select-all");
  // 取消后回到「全选」文案（pill 条整体消失）。
  expect(app.texts()).not.toContain("取消全选");
  expect(app.texts()).toContain("created:0");
});

test("a picked pill can be removed from the strip", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-seeded");
  // Quick Pack 预选成员：pill 已在条上、名称预填 orca。
  await app.waitFor("text:orca");
  await app.waitFor("id:pack-pill-skill-number-00");
  await app.press("id:pack-pill-skill-number-00");
  // 唯一成员被移除后创建回到禁用（成员为空）。
  await expect(app.press("id:pack-create")).rejects.toMatchObject({
    code: "ERR_NSH_TEST_DISABLED",
  });
});

test("the emoji well picks an icon for the deck", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-seeded");
  await app.waitFor("text:orca");
  await app.press("id:pack-icon");
  await app.press("id:pack-emoji-1"); // 🚀
  // 网格收起、井里的图标换成 🚀；提交带上它。
  await app.press("id:pack-row-skill-number-01");
  await app.press("id:pack-create");
  await app.waitFor("text:created:1");
  expect(app.texts().join(" ")).toContain("new|orca||🚀|skill-number-00,skill-number-01");
});

test("editing prefills the form and keeps the orphan member removable", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-edit");
  await app.waitFor("text:编辑卡组");
  // 预填：名称 demo、描述、成员 pill（missing 未安装 → orphan 保留）。
  await app.waitFor("text:demo");
  await app.waitFor("text:old description");
  await app.waitFor("id:pack-pill-missing");
  // 按钮文案是「保存」，提交走编辑 id。
  expect(app.texts()).toContain("保存");
  await app.press("id:pack-row-skill-number-01");
  await app.press("id:pack-create");
  await app.waitFor("text:created:1");
  expect(app.texts().join(" ")).toContain(
    "g1|demo|old description|🚀|missing,skill-number-00,skill-number-01"
  );
});

test("a failed save keeps the input for retry", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-fail-next");
  await app.press("host-open-edit");
  await app.waitFor("text:编辑卡组");
  await app.press("id:pack-create");
  // 域错误显示在表单里（对位 save_failure_keeps_the_input_for_retry）。
  await app.waitFor("id:pack-error");
  expect(app.texts().join(" ")).toContain("already exists");
  // 输入保留：名称还在、重试成功。
  await app.waitFor("text:demo");
  await app.press("id:pack-create");
  await app.waitFor("text:created:1");
});

test("an empty hub shows the no-skills hint instead of a picker", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-empty");
  await app.waitFor("text:还没有安装技能");
  expect(app.texts()).not.toContain("全选");
});

test("a long member list scrolls inside the fixed-height virtualized box", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("id:pack-picker");
  // 虚拟化：12 行只渲染视口附近的，最后一行不在 texts 里。
  expect(app.texts()).not.toContain("skill-number-11");
  await app.scroll("id:pack-picker", 240);
  await app.waitFor("text:skill-number-11");
});

test("a short member list renders at its natural height without a scroll box", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-few");
  await app.waitFor("text:gamma");
  expect(app.texts()).not.toContain("skill-number-00");
});

test("a successful create publishes groupsChanged for the deck page", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-02");
  await app.type("id:pack-name", "events");
  await app.press("id:pack-row-skill-number-02");
  await app.press("id:pack-create");
  // Quick Pack 的 GroupsChanged 对位：页面直开形态成功后发布一次。
  await app.waitFor("text:groups:1");
});

test("a veil press while submitting does not dismiss the deck editor", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-hold-commit");
  await app.press("host-open-new");
  await app.waitFor("text:skill-number-04");
  await app.type("id:pack-name", "slow");
  await app.press("id:pack-row-skill-number-04");
  await app.press("id:pack-create");
  // 提交挂起（busy）：遮罩点击被拦（overlay_closable(!busy)）。
  await app.press("id:group-editor-veil");
  await app.waitFor("text:新建卡组");
  // 放行后提交完成、对话框自动关闭。
  await app.press("host-release-commit");
  await app.waitFor("text:created:1");
  expect(app.texts()).not.toContain("新建卡组");
});

test("cancel closes the dialog without committing", async () => {
  const app = await open(HOST, VIEWPORT);
  await app.press("host-open-new");
  await app.waitFor("text:新建卡组");
  await app.type("id:pack-name", "nope");
  await app.press("id:pack-row-skill-number-03");
  await app.press("id:pack-cancel");
  expect(app.texts()).not.toContain("新建卡组");
  expect(app.texts()).not.toContain("created:1");
});
