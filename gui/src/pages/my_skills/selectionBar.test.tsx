// 批量选择胶囊测试 — selection_bar.rs 对位。纯语义（三态全选/可译集/待更
// 新计数/目标 agent 集）+ 页面 UI（选中出胶囊、五类操作挂点、上开 agent
// 菜单、卸载确认拦在前）。UI 断言从真实快照推导期望（本机有技能→逐卡可
// 见；全新安装→跳过 UI 段），不写真实 $HOME（只读 + 选择/开合/取消；批量
// 动作本身不触发——链接/卸载/更新都会写盘）。
import { expect, open, test, type App } from "nsh/test";
import { MySkills } from "skillstar-gui/native";
import { WINDOW_W, WINDOW_H } from "../../layout";
import type { AgentProfileRecord, SkillRecord } from "./filters";
import {
  descriptionSource,
  selectAllFace,
  targetableAgents,
  translateIdleNames,
  updatesInSelection,
  type TranslationGate,
} from "./selectionBarSlot";

// 卡片刚出现的短暂窗口里，页面的后台更新检查（开页即跑，GPUI 产品行为）
// 完成后的第二次装载会重建列表行，落在这扇窗里的按压会被吞（对卡面主点
// 击同样成立——app 级重建竞态，非选择条逻辑）。首次点卡用有界重按：一次
// 没生效就再点一次（用户视角即「再点一下」），断言仍然严格——最终必须
// 出现预期效果。
async function tapCardSelect(app: App, name: string): Promise<void> {
  const selector = `id:skill-card-check-${name}`;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await app.press(selector);
    try {
      await app.waitUntil(() => app.shows("id:selection-pill"), { timeout: 1200 });
      return;
    } catch {
      // 按压被重建吞掉：再点一次。
    }
  }
  await app.waitUntil(() => app.shows("id:selection-pill"));
}

// ---- 纯语义（selection_bar.rs 的派生半） --------------------------------------

function skill(name: string, description: string): SkillRecord {
  return {
    name,
    description,
    skillType: "hub",
    stars: 0,
    installed: true,
    updateAvailable: false,
    upstreamChange: null,
    lastUpdated: "",
    gitUrl: "",
    agentLinks: [],
  };
}

/** 中英分流的替身翻译门：中文串（含 CJK）不要译，英文串要译；缓存只认
 * 记过的键。 */
function cjkGate(cached: Record<string, string> = {}): TranslationGate {
  return {
    needsTranslation: (source) => !/[\u4e00-\u9fff]/.test(source),
    cachedTranslation: (source) => cached[source] ?? null,
  };
}

test("description source prefers a non-empty localized line (mod.rs description_source)", () => {
  expect(descriptionSource(skill("a", "Build deep modules."))).toBe("Build deep modules.");
  expect(
    descriptionSource({ ...skill("a", "Raw text"), localizedDescription: "  " })
  ).toBe("Raw text");
  expect(
    descriptionSource({
      ...skill("a", "Raw text"),
      localizedDescription: "构建深模块。",
    })
  ).toBe("构建深模块。");
  expect(descriptionSource(skill("a", "   "))).toBe(null);
});

test("a click turns on translation for the selected, untranslated lines only", () => {
  // selection_bar.rs the_bar_translates_the_selected_cards 的语义半：
  const alpha = skill("alpha", "Build deep modules.");
  const beta = skill("beta", "Write tests first."); // 未选中
  const han = skill("han", "为代理构建深模块。"); // 中文文案：目标即中文
  const shown = {
    ...skill("shown", "Already translated."),
    localizedDescription: "已经译过。",
  };
  const off = skill("off", "Show the original.");
  const list = [alpha, beta, han, shown, off];
  const batch = new Set(["alpha", "han", "shown", "off"]);
  const choices: Record<string, boolean> = { shown: true, off: false };

  // shown 已请求且缓存命中（已在显示译文）→ 不在点击集。
  const idle = translateIdleNames(list, batch, choices, false, cjkGate({
    [shown.description]: shown.localizedDescription as string,
  }));
  expect(idle).toEqual(["alpha", "off"]);
  // 未选中的 beta 永不进集。
  expect(idle).not.toContain("beta");
  expect(idle).not.toContain("han");
  expect(idle).not.toContain("shown");
});

test("a global-on surface leaves only cached lines out (translate_batch)", () => {
  const alpha = skill("alpha", "Build deep modules.");
  const cached = skill("cached", "Warm line.");
  const gate = cjkGate({ "Warm line.": "热缓存。" });
  // 全局开关开：已请求的全部要求译文；只有缓存命中的「已在显示」排除。
  expect(
    translateIdleNames([alpha, cached], new Set(["alpha", "cached"]), {}, true, gate)
  ).toEqual(["alpha"]);
  // 无缓存时全部待开。
  expect(translateIdleNames([alpha], new Set(["alpha"]), {}, true, cjkGate())).toEqual([
    "alpha",
  ]);
});

test("the select-all toggle is a tri-state (render_select_all)", () => {
  const a = skill("a", "");
  const b = skill("b", "");
  const c = skill("c", ""); // 过滤集外
  const filtered = [a, b];

  // 空批次 → 未选。
  expect(selectAllFace(filtered, new Set())).toEqual({
    all: false,
    some: false,
    icon: "square",
  });
  // 部分（只选 a）→ some。
  expect(selectAllFace(filtered, new Set(["a"]))).toEqual({
    all: false,
    some: true,
    icon: "squareMinus",
  });
  // 过滤集全选（批次还含集外的 c）→ all。
  expect(selectAllFace(filtered, new Set(["a", "b", "c"]))).toEqual({
    all: true,
    some: false,
    icon: "squareCheck",
  });
  // 过滤集为空永不 all。
  expect(selectAllFace([], new Set(["c"]))).toEqual({
    all: false,
    some: true,
    icon: "squareMinus",
  });
});

test("the update count reads only the selected pending cards (render_update_button)", () => {
  const pending = { ...skill("pending", ""), updateAvailable: true };
  const other = { ...skill("other", ""), updateAvailable: true }; // 未选中
  const quiet = skill("quiet", "");
  expect(updatesInSelection([pending, other, quiet], new Set(["pending", "quiet"]))).toBe(1);
  expect(updatesInSelection([pending], new Set())).toBe(0);
});

test("targetable agents keep the rail's rule: enabled with a global dir", () => {
  const records: AgentProfileRecord[] = [
    { id: "on", displayName: "On", enabled: true, targetable: true },
    { id: "off", displayName: "Off", enabled: false, targetable: false },
    { id: "no-dir", displayName: "No dir", enabled: true, targetable: false },
  ];
  expect(targetableAgents(records).map((r) => r.id)).toEqual(["on"]);
});

// ---- 页面 UI（真实快照推导；不写 $HOME） --------------------------------------

test("selecting cards floats the pill; clear drops it; the five commands ride along", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-check-${first}`));

  // 未选中：浮层不占位。
  expect(app.shows("id:selection-bar")).toBe(false);
  await tapCardSelect(app, first);
  await app.waitUntil(() => app.shows("id:selection-pill"));
  // 计数文案（zh/en 两目录同测）。
  await app.waitUntil(() =>
    app.texts().some((text) => text === "已选 1 个" || text === "1 selected")
  );
  // 五类操作 + 三态全选 + 分隔线两侧的清除全部在胶囊上。
  expect(app.shows("id:select-all-toggle-btn")).toBe(true);
  expect(app.shows("id:batch-link-trigger")).toBe(true);
  expect(app.shows("id:batch-translate-btn")).toBe(true);
  expect(app.shows("id:batch-update-btn")).toBe(true);
  expect(app.shows("id:batch-uninstall-btn")).toBe(true);
  expect(app.shows("id:selection-clear")).toBe(true);

  // 清除：整条浮层退场（selected_batch.clear + link_menu_open=false）。
  await app.press("id:selection-clear");
  await app.waitUntil(() => !app.shows("id:selection-bar"));
});

test("the select-all toggle sweeps the filtered set and sweeps it back out", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  const total = snapshot.skills.length;
  await app.waitUntil(() => app.shows(`id:skill-card-check-${first}`));
  await tapCardSelect(app, first);
  await app.waitUntil(() => app.shows("id:select-all-toggle-btn"));

  // 三态（render_select_all）：部分选 → 按下补齐过滤集；全选态 → 按下清空
  // 整批。单卡库（total=1）选中即全选态，第一次按下就是清空半。
  if (total > 1) {
    await app.press("id:select-all-toggle-btn");
    await app.waitUntil(() =>
      app.texts().some(
        (text) => text === `已选 ${total} 个` || text === `${total} selected`
      )
    );
  }
  // 全选态再点 → 清空整批，浮层退场。
  await app.press("id:select-all-toggle-btn");
  await app.waitUntil(() => !app.shows("id:selection-bar"));
});

test("the agent menu opens upward from the pill and lists only targetable agents", async () => {
  const snapshot = (await MySkills.snapshot()) as {
    skills: { name: string }[];
    agents: { id: string; targetable: boolean }[];
  };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-check-${first}`));
  await tapCardSelect(app, first);
  await app.waitUntil(() => app.shows("id:batch-link-trigger"));

  // 菜单未开。
  expect(app.shows("id:selection-link-menu")).toBe(false);
  await app.press("id:batch-link-trigger");
  await app.waitUntil(() => app.shows("id:selection-link-menu"));
  // 行集 = targetable agents（与卡面轮播同源）；不可目标化的 agent 不上菜单。
  for (const agent of snapshot.agents) {
    if (agent.targetable) {
      await app.waitUntil(() => app.shows(`id:link-to-${agent.id}`));
    } else {
      expect(app.shows(`id:link-to-${agent.id}`)).toBe(false);
    }
  }
  // 取消链接行在菜单内、垫底。
  expect(app.shows("id:batch-unlink-all-btn")).toBe(true);
  // 无可目标 agent 时是空态行而不是列表。
  if (snapshot.agents.every((agent) => !agent.targetable)) {
    expect(app.shows("id:selection-link-empty")).toBe(true);
  }

  // 点外幕收起（GPUI selection-menu-dismiss 对位）。
  await app.press("id:selection-link-dismiss");
  await app.waitUntil(() => !app.shows("id:selection-link-menu"));
  // 清理：批次回空。
  await app.press("id:selection-clear");
  await app.waitUntil(() => !app.shows("id:selection-bar"));
});

test("batch uninstall opens the confirm; cancel keeps the selection (selection_bar.rs)", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-check-${first}`));
  await tapCardSelect(app, first);
  await app.waitUntil(() => app.shows("id:batch-uninstall-btn"));

  // 未确认前没有弹窗。
  expect(app.shows("id:dialog-confirm-card")).toBe(false);
  await app.press("id:batch-uninstall-btn");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"));
  // 取消：弹窗收起、批次原样（GPUI uninstall_opens_a_confirm_… 同款）。
  await app.press("id:dialog-confirm-cancel");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"));
  await app.waitUntil(() => app.shows("id:selection-pill"));
  // 清理。
  await app.press("id:selection-clear");
  await app.waitUntil(() => !app.shows("id:selection-bar"));
});
