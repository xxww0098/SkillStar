// 安装技能详情列测试 — detail_facts.rs 语义逐条 + commands.rs 开关编排 +
// detail_drawer_tests.rs 能无头覆盖的部分（翻译选择面、滚轮隔离外的数据
// 流与交互）。滚轮不外泄由 nsh 原生嵌套滚动承担（chrome/sheet 台账），
// 无头引擎驱动不了滚轮——不在本文件断言。
import { expect, open, test } from "nsh/test";
import { filteredSkills, type SkillRecord } from "./filters";
import {
  batchLinkNotice,
  descriptionSource,
  formatUpdated,
  masterSwitchAgentIds,
  masterSwitchEnable,
  profileIsLinked,
  shownAuthor,
  sourceValues,
  toggleSkipMessage,
  upstreamNoteOf,
  wrapLongToken,
} from "./detailFacts";
import { createDrawerTranslateSession, type DrawerTranslateBridge } from "./drawerTranslate";
import { descriptionChoices } from "./state";

const HOST = "./detailDrawer.test.host.tsx";

function baseSkill(): SkillRecord {
  return {
    name: "gone",
    description: "",
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

// ---- detail_facts.rs 语义 ----------------------------------------------------

test("a repository URL hides the owner/repo it already contains (source_values)", () => {
  const lines = sourceValues(
    "https://github.com/nextlevelbuilder/ui-ux-pro-max.git",
    "nextlevelbuilder/ui-ux-pro-max",
    "ui-ux-pro-max"
  );
  expect(lines).toEqual([
    { kind: "link", value: "https://github.com/nextlevelbuilder/ui-ux-pro-max.git" },
  ]);
});

test("an SSH URL covers its owner/repo and stays plain text", () => {
  const lines = sourceValues(
    "git@github.com:nextlevelbuilder/ui-ux-pro-max.git",
    "nextlevelbuilder/ui-ux-pro-max",
    "ui-ux-pro-max"
  );
  expect(lines).toEqual([
    { kind: "text", value: "git@github.com:nextlevelbuilder/ui-ux-pro-max.git" },
  ]);
});

test("a local source shows when there is no URL; a source equal to the name does not", () => {
  expect(sourceValues("", "local/cursor", "brand")).toEqual([
    { kind: "text", value: "local/cursor" },
  ]);
  expect(sourceValues("", "brand", "brand")).toEqual([]);
});

test("a different source stays beside an unrelated URL", () => {
  const lines = sourceValues("https://gitlab.com/other/repo", "local/cursor", "brand");
  expect(lines).toEqual([
    { kind: "link", value: "https://gitlab.com/other/repo" },
    { kind: "text", value: "local/cursor" },
  ]);
});

test("the author row hides behind the shown source's owner (shown_author)", () => {
  const lines = sourceValues(
    "https://github.com/nextlevelbuilder/ui-ux-pro-max",
    "nextlevelbuilder/ui-ux-pro-max",
    "ui-ux-pro-max"
  );
  expect(shownAuthor("nextlevelbuilder", lines)).toBe(null);
  expect(shownAuthor("someone-else", lines)).toBe("someone-else");
  expect(shownAuthor("@handle", [])).toBe("@handle");
  expect(shownAuthor("   ", [])).toBe(null);
});

test("an unparseable timestamp stays whole; empty is absent (format_updated)", () => {
  expect(formatUpdated("yesterday")).toBe("yesterday");
  expect(formatUpdated("  ")).toBe(null);
  expect(formatUpdated("2026-10-01")).toBe("2026-10-01");
  // RFC3339 带时区：按本地时区排成 YYYY-MM-DD HH:mm（与 chrono Local 同形）。
  const local = formatUpdated("2026-10-01T08:30:00Z");
  expect(local).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
});

test("wrap inserts break points without dropping characters", () => {
  const wrapped = wrapLongToken("https://github.com/a/b");
  expect(wrapped).toContain("\u200b");
  expect(wrapped.replace(/\u200b/g, "")).toBe("https://github.com/a/b");
});

test("upstream_note maps every change kind (removed names the source)", () => {
  const removed: SkillRecord = {
    ...baseSkill(),
    source: "acme/repo",
    upstreamChange: { kind: "removed" },
  };
  expect(upstreamNoteOf(removed)).toEqual({
    kind: "removed",
    source: "acme/repo",
    folder: "gone",
  });
  const renamed: SkillRecord = {
    ...baseSkill(),
    upstreamChange: { kind: "identity_changed", upstreamName: "moved-on" },
  };
  expect(upstreamNoteOf(renamed)).toEqual({ kind: "renamed", from: "gone", to: "moved-on" });
  const edited: SkillRecord = {
    ...baseSkill(),
    upstreamChange: { kind: "local_changes" },
  };
  expect(upstreamNoteOf(edited)).toEqual({ kind: "localEdits", baselineMissing: false });
  expect(upstreamNoteOf(edited, true)!.kind).toBe("localEdits");
  expect((upstreamNoteOf(edited, true) as { baselineMissing: boolean }).baselineMissing).toBe(
    true
  );
});

test("description_source prefers the localized line and drops blank copy", () => {
  expect(descriptionSource({ ...baseSkill(), description: "Keep it." })).toBe("Keep it.");
  expect(
    descriptionSource({ ...baseSkill(), localizedDescription: "保留它。", description: "Keep it." })
  ).toBe("保留它。");
  expect(descriptionSource({ ...baseSkill(), description: "   " })).toBe(null);
});

// ---- commands.rs 开关语义 ----------------------------------------------------

test("the master switch links the missing half, unlinks only when all are on", () => {
  expect(masterSwitchEnable([])).toBe(null);
  const none: [string, boolean][] = [
    ["a", false],
    ["b", false],
  ];
  expect(masterSwitchEnable(none)).toBe(true);
  expect(masterSwitchAgentIds(none, true)).toEqual(["a", "b"]);
  const mixed: [string, boolean][] = [
    ["a", true],
    ["b", false],
  ];
  expect(masterSwitchEnable(mixed)).toBe(true);
  // 已就位的行不动：只补 b。
  expect(masterSwitchAgentIds(mixed, true)).toEqual(["b"]);
  const all: [string, boolean][] = [
    ["a", true],
    ["b", true],
  ];
  expect(masterSwitchEnable(all)).toBe(false);
  expect(masterSwitchAgentIds(all, false)).toEqual(["a", "b"]);
});

test("profile_is_linked matches by id or display name (older records)", () => {
  const links = ["claude", "Code"];
  expect(profileIsLinked({ id: "claude", displayName: "Claude Code" }, links)).toBe(true);
  expect(profileIsLinked({ id: "codex", displayName: "Code" }, links)).toBe(true);
  expect(profileIsLinked({ id: "gemini", displayName: "Gemini" }, links)).toBe(false);
});

test("the batch notice assembles partial lines and the canonical explanation", () => {
  expect(batchLinkNotice({ total: 1, failed: 0, skipped: 0, canonical: 0 })).toBe(null);
  expect(batchLinkNotice({ total: 2, failed: 1, skipped: 1, canonical: 0 })).toBe(
    "有 1/2 个链接未能更新\n已跳过 1/2 个链接：目标名称已被占用，未覆盖原目录"
  );
  expect(batchLinkNotice({ total: 2, failed: 0, skipped: 0, canonical: 2 })).toContain(
    "由规范根直接提供"
  );
  // 全成功但部分 canonical 不再解释（canonical != total）。
  expect(batchLinkNotice({ total: 2, failed: 0, skipped: 0, canonical: 1 })).toBe(null);
});

test("toggle_skip_message explains canonical root and names the unmanaged folder", () => {
  expect(toggleSkipMessage("drawer-demo", { code: "canonical_root_agent", path: "/x" })).toContain(
    "由规范根直接提供"
  );
  expect(
    toggleSkipMessage("drawer-demo", { code: "unmanaged_real_directory", path: "/tmp/occupied" })
  ).toContain("/tmp/occupied");
});

// ---- 翻译会话（drawerTranslate） ----------------------------------------------

function fakeBridge(over: Partial<DrawerTranslateBridge> = {}): DrawerTranslateBridge {
  return {
    descriptionsEnabled: () => false,
    needsTranslation: (text) => !/[\u3400-\u9fff]/.test(text),
    lookup: () => null,
    blocked: () => false,
    setChoices: () => Promise.resolve(),
    fill: () => Promise.resolve({ done: [], rejection: null }),
    ...over,
  };
}

test("the choice outranks the settings switch until it is written", () => {
  const session = createDrawerTranslateSession(fakeBridge({ descriptionsEnabled: () => true }));
  expect(session.resolveTranslate("drawer-demo", {})).toBe(true);
  expect(session.resolveTranslate("drawer-demo", { "drawer-demo": false })).toBe(false);
  expect(session.resolveTranslate("drawer-demo", { other: true })).toBe(true);
});

test("applyChoice persists the flipped table and bumps the generation", () => {
  const written: Record<string, boolean>[] = [];
  const session = createDrawerTranslateSession(
    fakeBridge({ setChoices: (choices) => { written.push(choices); return Promise.resolve(); } })
  );
  const before = session.generation();
  descriptionChoices.set({});
  session.applyChoice("drawer-demo", false);
  expect(written).toEqual([{ "drawer-demo": true }]);
  expect(descriptionChoices()).toEqual({ "drawer-demo": true });
  expect(session.generation()).toBe(before + 1);
  session.applyChoice("drawer-demo", true);
  expect(written[1]).toEqual({ "drawer-demo": false });
});

test("the button gate sweeps only while a needed translation is missing", () => {
  const session = createDrawerTranslateSession(
    fakeBridge({
      lookup: (source) => (source === "cached" ? "缓存" : null),
    })
  );
  expect(session.buttonGate("中文", true).visible).toBe(false);
  const missing = session.buttonGate("English", true);
  expect(missing.visible).toBe(true);
  expect(missing.pending).toBe(true);
  expect(session.buttonGate("cached", true).pending).toBe(false);
  // 原文态不扫（pending 只属于译文态）。
  expect(session.buttonGate("English", false).pending).toBe(false);
});

test("a model rejection pins the failure line until the next Original press", async () => {
  const session = createDrawerTranslateSession(
    fakeBridge({
      fill: () =>
        Promise.resolve({
          done: [],
          rejection: { model: "gpt-x", detail: "too long." },
        }),
    })
  );
  expect(session.failureLine()).toBe(null);
  session.ensureTranslation("English line", true);
  await Promise.resolve();
  await Promise.resolve();
  expect(session.rejected()).toBe(true);
  expect(session.failureLine()).toContain("gpt-x");
  // 从原文再翻：清拒绝（retry_rejected）。
  session.applyChoice("drawer-demo", false);
  expect(session.rejected()).toBe(false);
});

// ---- 抽屉 UI（装置驱动） ------------------------------------------------------

test("the column paints the header, badges and every fact once", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-uninstall-btn"));
  expect(app.shows("id:close-detail-drawer")).toBe(true);
  const texts = app.texts();
  // 名字带零宽断行点（wrap_long_token）：剥除后比对。
  expect(texts.some((text) => text.replace(/\u200b/g, "") === "drawer-demo")).toBe(true);
  expect(texts).toContain("Hub 技能");
  expect(texts).toContain("有可用更新");
  expect(texts).toContain("描述");
  expect(texts).toContain("来源");
  // URL 断行点插在标点后：按段断言（"github.\u200bcom"）。
  expect(texts.some((text) => text.replace(/\u200b/g, "").startsWith("https://github.com/nextlevelbuilder"))).toBe(true);
  // 作者已内含在仓库 URL 里：不出现第二条。
  expect(texts.some((text) => text.includes("@nextlevelbuilder"))).toBe(false);
  expect(texts).toContain("最近更新");
  expect(app.shows("id:drawer-view-skill-md")).toBe(true);
  expect(app.shows("id:drawer-update-btn")).toBe(true);
  expect(app.shows("id:drawer-open-folder-btn")).toBe(true);
  expect(app.shows("id:drawer-uninstall-btn")).toBe(true);
  expect(app.shows("id:skill-detail-scroll")).toBe(true);
});

test("a local creation wears the ok badge and the empty description line", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:host-scene-brand");
  await app.waitUntil(() => app.texts().some((text) => text === "本地创建"));
  expect(app.texts()).toContain("暂无描述。");
  // 无更新项：更新钮不出现。
  expect(app.shows("id:drawer-update-btn")).toBe(false);
});

test("upstream notes explain removal, rename and local edits in full", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:host-scene-gone");
  await app.waitUntil(() => app.texts().some((text) => text === "来源仓库已移除该技能"));
  expect(app.texts().some((text) => text.includes("acme/repo") && text.includes("gone"))).toBe(
    true
  );
  await app.press("id:host-scene-moved");
  await app.waitUntil(() => app.texts().some((text) => text.includes("moved-on")));
  await app.press("id:host-scene-edited");
  await app.waitUntil(() => app.texts().some((text) => text === "本地内容和安装时不一致"));
  expect(app.texts()).toContain("更新会覆盖这些本地修改。");
  // 异步补读的基线缺失落地后换「无法确认」文案。
  await app.press("id:host-baseline-missing");
  await app.waitUntil(() => app.texts().some((text) => text.includes("无法确认")));
  // 本地改动 + 待更新：更新钮带覆盖警示文案。
  expect(app.texts()).toContain("覆盖本地更新");
});

test("the language button translates this opening only, and back", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-translate"));
  expect(app.shows("id:drawer-translation")).toBe(false);
  expect(app.texts()).toContain("Build deep modules for agents.");
  await app.press("id:drawer-translate");
  await app.waitUntil(() => app.shows("id:drawer-translation"));
  expect(app.texts()).toContain("为代理构建深模块。");
  // 选择整表落库（MySkills.setDescriptionChoices 的桥形状）。
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes('"drawer-demo":true'))
  );
  // 原文按钮把同一条选择翻回去。
  await app.press("id:drawer-translate");
  await app.waitUntil(() => !app.shows("id:drawer-translation"));
  expect(app.texts()).toContain("Build deep modules for agents.");
});

test("the language button sweeps a laser until the description lands", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  // 引擎挂起：描述一直未落缓存，按钮停在 icon_sweep 的扫描态。
  await app.press("id:host-hold-fill");
  await app.press("id:host-scene-fresh");
  await app.waitUntil(() => app.shows("id:drawer-translate"));
  // 原文态不扫（首帧就是静止字形，之后翻开关必须跟着扫）。
  expect(app.shows("id:drawer-translate-sweep")).toBe(false);
  await app.press("id:drawer-translate");
  await app.waitFor("id:drawer-translate-sweep");
  // 放行引擎调用：译文落地，扫描停，按钮退回静止字形。
  await app.press("id:host-release-fill");
  await app.waitUntil(() => !app.shows("id:drawer-translate-sweep"));
  expect(app.shows("id:drawer-translation")).toBe(true);
});

test("already-Chinese copy offers no translation", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:host-scene-zh");
  await app.waitUntil(() => app.texts().some((text) => text === "为代理构建深模块。"));
  expect(app.shows("id:drawer-translate")).toBe(false);
});

test("an uncached translation queues one engine fill", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:host-scene-fresh");
  await app.waitUntil(() => app.shows("id:drawer-translate"));
  await app.press("id:drawer-translate");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("fill:Fresh uncached line."))
  );
});

test("one switch per targetable agent; the disabled profile stays off", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-agent-row-drawer-demo-claude"));
  expect(app.shows("id:drawer-agent-row-drawer-demo-codex")).toBe(true);
  expect(app.shows("id:drawer-agent-row-drawer-demo-gemini")).toBe(false);
  expect(app.shows("id:drawer-toggle-all-drawer-demo")).toBe(true);
  expect(app.texts()).toContain("智能体部署");
  expect(app.texts()).toContain("全部");
});

test("no targetable agents explains how to enable one", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:host-no-agents");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("没有已启用的智能体"))
  );
  expect(app.shows("id:drawer-toggle-all-drawer-demo")).toBe(false);
});

test("a row switch flips optimistically and restores on a skip", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-drawer-demo-codex"));
  await app.press("id:host-toggle-skip");
  // 闸门保持域调用：乐观翻链的中间态可断言（微任务内一闪而过的对策）。
  await app.press("id:host-hold-master");
  await app.press("id:drawer-toggle-drawer-demo-codex");
  await app.waitUntil(() => app.texts().some((text) => text === "Claude Code,Code"));
  await app.press("id:host-release");
  // skip（规范根）回滚 + 说明性通知带。
  await app.waitUntil(() => app.texts().some((text) => text === "Claude Code"));
  await app.waitUntil(() => app.texts().some((text) => text.includes("由规范根直接提供")));
});

test("a failed row switch restores and surfaces the domain chain", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-drawer-demo-codex"));
  await app.press("id:host-toggle-fail");
  await app.press("id:drawer-toggle-drawer-demo-codex");
  await app.waitUntil(() => app.texts().some((text) => text.includes("失败") && text.includes("boom")));
  await app.waitUntil(() => app.texts().some((text) => text === "Claude Code"));
});

test("the master switch only moves the rows that are not there yet", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-all-drawer-demo"));
  await app.press("id:host-reset-calls");
  await app.press("id:drawer-toggle-all-drawer-demo");
  // claude 已链接：只补 codex（master_switch_agent_ids）。
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("master:drawer-demo:codex:true"))
  );
  await app.waitUntil(() => app.texts().some((text) => text === "Claude Code,Code"));
  // 再按：全已链接 → 全取消。
  await app.press("id:host-reset-calls");
  await app.press("id:drawer-toggle-all-drawer-demo");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("master:drawer-demo:claude+codex:false"))
  );
  await app.waitUntil(() => app.texts().some((text) => text === "-"));
});

test("a partial batch report becomes one warning notice", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-all-drawer-demo"));
  await app.press("id:host-master-partial");
  await app.press("id:drawer-toggle-all-drawer-demo");
  await app.waitUntil(() => app.texts().some((text) => text.includes("有 1/2 个链接未能更新")));
  await app.waitUntil(() => app.texts().some((text) => text.includes("已跳过 1/2 个链接")));
});

test("an all-canonical batch explains itself instead of failing", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-all-drawer-demo"));
  await app.press("id:host-master-canonical");
  await app.press("id:drawer-toggle-all-drawer-demo");
  await app.waitUntil(() => app.texts().some((text) => text.includes("由规范根直接提供")));
});

test("a held master job disables the switch until it settles", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-toggle-all-drawer-demo"));
  await app.press("id:host-hold-master");
  await app.press("id:drawer-toggle-all-drawer-demo");
  expect(app.enabled("id:drawer-toggle-all-drawer-demo")).toBe(false);
  await app.press("id:host-release");
  await app.waitUntil(() => app.enabled("id:drawer-toggle-all-drawer-demo"));
  // 命令尾部的 refresh（ops 注入的假件）被记录。
  await app.waitUntil(() => app.texts().some((text) => text.includes("refresh")));
});

test("the update button follows the busy token and the overwrites hint", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.texts().some((text) => text === "更新"));
  await app.press("id:host-reset-calls");
  await app.press("id:drawer-update-btn");
  await app.waitUntil(() => app.texts().some((text) => text.includes("update:drawer-demo")));
  // busy=技能名：label 转「更新中」且按钮禁用（不重复提交）。
  await app.press("id:host-busy-demo");
  await app.waitUntil(() => app.texts().some((text) => text === "更新中..."));
  await app.waitUntil(() => app.enabled("id:drawer-update-btn") === false);
  await app.press("id:host-busy-demo");
});

test("view SKILL.md and open folder route through the page entries", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-view-skill-md"));
  await app.press("id:host-reset-calls");
  await app.press("id:drawer-view-skill-md");
  await app.press("id:drawer-open-folder-btn");
  await app.waitUntil(() =>
    app.texts().some(
      (text) => text.includes("reader:drawer-demo") && text.includes("folder:drawer-demo")
    )
  );
});

test("uninstall confirms by name, closes the drawer, then runs", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:drawer-uninstall-btn"));
  await app.press("id:drawer-uninstall-btn");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"));
  expect(app.texts()).toContain("卸载「drawer-demo」？");
  await app.press("id:dialog-confirm-ok");
  // 先关抽屉（closed 计数），再跑卸载（busy 尾巴清干净 + refresh）。
  await app.waitUntil(() => app.texts().some((text) => text === "closed:1"));
  await app.waitUntil(() => app.texts().some((text) => text.includes("uninstall:drawer-demo")));
  await app.waitUntil(() => app.texts().some((text) => text.includes("refresh")));
});

test("the close control clears the selection", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:close-detail-drawer"));
  await app.press("id:close-detail-drawer");
  await app.waitUntil(() => app.texts().some((text) => text === "closed:1"));
  // 选择清空后装置派生不出卡：空分支占位。
  await app.waitUntil(() => app.shows("id:my-skills-detail-empty"));
});

test("filteredSkills matches the original called name case-insensitively", () => {
  const skills: SkillRecord[] = [
    { ...baseSkill(), name: "demo-owner", called: "demo", source: "owner/repo" },
    { ...baseSkill(), name: "other-skill", source: "other/repo" },
  ];
  const matched = filteredSkills(skills, [], {
    query: "DEMO",
    sourceFilter: "all",
    repoFilter: null,
    agentFilterId: null,
    onlyUpdates: false,
    sortBy: "name",
  });
  expect(matched.map((s) => s.name)).toEqual(["demo-owner"]);
});

test("coexisting skill shows from-called badge, modified copy recovery actions, and beside skills", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.waitUntil(() => app.shows("id:host-scene-coexist"));
  await app.press("id:host-reset-calls");
  await app.press("id:host-scene-coexist");
  await app.waitUntil(() => app.shows("id:drawer-called-badge-coexist"));
  await app.waitUntil(() => app.shows("id:drawer-modified-copies-card"));
  expect(app.shows("id:drawer-beside-item-sib-one")).toBe(true);

  await app.press("id:drawer-adopt-copy-claude");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("adoptCopy:coexist:claude")),
  );

  await app.press("id:drawer-overwrite-copy-claude");
  await app.waitUntil(() =>
    app.texts().some((text) => text.includes("overwriteCopy:coexist:claude")),
  );
});
