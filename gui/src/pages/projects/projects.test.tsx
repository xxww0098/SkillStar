// 项目页测试 — crates/ss-gpui/src/projects/ 的 mod.rs（选中/胶囊不折叠/
// 注册弹窗）、detail.rs（保存并同步使能）、actions.rs + upkeep.rs +
// recovery_tests.rs（copy 看护区恢复）对位。分两半：纯语义（state 的
// needsDeploy/canApply/copyStateKey，直接单测）与页面 UI（假桥装置
// projects.test.host.tsx——真实恢复路径的文件系统断言由 native crate 的
// cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖，见 facades/projects.rs tests）。
//
// 验收对应：左列搜索+右侧三卡结构、agent 行展开勾选与模式胶囊、dirty
// 控制「保存并同步」、copy 看护区恢复。

import { expect, open, test } from "nsh/test";
import { realProjectsBridge } from "./bridge";
import { copyStateKey, createProjectsState } from "./state";
import type { CopyStateTag, ProjectsBridge } from "./bridge";

const HOST = "./projects.test.host.tsx";

// 产品默认 locale zh-CN（services/locale.ts），断言用中文文案。

async function openHost() {
  return open(HOST, { width: 1280, height: 900 });
}

function readout(app: { texts: () => string[] }): string {
  return app.texts().find((text) => text.startsWith("save:")) ?? "";
}

// ---- 纯语义 ---------------------------------------------------------------------

test("copy state tags map onto the copyState* copy keys (upkeep.rs state_key)", () => {
  expect(copyStateKey("current")).toBe("projects.copyStateCurrent");
  expect(copyStateKey("project_missing")).toBe("projects.copyStateProjectMissing");
  expect(copyStateKey("broken_link")).toBe("projects.copyStateBrokenLink");
  expect(copyStateKey("source_missing")).toBe("projects.copyStateSourceMissing");
});

test("needs_deploy counts only missing and broken_link; can_apply adds dirty (actions.rs)", async () => {
  const bridge = memoryBridge();
  const state = createProjectsState(bridge);
  // 直接驱动信号层（不经 UI）：canApply 的两扇门。
  expect(state.canApply()).toBe(false); // 既不 dirty 也无待部署
  state.dirty.set(true);
  expect(state.canApply()).toBe(true); // 编辑即需保存
  state.dirty.set(false);
  feedRecords(state, ["current", "stale", "modified"]);
  expect(state.needsDeploy()).toBe(false); // stale/modified 不算部署缺口
  expect(state.canApply()).toBe(false);
  feedRecords(state, ["current", "missing"]);
  expect(state.needsDeploy()).toBe(true); // 副本缺失：无需编辑也可保存
  feedRecords(state, ["current", "broken_link"]);
  expect(state.needsDeploy()).toBe(true); // 失效链接：同上
  feedRecords(state, []);
  expect(state.canApply()).toBe(false);
});

function feedRecords(state: ReturnType<typeof createProjectsState>, tags: CopyStateTag[]) {
  // 经 refreshCopyStatus 的桥回填？不必——直接写信号（测试专用通道）。
  state.copyRecords.set(
    tags.map((state_, index) => ({
      projectName: "alpha",
      relativePath: ".agents/skills",
      skillName: `demo-${index}`,
      state: state_,
      detail: null,
    }))
  );
}

/** 只回 registry 的最小假桥（纯语义用例不触发看护动作）。 */
function memoryBridge(): ProjectsBridge {
  return {
    async registry() {
      return { projects: [], profiles: [], hubSkills: [] };
    },
    async skillsList() {
      return { agents: {}, deployModes: {} };
    },
    async copyStatus() {
      return {
        project: { name: "alpha", path: "/tmp/alpha", createdAt: "" },
        agents: {},
        deployModes: {},
        copies: [],
        rules: [],
      };
    },
    async saveAndSync() {
      return { count: 0 };
    },
    async retryCopies() {
      return [];
    },
    async forgetCopy() {
      return null;
    },
    async register(path) {
      return { name: path, path, createdAt: "" };
    },
    async updatePath() {
      return null;
    },
    async remove() {
      return null;
    },
  };
}

// ---- 真桥形状（只读） -------------------------------------------------------------

test("the real registry snapshot carries projects, profiles and hub skills", async () => {
  const snapshot = await realProjectsBridge.registry();
  expect(Array.isArray(snapshot.projects)).toBe(true);
  // 内建 roster 恒在（registry specs：built-in 先于 prefs customs）。
  expect(snapshot.profiles.length).toBeGreaterThan(0);
  for (const profile of snapshot.profiles) {
    expect(typeof profile.id).toBe("string");
    expect(typeof profile.projectSkillsRel).toBe("string");
    expect(typeof profile.hasProjectSkills).toBe("boolean");
  }
  expect(Array.isArray(snapshot.hubSkills)).toBe(true);
});

// ---- 页面结构（mod.rs + detail.rs 硬契约） --------------------------------------

test("the page mounts the rail, the three cards in order and the footer", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:proj-card-alpha"));
  // 左列 rail + 搜索 + 计数 chip + 注册按钮。
  expect(app.shows("id:projects-search-input")).toBe(true);
  expect(app.shows("id:projects-register-btn")).toBe(true);
  // 右侧三卡 + 看护区 + 页脚（看护区在最上，三卡顺序固定）。
  expect(app.shows("id:project-copy-status")).toBe(true);
  expect(app.shows("id:agents-card")).toBe(true);
  expect(app.shows("id:projects-deploy-mode-card")).toBe(true);
  expect(app.shows("id:projects-rules-card")).toBe(true);
  expect(app.shows("id:proj-save-sync")).toBe(true);
  // 部署方式卡是静态图例：软链接/复制两行 + 各自提示。
  expect(app.texts()).toContain("软链接");
  expect(app.texts()).toContain("复制");
  // 检测规则卡：seed 项目的 CLAUDE.md 芯片。
  expect(app.texts()).toContain("CLAUDE.md");
  // agent 行：仅启用且支持项目技能的 profile 上卡（global-only 不在）。
  expect(app.shows("id:agent-top-codex")).toBe(true);
  expect(app.shows("id:agent-top-cursor")).toBe(true);
  expect(app.shows("id:agent-top-global-only")).toBe(false);
});

test("searching narrows the rail to the query predicate (mod.rs)", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:proj-card-alpha"));
  await app.type("id:projects-search-input", "zz-no-such-zz");
  await app.waitUntil(() => app.shows("id:projects-rail"));
  expect(app.texts()).toContain("没有匹配的项目");
  await app.clear("id:projects-search-input");
  await app.waitUntil(() => app.shows("id:proj-card-alpha"));
});

// ---- dirty 与「保存并同步」的使能（detail.rs Section 4） ------------------------

test("save & sync stays inert until edits or a deploy gap, then saves the working state", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:proj-card-alpha"));
  await app.waitUntil(() => !app.shows("id:projects-unsaved-badge"));

  // 干净状态：无编辑、无部署缺口 → 按钮描边不可点（静默描边硬契约）。
  expect(app.shows("id:projects-unsaved-badge")).toBe(false);
  expect(app.enabled("id:proj-save-sync")).toBe(false);
  expect(readout(app)).toContain("save:0");

  // 打开 codex 行（默认已启用并展开）— 直接翻 cursor 开关制造 dirty。
  await app.press("id:switch-cursor");
  await app.waitUntil(() => app.shows("id:projects-unsaved-badge"));
  expect(app.shows("id:agent-detail-cursor")).toBe(true); // 开启即展开
  expect(app.shows("id:mode-toggle-cursor")).toBe(true);

  // dirty 下看护动作与弹窗全部静默拒绝（dialogs.rs mutation_blocked）。
  expect(app.enabled("id:project-copy-retry")).toBe(false);

  // 保存：busy 期间按钮吞点击；成功后 dirty 清零、页脚给部署结论。
  await app.press("id:proj-save-sync");
  await app.waitUntil(() => readout(app).includes("save:1"));
  await app.waitUntil(() => !app.shows("id:projects-unsaved-badge"));
  expect(
    app.texts().some((text) => text.includes("已同步") && text.includes("个技能部署到项目"))
  ).toBe(true);
  // 工作态落库：cursor 开关留下空清单（mod.rs toggle_agent 的 Vec::new()）。
  expect(readout(app)).toContain('{"codex":["demo"],"cursor":[]}');
  expect(readout(app)).toContain("missingKeys:0");
});

test("a deploy gap alone enables save: retry will not restore a deleted copy", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.texts().some((text) => text.includes("与 Hub 一致")));

  // 删掉副本再「检查状态」：记录 missing，但不产生编辑（recovery_tests
  // deleted_copy… 的时序：变更先于检查发生）。
  await app.press("id:host-delete-copy");
  await app.press("id:project-copy-check");
  await app.waitUntil(() => app.texts().some((text) => text.includes("副本已不存在")));
  await app.waitUntil(() => app.shows("id:project-copy-forget-agents-skills-demo"));
  expect(app.shows("id:projects-unsaved-badge")).toBe(false);

  // 重试不恢复缺失副本（refresh_stale_copies 只按 copy 重建）。
  await app.press("id:project-copy-retry");
  await app.waitUntil(() => readout(app).includes("retry:1"));
  await app.waitUntil(() => readout(app).includes("target:gone"));
  expect(app.texts().some((text) => text.includes("副本已不存在"))).toBe(true);

  // needs_deploy 把保存点亮：显式保存才重建（缺失即部署缺口）。enabled()
  // 探测可按性（探测性按压会直接触发保存，不能用于断言）。
  expect(app.enabled("id:proj-save-sync")).toBe(true);
  await app.press("id:proj-save-sync");
  await app.waitUntil(() => readout(app).includes("save:1"));
  await app.waitUntil(() => readout(app).includes("target:v1"));
  await app.waitUntil(() => app.texts().some((text) => text.includes("与 Hub 一致")));
});

// ---- copy 看护区恢复（upkeep.rs + recovery_tests.rs） ---------------------------

test("a stale copy retries to the hub version (recovery_tests retry_updates…)", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.texts().some((text) => text.includes("与 Hub 一致")));

  // Hub 升级再「检查状态」：记录转 stale（「待同步」），计数行走待处理。
  await app.press("id:host-hub-v2");
  await app.press("id:project-copy-check");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("待同步到 Hub 当前版本"))
  );
  await app.waitUntil(() => app.texts().some((text) => text.includes("1 条待处理")));

  // 重试：stale 按当前 Hub 重建 → 回到 current，结论行「已与 Hub 一致」。
  await app.press("id:project-copy-retry");
  await app.waitUntil(() => readout(app).includes("retry:1"));
  await app.waitUntil(() => readout(app).includes("target:v2"));
  await app.waitUntil(() => app.texts().some((text) => text.includes("项目副本已与 Hub 一致")));
  expect(app.texts().some((text) => text.includes("待同步"))).toBe(false);
});

test("a missing root reports the project, then metadata-only cleanup clears the record", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.texts().some((text) => text.includes("与 Hub 一致")));

  // 项目根迁移再「检查状态」：记录转 project_missing，页面画「找不到项目」。
  await app.press("id:host-move-project");
  await app.press("id:project-copy-check");
  await app.waitUntil(() => app.texts().some((text) => text.includes("项目路径不存在")));
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("找不到项目") && text.includes("/tmp/proj/alpha"))
  );

  // 重试失败：错误行带项目名与路径（recovery_tests 断言两个 contains）。
  await app.press("id:project-copy-retry");
  await app.waitUntil(() => readout(app).includes("retry:1"));
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("alpha") && text.includes("/tmp/proj/alpha"))
  );

  // 清理记录：确认卡（破坏性文案写出后果）→ 记录消失、清单清空。
  await app.press("id:project-copy-forget-agents-skills-demo");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"));
  expect(app.texts().some((text) => text.includes("项目文件和 Hub 技能会保留"))).toBe(true);
  await app.press("id:dialog-confirm-ok");
  await app.waitUntil(() => readout(app).includes("forget:1"));
  await app.waitUntil(() => app.texts().some((text) => text.includes("0 条部署记录")));
  expect(app.texts().some((text) => text.includes("副本已不存在"))).toBe(false);
});

// ---- agent 行交互（agent_item.rs + mod.rs 内嵌测试） ---------------------------

test("clicking the deploy-mode capsule flips the mode without collapsing the row", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:agent-top-codex"));
  await app.press("id:agent-top-codex"); // 收起（激活时默认展开）
  await app.waitUntil(() => !app.shows("id:agent-detail-codex"));
  await app.press("id:agent-top-codex"); // 重新展开
  await app.waitUntil(() => app.shows("id:agent-detail-codex"));

  // 胶囊在行内点击：翻 copy 模式、行保持展开（mod.rs 回归测试）。
  await app.press("id:mode-toggle-codex");
  await app.waitUntil(() => app.shows("id:projects-unsaved-badge"));
  expect(app.shows("id:agent-detail-codex")).toBe(true);
  // 胶囊换字：复制（warn 色）且 dirty 起来。
  await app.press("id:mode-toggle-codex");
  expect(app.shows("id:mode-toggle-codex")).toBe(true);
});

test("toggling an agent off collapses its detail; adding skills dirties the page", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:agent-detail-codex"));
  // 勾选的行内技能 chip 与 Hub 添加区。
  expect(app.shows("id:rem-btn-codex-demo")).toBe(true);

  // 移除已分配技能 → dirty。
  await app.press("id:rem-btn-codex-demo");
  await app.waitUntil(() => app.shows("id:projects-unsaved-badge"));
  // Hub 添加区出现（demo 不再分配）→ 再加回。
  await app.waitUntil(() => app.shows("id:add-chip-codex-demo"));
  await app.press("id:add-chip-codex-demo");
  await app.waitUntil(() => !app.shows("id:add-chip-codex-demo"));

  // 关掉 codex：行收起、技能计数胶囊消失。
  await app.press("id:switch-codex");
  await app.waitUntil(() => !app.shows("id:agent-detail-codex"));
  await app.press("id:switch-codex"); // 重新打开：开启即展开
  await app.waitUntil(() => app.shows("id:agent-detail-codex"));
});

// ---- 注册弹窗（dialogs.rs + dialogs_tests.rs） ---------------------------------

test("an empty commit keeps the register dialog open with the reason", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:projects-register-btn"));
  await app.press("id:projects-register-btn");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"));
  // 空路径提交：理由行出现、弹窗保留（dialogs_tests 同名用例）。
  await app.press("id:dialog-confirm-ok");
  await app.waitUntil(() => app.shows("id:projects-register-error"));
  expect(app.shows("id:dialog-confirm-card")).toBe(true);
  expect(readout(app)).toContain("reg:0");
  await app.press("id:dialog-confirm-cancel");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"));
});

test("a successful registration closes the dialog and selects the new project", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:projects-register-btn"));
  await app.press("id:projects-register-btn");
  await app.waitUntil(() => app.shows("id:projects-path-input"));
  await app.type("id:projects-path-input", "/tmp/proj/beta");
  await app.press("id:dialog-confirm-ok");
  // 成功经 token 关卡：弹窗关闭、rail 出现新卡并选中。
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"));
  await app.waitUntil(() => app.shows("id:proj-card-beta"));
  expect(readout(app)).toContain("reg:1");
  // beta 是干净项目：选中后无未保存徽章、无部署记录。
  await app.waitUntil(() => app.texts().some((text) => text.includes("0 条部署记录")));
  expect(app.shows("id:projects-unsaved-badge")).toBe(false);
});

test("a failed registration keeps the dialog with the domain reason", async () => {
  const app = await openHost();
  await app.waitUntil(() => app.shows("id:projects-register-btn"));
  await app.press("id:projects-register-btn");
  await app.waitUntil(() => app.shows("id:projects-path-input"));
  // alpha 的路径已注册：域侧拒绝（already registered）。
  await app.type("id:projects-path-input", "/tmp/proj/alpha");
  await app.press("id:dialog-confirm-ok");
  await app.waitUntil(() => app.shows("id:projects-register-error"));
  expect(app.shows("id:dialog-confirm-card")).toBe(true);
  expect(readout(app)).toContain("reg:1");
  expect(app.texts().some((text) => text.includes("already registered"))).toBe(true);
});
