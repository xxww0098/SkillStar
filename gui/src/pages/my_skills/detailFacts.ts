// 详情抽屉纯逻辑半 — my_skills/detail_facts.rs + detail_agents.rs 的批量开关
// 语义（commands.rs master_switch_* / toggle_listed_agents 的通知组装）+
// skill_card/mod.rs toggle_skip_message 的对位。
//
// 全部纯函数（无信号、无 IO），对位测试直接驱动；组件在 JSX 表达式里调
// 用以获得响应性。语义逐条对齐 Rust 版，不自创规则：
// - 一条事实只出现一次：仓库 URL 已含 owner/repo 与作者时不再单列；
// - 安装路径不是事实（打开目录是动作）；
// - 未到期时间戳原样保留（不猜更短的值）。

import { t, tf } from "../../services/translation";
import type { SkillRecord } from "./filters";

/** detail_facts.rs SourceValue：Link 用浏览器外开，Text 原文照排（SSH 不
 * 是浏览器链接）。 */
export interface SourceValue {
  kind: "link" | "text";
  value: string;
}

function nonempty(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function isHttp(value: string): boolean {
  const lower = value.toLowerCase();
  return lower.startsWith("https://") || lower.startsWith("http://");
}

/** detail_facts.rs normalize_source：去协议/ssh 用户前缀、尾斜杠与 .git、
 * 冒号换斜杠、小写。 */
function normalizeSource(value: string): string {
  let v = value.trim();
  while (v.endsWith("/")) v = v.slice(0, -1);
  while (v.endsWith(".git")) v = v.slice(0, -4);
  for (const prefix of ["https://", "http://", "ssh://", "git@"]) {
    while (v.startsWith(prefix)) v = v.slice(prefix.length);
  }
  return v.replace(/:/g, "/").toLowerCase();
}

/** covers：URL 已包含该来源（相等 / 尾段 / 中段）。 */
function covers(url: string, source: string): boolean {
  const u = normalizeSource(url);
  const s = normalizeSource(source);
  if (s.length === 0) return false;
  return u === s || u.endsWith(`/${s}`) || u.includes(`/${s}/`);
}

/** detail_facts.rs source_values：HTTP(S) 地址在前，来源未被地址覆盖时再
 * 列；与技能名相同的来源不是第二条事实。 */
export function sourceValues(
  gitUrl: string,
  source: string | undefined,
  name: string
): SourceValue[] {
  const out: SourceValue[] = [];
  const git = nonempty(gitUrl);
  if (git !== null) out.push({ kind: isHttp(git) ? "link" : "text", value: git });
  const extra = source === undefined ? null : nonempty(source);
  if (extra !== null && extra !== name) {
    const covered = out.some((line) => covers(line.value, extra));
    if (!covered) out.push({ kind: isHttp(extra) ? "link" : "text", value: extra });
  }
  return out;
}

/** detail_facts.rs shown_author：作者已内含在某条来源里时不重复出现。 */
export function shownAuthor(
  author: string | undefined,
  lines: readonly SourceValue[]
): string | null {
  if (author === undefined) return null;
  const raw = nonempty(author);
  if (raw === null) return null;
  const handle = raw.replace(/^@+/, "");
  if (handle.length === 0) return null;
  const covered = lines.some(
    (line) => covers(line.value, raw) || covers(line.value, handle)
  );
  return covered ? null : raw;
}

/** RFC3339 严格闸（filters.ts updatedAtMs 同款正则；chrono 拒收的串这里
 * 也不给 Date.parse）。 */
const RFC3339 = /^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/;

/** detail_facts.rs format_updated：本地分钟精度；解析不了的原样保留。 */
export function formatUpdated(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
  if (!RFC3339.test(trimmed)) return trimmed;
  const date = new Date(trimmed);
  if (Number.isNaN(date.getTime())) return trimmed;
  const p2 = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${p2(date.getMonth() + 1)}-${p2(date.getDate())} ${p2(
    date.getHours()
  )}:${p2(date.getMinutes())}`;
}

/** upstream_note 的展示形态（detail_facts.rs UpstreamNote）。 */
export type UpstreamNote =
  | { kind: "removed"; source: string; folder: string }
  | { kind: "renamed"; from: string; to: string }
  | { kind: "localEdits"; baselineMissing: boolean };

/** display_source：来源优先（非空且不等于技能名），回退 git URL。 */
function displaySource(skill: SkillRecord): string {
  const source = skill.source === undefined ? null : nonempty(skill.source);
  if (source !== null && source !== skill.name) return source;
  return nonempty(skill.gitUrl) ?? "";
}

/** detail_facts.rs upstream_note：从卡记录推导（本地改动是否有基线由
 * MySkillsDrawer.upstreamNote 异步补充，见 detailDrawerSlot）。 */
export function upstreamNoteOf(
  skill: SkillRecord,
  baselineMissing = false
): UpstreamNote | null {
  const change = skill.upstreamChange;
  if (change === null) return null;
  switch (change.kind) {
    case "removed":
      return { kind: "removed", source: displaySource(skill), folder: skill.name };
    case "identity_changed":
      return { kind: "renamed", from: skill.name, to: change.upstreamName };
    case "local_changes":
      return { kind: "localEdits", baselineMissing };
  }
}

/** detail_facts.rs wrap_long_token：URL 标点后插零宽断行点，窄列能显示
 * 完整地址；存储的 href 不变。 */
export function wrapLongToken(value: string): string {
  let out = "";
  for (const ch of value) {
    out += ch;
    if (ch === "/" || ch === "." || ch === "-" || ch === "_") out += "\u200b";
  }
  return out;
}

// ---- agent 开关语义（detail_agents.rs + commands.rs） ------------------------

/** chrome/detail.rs profile_is_linked：链接按 id 或显示名匹配（旧记录存
 * 显示名）。 */
export function profileIsLinked(
  agent: { id: string; displayName: string },
  links: readonly string[]
): boolean {
  return links.some((link) => link === agent.id || link === agent.displayName);
}

/** commands.rs master_switch_enable：还有没链接的 → 链齐（true）；全已链
 * 接 → 全取消（false）；空表 → null。 */
export function masterSwitchEnable(pairs: readonly [string, boolean][]): boolean | null {
  if (pairs.length === 0) return null;
  return pairs.some(([, linked]) => !linked);
}

/** commands.rs master_switch_agent_ids：只动还没到位的行，已就位的不动。 */
export function masterSwitchAgentIds(
  pairs: readonly [string, boolean][],
  enable: boolean
): string[] {
  return pairs.filter(([, linked]) => linked !== enable).map(([id]) => id);
}

/** MySkillsDrawer.setAgentLinks 的报告形状（facade toggle_listed）。 */
export interface AgentLinkReport {
  total: number;
  failed: number;
  skipped: number;
  canonical: number;
}

/** commands.rs toggle_listed_agents 的通知组装：失败/跳过逐行计数；全为
 * 规范根 agent 时给出解释；一切正常返回 null（成功路径保持安静）。 */
export function batchLinkNotice(report: AgentLinkReport): string | null {
  const total = String(report.total);
  if (report.failed === 0 && report.skipped === 0) {
    if (report.canonical === report.total && report.total > 0) {
      return t("skillToggle.servedByCanonicalRoot");
    }
    return null;
  }
  const parts: string[] = [];
  if (report.failed > 0) {
    parts.push(
      tf("skillCards.batchTogglePartialFailed", {
        failed: report.failed,
        total,
      })
    );
  }
  if (report.skipped > 0) {
    parts.push(
      tf("skillCards.batchTogglePartialSkipped", {
        skipped: report.skipped,
        total,
      })
    );
  }
  return parts.join("\n");
}

/** MySkillsDrawer.toggleAgent 的返回形状。 */
export type ToggleOutcome =
  | { kind: "applied" }
  | { kind: "skipped"; code: string; path: string; reason: string };

/** skill_card/mod.rs toggle_skip_message：规范根 agent 是说明不是错误；
 * 副本修改与未管理目录分别点名路径。 */
export function toggleSkipMessage(
  name: string,
  outcome: { code: string; path: string }
): string {
  if (outcome.code === "canonical_root_agent") {
    return t("skillToggle.servedByCanonicalRoot");
  }
  if (outcome.code === "modified_agent_copy") {
    return tf("skillToggle.skipModifiedCopyItem", { name, path: outcome.path });
  }
  if (outcome.code === "foreign_link") {
    return tf("skillToggle.skipForeignLinkItem", { name, path: outcome.path });
  }
  return tf("skillToggle.skipUnmanagedDirItem", { name, path: outcome.path });
}

// ---- 描述翻译面（detail_drawer.rs description_source） ----------------------

/** 原文描述优先（卡片缓存或快照填充的 localizedDescription 不可反压原文，
 * 否则已译技能会丢失翻译按钮且无法切回原文）；仅原文为空时回退。 */
export function descriptionSource(skill: SkillRecord): string | null {
  const raw = nonempty(skill.description);
  if (raw !== null) return raw;
  const localized =
    typeof skill.localizedDescription === "string" ? skill.localizedDescription : "";
  return nonempty(localized);
}
