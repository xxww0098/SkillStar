// 市场详情列测试 — mod.rs tests + translate.rs tests + column 的无头对位。
//
// 纯状态机部分（createDetailReader / createTranslateSession）直接建实例、
// 可控 Promise 断言数据流：epoch 丢弃迟到响应（验收红线）、stale/miss
// 只回填刷新一次、空体 remote_error 降级失败、翻译选择只翻当前打开并记
// 住、模型拒绝走通知带。组件部分经 detailDrawer.test.host.tsx 的装置按
// 钮驱动（skill_cards/detailDrawer.test.tsx 样板）；真实域读写（写选择
// 落盘、fill 网络行为）由 native crate 的 cargo test 用隔离数据目录覆盖。
import { expect, open, test } from "nsh/test";
import { subscribeNotify, type ToastRequest } from "../../../shell/events";
import { parseSkill, type MarketSkill } from "../types";
import {
  createDetailReader,
  detailBodyEmpty,
  extractGithubSource,
  parseDetailPayload,
  resolveIdentity,
  shownDescription,
  shownReadme,
  type SkillDetailPayload,
} from "./detailState";
import { createTranslateSession, rejectionMessage, type TranslateBridge } from "./translate";

const HOST = "./detailDrawer.test.host.tsx";

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
  }
}

// ---- 详情读状态机（mod.rs MarketDetailController tests 对位）--------------

function machineHarness() {
  const calls: {
    source: string;
    name: string;
    resolve: (payload: SkillDetailPayload) => void;
    reject: (reason: unknown) => void;
  }[] = [];
  const syncs: { scope: string; resolve: (v: void) => void; reject: (e: unknown) => void }[] = [];
  const reader = createDetailReader({
    reader: (source, name) =>
      new Promise<SkillDetailPayload>((resolve, reject) => {
        calls.push({ source, name, resolve, reject });
      }),
    syncer: (scope) =>
      new Promise<void>((resolve, reject) => {
        syncs.push({ scope, resolve, reject });
      }),
  });
  return { reader, calls, syncs };
}

function payload(
  overrides: Partial<SkillDetailPayload["data"]> = {},
  status = "fresh"
): SkillDetailPayload {
  return {
    data: {
      summary: null,
      readme: null,
      weeklyInstalls: null,
      githubStars: null,
      firstSeen: null,
      securityAudits: [],
      ...overrides,
    },
    snapshotStatus: status,
    snapshotUpdatedAt: null,
    error: null,
  };
}

function skillFixture(name: string, source: string | null): MarketSkill {
  return parseSkill({ name, description: `${name} blurb`, stars: 0, source: source ?? undefined });
}

test("换卡后旧响应被丢弃：迟到读取不覆盖新选择（epoch 红线）", async () => {
  const h = machineHarness();
  h.reader.open({ source: "vercel-labs/skills", name: "find-skills" }); // call 0
  await settle();
  expect(h.calls.length).toBe(1);
  h.reader.open({ source: "mattpocock/skills", name: "grill-me" }); // call 1
  await settle();
  expect(h.calls.length).toBe(2);

  // 旧选择（find-skills）的响应后到：必须整笔丢弃。
  h.calls[0].resolve(payload({ summary: "old" }));
  await settle();
  expect(h.reader.current()).toEqual({ source: "mattpocock/skills", name: "grill-me" });
  expect(h.reader.phase().kind).toBe("loading");
  expect(JSON.stringify(h.reader.phase())).not.toContain("old");

  // 新选择的响应落地。
  h.calls[1].resolve(payload({ summary: "new" }));
  await settle();
  const landed = h.reader.phase();
  expect(landed.kind).toBe("ready");
  if (landed.kind === "ready") {
    expect(landed.details.summary).toBe("new");
  }
});

test("关闭后迟到的响应同样被丢弃", async () => {
  const h = machineHarness();
  h.reader.open({ source: "a/b", name: "one" });
  await settle();
  h.reader.open(null);
  h.calls[0].resolve(payload({ summary: "late" }));
  await settle();
  expect(h.reader.current()).toBe(null);
  expect(h.reader.phase().kind).toBe("idle");
});

test("stale 快照先出摘要、后台只再同步一次；刷新失败不清已有摘要", async () => {
  const h = machineHarness();
  h.reader.open({ source: "vercel-labs/skills", name: "find-skills" });
  await settle();
  // 首读（非 refresh）：stale + 有摘要 → 立即 ready，并排一次后台同步。
  h.calls[0].resolve(payload({ summary: "Helps users" }, "stale"));
  await settle();
  expect(h.reader.phase().kind).toBe("ready");
  expect(h.syncs.map((sync) => sync.scope)).toEqual(["skill_detail:vercel-labs/skills/find-skills"]);
  // 同步完成后的重读（call 1）网络失败：已显示的摘要不动。
  h.syncs[0].resolve(undefined);
  await settle();
  expect(h.calls.length).toBe(2);
  h.calls[1].reject(new Error("network"));
  await settle();
  const kept = h.reader.phase();
  expect(kept.kind).toBe("ready");
  if (kept.kind === "ready") {
    expect(kept.details.summary).toBe("Helps users");
  }
  // 重读失败后不再循环发起第三次同步。
  expect(h.syncs.length).toBe(1);
});

test("miss 空体先回填；回填后 remote_error 且仍空体 → 失败相位", async () => {
  const h = machineHarness();
  h.reader.open({ source: "a/b", name: "one" });
  await settle();
  h.calls[0].resolve(payload({}, "miss"));
  await settle();
  expect(h.reader.phase().kind).toBe("loading");
  expect(h.syncs.length).toBe(1);
  h.syncs[0].resolve(undefined);
  await settle();
  expect(h.calls.length).toBe(2);
  h.calls[1].resolve({ ...payload({}, "remote_error"), error: "remote blew up" });
  await settle();
  const phase = h.reader.phase();
  expect(phase.kind).toBe("failed");
  if (phase.kind === "failed") {
    expect(phase.error).toContain("remote blew up");
  }
});

test("同卡重开不重发；无 source 不发请求（Idle）", async () => {
  const h = machineHarness();
  h.reader.open({ source: "a/b", name: "one" });
  await settle();
  expect(h.calls.length).toBe(1);
  h.reader.open({ source: "a/b", name: "one" });
  h.reader.open({ source: null, name: "local-only" });
  await settle();
  expect(h.calls.length).toBe(1);
  expect(h.reader.phase().kind).toBe("idle");
  expect(h.reader.current()).toEqual({ source: null, name: "local-only" });
});

test("失败行的重试强制刷新（先同步该 scope 再读）", async () => {
  const h = machineHarness();
  h.reader.open({ source: "a/b", name: "one" });
  await settle();
  h.calls[0].reject(new Error("first fail"));
  await settle();
  expect(h.reader.phase().kind).toBe("failed");
  h.reader.retry();
  await settle();
  expect(h.reader.phase().kind).toBe("loading");
  expect(h.syncs.length).toBe(1);
  h.syncs[0].resolve(undefined);
  await settle();
  expect(h.calls.length).toBe(2);
  h.calls[1].resolve(payload({ summary: "back" }));
  await settle();
  expect(h.reader.phase().kind).toBe("ready");
});

test("显式重试走 Loading 相位：再失败可呈失败（GPUI retry 语义）；后台刷新失败不清 ready", async () => {
  const h = machineHarness();
  h.reader.open({ source: "a/b", name: "one" });
  await settle();
  h.calls[0].resolve(payload({ summary: "kept" }));
  await settle();
  // 显式 retry 先置 Loading；其错误按 GPUI 语义可覆盖为 Failed。
  h.reader.retry();
  await settle();
  h.syncs[0].resolve(undefined);
  await settle();
  h.calls[1].reject(new Error("later fail"));
  await settle();
  const retried = h.reader.phase();
  expect(retried.kind).toBe("failed");
  if (retried.kind === "failed") {
    expect(retried.error).toContain("later fail");
  }
  // 再显式重试可以恢复（失败不是终态）。
  h.reader.retry();
  await settle();
  expect(h.reader.phase().kind).toBe("loading");
});

// ---- 身份与展示纯函数（mod.rs/column.rs 对位）------------------------------

test("source 回退到 GitHub 克隆 URL（extract_github_source_from_url 对位）", () => {
  expect(extractGithubSource("https://github.com/vercel-labs/skills")).toBe("vercel-labs/skills");
  expect(extractGithubSource("https://github.com/vercel-labs/skills.git")).toBe("vercel-labs/skills");
  expect(extractGithubSource("https://github.com/vercel-labs/skills/")).toBe("vercel-labs/skills");
  expect(extractGithubSource("https://gitlab.com/owner/repo")).toBe(null);
  expect(extractGithubSource("https://github.com/only-owner")).toBe(null);

  const fromUrl = resolveIdentity(
    parseSkill({ name: "find-skills", gitUrl: "https://github.com/vercel-labs/skills" }),
    { source: null, name: "find-skills" }
  );
  expect(fromUrl.source).toBe("vercel-labs/skills");
  const explicit = resolveIdentity(
    parseSkill({ name: "x", source: "o/r", gitUrl: "https://github.com/other/repo" }),
    { source: null, name: "x" }
  );
  expect(explicit.source).toBe("o/r");
});

test("shownDescription：快照摘要优先，回退卡面 blurb；readme 与描述相同不出按钮", () => {
  const skill = skillFixture("demo", "a/b");
  expect(shownDescription(skill, { kind: "loading" })).toBe("demo blurb");
  expect(shownDescription(skill, { kind: "ready", details: payload().data })).toBe("demo blurb");
  expect(
    shownDescription(skill, { kind: "ready", details: payload({ summary: "Summary wins" }).data })
  ).toBe("Summary wins");
  expect(shownReadme({ kind: "loading" }, "d")).toBe(null);
  expect(shownReadme({ kind: "ready", details: payload().data }, "d")).toBe(null);
  expect(
    shownReadme({ kind: "ready", details: payload({ readme: "same as d" }).data }, "same as d")
  ).toBe(null);
  expect(
    shownReadme({ kind: "ready", details: payload({ readme: "# full body" }).data }, "short")
  ).toBe("# full body");
});

test("parseDetailPayload 宽容降级：缺字段/未知状态不抛", () => {
  const parsed = parseDetailPayload(undefined);
  expect(parsed.snapshotStatus).toBe("fresh");
  expect(parsed.data.githubStars).toBe(null);
  expect(parsed.data.securityAudits).toEqual([]);
  const stars = parseDetailPayload({ data: { githubStars: 0, summary: "  " } });
  expect(stars.data.githubStars).toBe(null); // 0 不上卡
  expect(stars.data.summary).toBe(null); // 纯空白不算正文
  expect(detailBodyEmpty(payload().data)).toBe(true);
  expect(detailBodyEmpty(payload({ summary: "x" }).data)).toBe(false);
});

// ---- 翻译会话（translate.rs tests 对位）------------------------------------

function bridgeHarness(options: { switchOn?: boolean } = {}) {
  const cache = new Map<string, string>();
  const choices = new Map<string, boolean>();
  const fills: {
    sources: string[];
    resolve: (v: { done: { source: string; text: string }[]; rejection: { model: string; detail: string } | null }) => void;
    reject: (e: unknown) => void;
  }[] = [];
  const toasts: ToastRequest[] = [];
  const stop = subscribeNotify((toast) => toasts.push(toast));
  const bridge: TranslateBridge = {
    descriptionsEnabled: () => options.switchOn ?? false,
    descriptionChoice: (name) => (choices.has(name) ? (choices.get(name) as boolean) : null),
    needsTranslation: (text) => /[A-Za-z]/.test(text),
    lookup: (source) => cache.get(source) ?? null,
    blocked: () => false,
    setChoice: (name, translated) => {
      choices.set(name, translated);
    },
    fill: (sources) =>
      new Promise((resolve, reject) => {
        fills.push({ sources, resolve, reject });
      }),
  };
  return { bridge, cache, choices, fills, toasts, stop };
}

test("resolveTranslate：存储选择优先，其余跟设置开关", () => {
  const off = bridgeHarness();
  const session = createTranslateSession(off.bridge);
  expect(session.resolveTranslate("demo")).toBe(false);
  off.choices.set("demo", true);
  expect(session.resolveTranslate("demo")).toBe(true);
  const on = bridgeHarness({ switchOn: true });
  const sessionOn = createTranslateSession(on.bridge);
  expect(sessionOn.resolveTranslate("fresh-name")).toBe(true);
});

test("目标语言不需要翻译的文案不画语言按钮（中文之于 zh-CN）", () => {
  const h = bridgeHarness({ switchOn: true });
  const session = createTranslateSession(h.bridge);
  expect(session.buttonGate("为代理构建深模块。", true).visible).toBe(false);
  const english = session.buttonGate("Build deep modules for agents.", false);
  expect(english.visible).toBe(true);
  // 显示原文（未选译文）时按钮不扫。
  expect(english.pending).toBe(false);
});

test("选了译文且缓存未命中：按钮 pending，fill 只发一次且去重", async () => {
  const h = bridgeHarness({ switchOn: true });
  const session = createTranslateSession(h.bridge);
  const text = "Build deep modules for agents.";
  expect(session.buttonGate(text, true).pending).toBe(true);
  session.ensureTranslation(text, true);
  session.ensureTranslation(text, true); // 在途去重
  expect(h.fills.length).toBe(1);
  expect(h.fills[0].sources).toEqual([text]);
  // 域语义：成功即写盘，随后 generation bump、lookup 命中。
  h.cache.set(text, "为代理构建深模块。");
  const before = session.generation();
  h.fills[0].resolve({ done: [{ source: text, text: "为代理构建深模块。" }], rejection: null });
  await settle();
  expect(session.generation()).toBe(before + 1);
  expect(session.buttonGate(text, true).pending).toBe(false);
  expect(session.descriptionLine(text, true)).toBe("为代理构建深模块。");
  expect(session.descriptionLine(text, false)).toBe(text);
});

test("模型拒绝：钉住的通知带 + 会话内不再发 fill；从原文再翻时清除", async () => {
  const h = bridgeHarness({ switchOn: true });
  const session = createTranslateSession(h.bridge);
  const text = "Build deep modules for agents.";
  session.ensureTranslation(text, true);
  h.fills[0].resolve({ done: [], rejection: { model: "demo-model", detail: "refused." } });
  await settle();
  expect(session.rejected()).toBe(true);
  expect(h.toasts.length).toBe(1);
  const toast = h.toasts[0];
  if (!toast) throw new Error("expected a rejection toast");
  expect(toast.tone).toBe("error");
  expect(toast.pinned).toBe(true);
  expect(toast.replaceKey).toBe("translation-model-rejected");
  expect(toast.message).toContain("demo-model");
  // 拒绝后按钮不再 pending，也不再排引擎调用。
  expect(session.buttonGate(text, true).pending).toBe(false);
  session.ensureTranslation(text, true);
  expect(h.fills.length).toBe(1);
  // 从原文翻过去（showing=false 的按下）清拒绝状态（retry_rejected）。
  session.applyChoice("demo", false);
  expect(session.rejected()).toBe(false);
  expect(h.choices.get("demo")).toBe(true);
  h.stop();
});

test("applyChoice 只写这一技的选择，不动设置开关；generation bump", () => {
  const h = bridgeHarness();
  const session = createTranslateSession(h.bridge);
  const before = session.generation();
  session.applyChoice("demo", false); // 显示原文 → 选择译文
  expect(h.choices.get("demo")).toBe(true);
  expect(h.choices.has("other")).toBe(false);
  session.applyChoice("demo", true); // 显示译文 → 选择原文
  expect(h.choices.get("demo")).toBe(false);
  expect(session.generation()).toBe(before + 2);
});

test("传输失败（非模型拒绝）不热循环：本会话静默该行，按钮再按下可重问", async () => {
  const h = bridgeHarness({ switchOn: true });
  const session = createTranslateSession(h.bridge);
  const text = "Build deep modules for agents.";
  session.ensureTranslation(text, true);
  h.fills[0].reject(new Error("network down"));
  await settle();
  expect(h.fills.length).toBe(1); // bump 后 effect 重跑也不该再排
  session.ensureTranslation(text, true);
  expect(h.fills.length).toBe(1);
  expect(session.buttonGate(text, true).pending).toBe(false);
  // 从原文再翻一次清失败标记，可以再问。
  session.applyChoice("demo", false);
  session.ensureTranslation(text, true);
  expect(h.fills.length).toBe(2);
  h.stop();
});

test("rejectionMessage：无细节走短文案，有细节带细节", () => {
  expect(rejectionMessage("demo-model", "")).not.toContain(":");
  expect(rejectionMessage("demo-model", "refused.")).toContain("refused");
});

// ---- 组件（host 装置）------------------------------------------------------

test("列渲染：名称、元数据、安装动作与 skills.sh 链接（column.rs 对位）", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-alpha");
  await app.waitUntil(() => app.texts().some((t) => t === "alpha"), { timeout: 5000 });
  expect(app.shows("id:mk-detail-column")).toBe(true);
  expect(app.shows("id:mk-detail-install-alpha")).toBe(true);
  // 元数据卡：来源/作者/下载量/最近更新（列表行已有的信息）。
  expect(app.shows("id:mk-detail-metadata")).toBe(true);
  expect(app.texts()).toContain("vercel-labs/skills");
  expect(app.texts()).toContain("@vercel");
  expect(app.texts()).toContain("1.2k");
  expect(app.shows("id:mk-detail-source")).toBe(true);
});

test("详情落地：摘要/审计/周安装量上列，readme 不同出 SKILL.md 按钮并开悬浮窗", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-alpha");
  // 打开即首读在途：loading 行可见。
  await app.waitUntil(() => app.shows("id:mk-detail-loading"), { timeout: 5000 });
  await app.press("id:host-resolve-first");
  await app.waitUntil(() => app.texts().some((t) => t.includes("Fresh alpha summary")), { timeout: 5000 });
  expect(app.texts()).toContain("103.7K");
  expect(app.texts()).toContain("42");
  expect(app.shows("id:mk-detail-audits")).toBe(true);
  expect(app.texts()).toContain("secrets");
  // readme 与摘要不同 → 按钮；点开窗口正中的悬浮窗，正文可滚动。
  await app.press("id:mk-view-skill-md");
  await app.waitUntil(() => app.shows("id:mk-skill-md-overlay"), { timeout: 5000 });
  expect(app.shows("id:mk-skill-md-body")).toBe(true);
  await app.press("id:mk-skill-md-close");
  await app.waitUntil(() => !app.shows("id:mk-skill-md-overlay"), { timeout: 5000 });
});

test("换卡丢弃（组件级红线）：旧卡的迟到响应不进新卡列", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  // alpha 的首读挂起；换到 beta（新读取在途）。
  await app.press("id:host-pick-alpha");
  await app.waitUntil(() => app.texts().some((t) => t === "alpha"), { timeout: 5000 });
  await app.press("id:host-pick-beta");
  await app.waitUntil(() => app.texts().some((t) => t === "beta"), { timeout: 5000 });
  // 放行最旧一笔 = alpha 的迟到响应：列仍显示 beta 的卡面信息。
  await app.press("id:host-resolve-first");
  await app.waitUntil(() => app.shows("id:mk-detail-loading"), { timeout: 5000 });
  expect(app.texts().some((t) => t.includes("Fresh alpha summary"))).toBe(false);
  expect(app.texts()).not.toContain("alpha blurb");
});

test("无 source 不发请求：列只显示列表行已有信息", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-none");
  await app.waitUntil(() => app.texts().some((t) => t === "local-only"), { timeout: 5000 });
  expect(app.shows("id:mk-detail-loading")).toBe(false);
  expect(app.shows("id:mk-detail-source")).toBe(false);
  expect(app.texts()).toContain("local-only blurb");
});

test("失败相位：原因行 + 重试按钮；重试先同步该 scope 再读", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-alpha");
  await app.waitUntil(() => app.shows("id:mk-detail-loading"), { timeout: 5000 });
  await app.press("id:host-mode-reject");
  await app.press("id:host-resolve-first");
  await app.waitUntil(() => app.shows("id:mk-detail-error"), { timeout: 5000 });
  expect(app.shows("id:mk-detail-retry")).toBe(true);
  await app.press("id:host-mode-fresh");
  await app.press("id:host-resolve-first"); // 复位脚本
  // 重试 → refresh：先排 skill_detail 同步。
  await app.press("id:mk-detail-retry");
  await app.waitUntil(() => app.shows("id:mk-detail-loading"), { timeout: 5000 });
  await app.press("id:host-resolve-sync");
  await app.press("id:host-resolve-first");
  await app.waitUntil(() => app.texts().some((t) => t.includes("Fresh alpha summary")), { timeout: 5000 });
});

test("翻译按钮只翻当前打开：按下写选择，fill 落地后列换译文；再按回原文", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-beta");
  await app.waitUntil(() => app.texts().some((t) => t.includes("Build deep modules for agents.")), {
    timeout: 5000,
  });
  expect(app.shows("id:mk-detail-translate")).toBe(true);
  expect(app.shows("id:mk-drawer-original")).toBe(true);
  // 原文态不扫（pending 只属于「选了译文还没落地」）。
  expect(app.shows("id:mk-detail-translate-sweep")).toBe(false);
  // 按下：选择=译文。缓存未命中先显示原文并排一次引擎调用。
  await app.press("id:mk-detail-translate");
  await app.waitUntil(() => app.texts().some((t) => /^fills:[1-9]/.test(t)), { timeout: 5000 });
  // 引擎在飞：一道激光在图标上扫（icon_sweep），译文还没落地。
  expect(app.shows("id:mk-detail-translate-sweep")).toBe(true);
  // fill 落地（域语义：成功即写盘）→ 列换译文，扫光停。
  await app.press("id:host-resolve-fill");
  await app.waitUntil(() => app.shows("id:mk-drawer-translation"), { timeout: 5000 });
  expect(app.shows("id:mk-detail-translate-sweep")).toBe(false);
  expect(app.texts()).toContain("为代理构建深模块。");
  // 再按：回原文（选择记为 false）。
  await app.press("id:mk-detail-translate");
  await app.waitUntil(() => app.shows("id:mk-drawer-original"), { timeout: 5000 });
});

test("设置开关开着时无需点击即显示缓存译文（the_description_setting 对位）", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-remember");
  await app.press("id:host-switch-on");
  await app.press("id:host-pick-beta");
  await app.waitUntil(() => app.shows("id:mk-drawer-translation"), { timeout: 5000 });
  // 按钮仍在（此刻提供「原文」）。
  expect(app.shows("id:mk-detail-translate")).toBe(true);
});

test("模型拒绝走通知带：钉住且带 replaceKey，描述下方一行原因", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-switch-on");
  await app.press("id:host-pick-beta");
  await app.waitUntil(() => app.shows("id:mk-drawer-original"), { timeout: 5000 });
  await app.waitUntil(() => app.texts().some((t) => /^fills:[1-9]/.test(t)), { timeout: 5000 });
  await app.press("id:host-reject-fill");
  // 通知带出现模型拒绝文案；描述段仍在（failure_line 在其下方）。
  await app.waitUntil(() => app.texts().some((t) => t.includes("demo-model")), { timeout: 5000 });
  expect(app.shows("id:mk-detail-description")).toBe(true);
});

test("busy 钉住安装动作；点击透传技能；列内关闭回调", async () => {
  const app = await open(HOST);
  await app.setViewport(1280, 900);
  await app.press("id:host-reset");
  await app.press("id:host-pick-alpha");
  await app.waitUntil(() => app.enabled("id:mk-detail-install-alpha"), { timeout: 5000 });
  await app.press("id:mk-detail-install-alpha");
  await app.waitUntil(() => app.texts().some((t) => t.includes("toggle:alpha")), { timeout: 5000 });
  await app.press("id:host-page-busy");
  expect(app.enabled("id:mk-detail-install-alpha")).toBe(false);
  await app.press("id:host-page-busy");
  expect(app.enabled("id:mk-detail-install-alpha")).toBe(true);
  // 列内关闭按钮把清除选择交回页面（onClose）。
  await app.press("id:close-market-detail-drawer");
  await app.waitUntil(() => app.texts().some((t) => t === "closed:1"), { timeout: 5000 });
});

