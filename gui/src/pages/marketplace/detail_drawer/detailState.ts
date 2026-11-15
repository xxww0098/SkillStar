import { signal, type Signal } from "nsh";
import type { MarketSkill } from "../types";
import { skillBlurb } from "../types";

// 市场详情列读侧状态机 — detail_drawer/mod.rs 的 MarketDetailController
// 对位（本切片核心数据流）。
//
// 打开一张卡读取 source+name 的本地详情（get_skill_detail_local）：
// - 迟到响应按（epoch + 身份）丢弃——换卡/关抽屉后旧响应绝不能覆盖新
//   选择（mod.rs:5-7 契约，验收红线）；
// - 本地结果 stale/miss 时后台再同步一次（sync_scope
//   "skill_detail:<source>/<name>"）并重读，不循环——每次打开只刷一次；
// - remote_error/error_fallback 且正文为空 → Failed（技术原因进结构化
//   字段，文案层再映射）；
// - 没有 source 时不发请求（Idle，列只显示列表行已有的信息）。
//
// 依赖全部注入（reader/syncer），页面用真实 facade 桥，nsh test 用可控
// Promise 直接断言状态机（readSide.ts 同款约定）。每个实例自持 signal，
// 页面模块级单例即页面实体；测试各建各的。

/** 详情体（MarketplaceSkillDetails 的桥形状，宽容解析后）。 */
export interface SkillDetails {
  summary: string | null;
  readme: string | null;
  weeklyInstalls: string | null;
  githubStars: number | null;
  firstSeen: string | null;
  securityAudits: { name: string; result: string }[];
}

/** 一次详情读取的桥载荷（LocalFirstResult<SkillDetails>）。 */
export interface SkillDetailPayload {
  data: SkillDetails;
  snapshotStatus: string;
  snapshotUpdatedAt: string | null;
  error: string | null;
}

/** 列的相位（mod.rs DetailPhase）。 */
export type DetailPhase =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; details: SkillDetails }
  | { kind: "failed"; error: string };

export interface DetailIdentity {
  source: string | null;
  name: string;
}

export interface DetailReaderDeps {
  /** get_skill_detail_local 桥（MarketplaceDrawer.skillDetail）。 */
  reader(source: string, name: string): Promise<SkillDetailPayload>;
  /** sync_marketplace_scope 桥（Marketplace.syncScope，scope 串由本模块拼）。 */
  syncer(scope: string): Promise<void>;
}

export interface DetailReader {
  phase: Signal<DetailPhase>;
  /** 当前选中身份（null = 关闭）。 */
  current: Signal<DetailIdentity | null>;
  /** 打开/换卡/关闭（mod.rs toggle/clear 的相位半；选择归属页面 readSide）。 */
  open(identity: DetailIdentity | null): void;
  /** 失败行的重试（mod.rs retry）：强制 refresh 一次。 */
  retry(): void;
}

/** detail_drawer/mod.rs `skill_detail_scope`：`skill_detail:<source>/<name>`。 */
export function skillDetailScope(source: string, name: string): string {
  return `skill_detail:${source}/${name}`;
}

function nonEmpty(text: string | null | undefined): string | null {
  if (typeof text !== "string") return null;
  const trimmed = text.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/** detail_body_empty（mod.rs:296-300）：摘要与正文都空才算空。 */
export function detailBodyEmpty(details: SkillDetails): boolean {
  return nonEmpty(details.summary) === null && nonEmpty(details.readme) === null;
}

/** 桥载荷宽容解析（types.ts parseSkill 同款纪律：缺字段降级不抛）。 */
export function parseDetailPayload(payload: unknown): SkillDetailPayload {
  const record = (payload ?? {}) as Record<string, unknown>;
  const data = (record.data ?? {}) as Record<string, unknown>;
  const audits = Array.isArray(data.securityAudits) ? data.securityAudits : [];
  const stars = typeof data.githubStars === "number" ? data.githubStars : null;
  return {
    data: {
      summary: nonEmpty(typeof data.summary === "string" ? data.summary : null),
      readme: nonEmpty(typeof data.readme === "string" ? data.readme : null),
      weeklyInstalls:
        typeof data.weeklyInstalls === "string" ? data.weeklyInstalls : null,
      githubStars: stars !== null && stars > 0 ? stars : null,
      firstSeen: typeof data.firstSeen === "string" ? data.firstSeen : null,
      securityAudits: audits.map((row) => {
        const audit = (row ?? {}) as Record<string, unknown>;
        return {
          name: typeof audit.name === "string" ? audit.name : "",
          result: typeof audit.result === "string" ? audit.result : "",
        };
      }),
    },
    snapshotStatus: typeof record.snapshotStatus === "string" ? record.snapshotStatus : "fresh",
    snapshotUpdatedAt: typeof record.snapshotUpdatedAt === "string" ? record.snapshotUpdatedAt : null,
    error: typeof record.error === "string" ? record.error : null,
  };
}

export function createDetailReader(deps: DetailReaderDeps): DetailReader {
  const phase = signal<DetailPhase>({ kind: "idle" });
  const current = signal<DetailIdentity | null>(null);
  // 每次打开/换卡/关闭/重试都 bump；迟到的读取按它丢弃（mod.rs epoch）。
  let epoch = 0;
  // 一次打开至多一次后台同步（mod.rs refreshed）。
  let refreshed = false;

  function sameIdentity(a: DetailIdentity, b: DetailIdentity): boolean {
    return a.name === b.name && a.source === b.source;
  }

  /** spawn_market_detail：refresh 先同步该 scope，再读本地。 */
  function fetch(identity: DetailIdentity, atEpoch: number, refresh: boolean): void {
    const source = identity.source;
    if (source === null) return;
    const pre = refresh
      ? deps.syncer(skillDetailScope(source, identity.name)).catch(() => {
          // 同步失败也让随后的本地读回答（mod.rs 里 sync 的错误被丢弃）。
        })
      : Promise.resolve();
    pre
      .then(() => deps.reader(source, identity.name))
      .then((payload) => apply(atEpoch, identity, refresh, { ok: true, payload }))
      .catch((err: unknown) => apply(atEpoch, identity, refresh, { ok: false, error: String(err) }));
  }

  /** mod.rs apply/apply_local：迟到丢弃 + 一次性回填刷新 + 空体降级。 */
  function apply(
    atEpoch: number,
    identity: DetailIdentity,
    wasRefresh: boolean,
    outcome: { ok: true; payload: SkillDetailPayload } | { ok: false; error: string }
  ): void {
    const open = current();
    // 红线：epoch 或身份不再匹配的响应直接丢。
    if (atEpoch !== epoch || open === null || !sameIdentity(open, identity)) return;
    if (!outcome.ok) {
      if (phase().kind !== "ready") {
        phase.set({ kind: "failed", error: outcome.error });
      }
      return;
    }
    const parsed = outcome.payload;
    const details = parsed.data;
    const empty = detailBodyEmpty(details);
    const refreshable = parsed.snapshotStatus === "stale" || parsed.snapshotStatus === "miss";
    if (refreshable && !refreshed && !wasRefresh) {
      refreshed = true;
      if (!empty) {
        phase.set({ kind: "ready", details });
      }
      fetch(identity, atEpoch, true);
      return;
    }
    if (
      (parsed.snapshotStatus === "remote_error" || parsed.snapshotStatus === "error_fallback") &&
      empty &&
      phase().kind !== "ready"
    ) {
      phase.set({ kind: "failed", error: parsed.error ?? "" });
      return;
    }
    phase.set({ kind: "ready", details });
  }

  return {
    phase,
    current,

    open(identity: DetailIdentity | null): void {
      const open = current();
      if (identity === null) {
        if (open === null) return;
        current.set(null);
        epoch += 1;
        phase.set({ kind: "idle" });
        return;
      }
      if (open !== null && sameIdentity(open, identity)) return;
      current.set(identity);
      epoch += 1;
      refreshed = false;
      const atEpoch = epoch;
      phase.set(identity.source === null ? { kind: "idle" } : { kind: "loading" });
      fetch(identity, atEpoch, false);
    },

    retry(): void {
      const open = current();
      if (open === null || open.source === null) return;
      current.set(open);
      epoch += 1;
      refreshed = true;
      const atEpoch = epoch;
      phase.set({ kind: "loading" });
      fetch(open, atEpoch, true);
    },
  };
}

/** MarketSkillId::from_skill 的身份解析：skill.source 优先，git URL 回退
 * （mod.rs:55-69；extract_github_source_from_url 对位）。fallback 是页面
 * 选择里已有的身份：技能行在刷新后丢了 source 时沿用它，避免同一选择
 * 在两次 props 变化间反复换身份（GPUI 的控制器在点击时定格身份，本列
 * 每次 props 变化重解析，需要这个稳定器）。 */
export function resolveIdentity(skill: MarketSkill, fallback: DetailIdentity): DetailIdentity {
  const trimmed = skill.source?.trim();
  if (trimmed) return { source: trimmed, name: skill.name };
  if (typeof fallback.source === "string" && fallback.source.trim().length > 0) {
    return { source: fallback.source, name: skill.name };
  }
  return { source: extractGithubSource(skill.gitUrl), name: skill.name };
}

/** extract_github_source_from_url（ss-core types/skill.rs:154-168）。 */
export function extractGithubSource(url: string): string | null {
  const lower = url.toLowerCase();
  const prefix = "https://github.com/";
  if (!lower.startsWith(prefix)) return null;
  let clean = url.slice(prefix.length);
  if (clean.endsWith(".git")) clean = clean.slice(0, -4);
  if (clean.endsWith("/")) clean = clean.slice(0, -1);
  return clean.includes("/") ? clean : null;
}

/** shown_description（column.rs:737-744）：列显示的一行——快照摘要优先，
 * 否则卡面 blurb。 */
export function shownDescription(skill: MarketSkill, phase: DetailPhase): string | null {
  if (phase.kind === "ready") {
    const summary = nonEmpty(phase.details.summary);
    if (summary !== null) return summary;
  }
  return skillBlurb(skill);
}

/** shown_readme（column.rs:746-755）：正文与描述不同才给按钮。 */
export function shownReadme(phase: DetailPhase, description: string | null): string | null {
  if (phase.kind !== "ready") return null;
  const readme = nonEmpty(phase.details.readme);
  if (readme === null) return null;
  if (description !== null && description.trim() === readme) return null;
  return readme;
}
