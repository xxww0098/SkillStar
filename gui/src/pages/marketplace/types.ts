// 市场页共享类型与纯工具 — marketplace/types.rs 对位。Tab 枚举、快照行
// 的宽容解析（parseX 把未知形状降级成默认值，页面永不因桥数据缺字段
// 抛错）、装机量格式化、卡面 blurb 选择，全部纯函数。

import { marketTranslatedText } from "./translateQueue";

/** 四个 tab（types.rs MarketplaceTab）："all"/"trending"/"hot" 走
 * leaderboard 读取，"official" 换成发布者列表。 */
export type MarketTabId = "all" | "trending" | "hot" | "official";

/** Tab 的 leaderboard 类目（types.rs leaderboard_category；official 恒
 * "all"，只有非 official tab 会用到）。 */
export function leaderboardCategory(tab: MarketTabId): string {
  return tab === "trending" ? "trending" : tab === "hot" ? "hot" : "all";
}

/** 快照状态 tag（LocalFirstResult.snapshotStatus 的桥侧字符串，含未初始化 unknown）。 */
export type SnapshotStatusTag =
  | "unknown"
  | "fresh"
  | "stale"
  | "seeding"
  | "miss"
  | "error_fallback"
  | "remote_error";

/** 结构化快照错误（{ kind, scope, detail }）。 */
export interface SnapshotErrorInfo {
  kind: SnapshotStatusTag;
  scope: string;
  detail: string;
}

/** 本地优先读结果（LocalFirstResult<T> 的桥形状）。 */
export interface LocalFirst<T> {
  data: T;
  snapshotStatus: SnapshotStatusTag;
  snapshotUpdatedAt: string | null;
  error: SnapshotErrorInfo | null;
}

/** 市场技能行（Skill 的桥形状 + 宽容解析后的视图记录）。 */
export interface MarketSkill {
  name: string;
  description: string;
  localizedDescription: string | null;
  skillType: "hub" | "local";
  stars: number;
  installed: boolean;
  updateAvailable: boolean;
  lastUpdated: string;
  gitUrl: string;
  category: string;
  author: string | null;
  rank: number | null;
  source: string | null;
  called?: string;
  upstreamRemoved: boolean;
}

/** 官方发布者行（OfficialPublisher 的桥形状）。 */
export interface PublisherRecord {
  name: string;
  repo: string;
  repoCount: number;
  skillCount: number;
  url: string;
}

function str(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function optStr(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function num(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function optNum(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** 桥的一行 → 视图记录。未知形状全部降级（deckQuery parseDeck 同款）。 */
export function parseSkill(row: unknown): MarketSkill {
  const record = (row ?? {}) as Record<string, unknown>;
  return {
    name: str(record.name),
    description: str(record.description),
    localizedDescription: optStr(record.localizedDescription),
    skillType: record.skillType === "local" ? "local" : "hub",
    stars: num(record.stars),
    installed: record.installed === true,
    updateAvailable: record.updateAvailable === true,
    lastUpdated: str(record.lastUpdated),
    gitUrl: str(record.gitUrl),
    category: str(record.category, "none"),
    author: optStr(record.author),
    rank: optNum(record.rank),
    source: optStr(record.source),
    called: optStr(record.called) ?? undefined,
    upstreamRemoved: record.upstreamRemoved === true,
  };
}

export function parsePublisher(row: unknown): PublisherRecord {
  const record = (row ?? {}) as Record<string, unknown>;
  return {
    name: str(record.name),
    repo: str(record.repo),
    repoCount: num(record.repoCount),
    skillCount: num(record.skillCount),
    url: str(record.url),
  };
}

export function parseSnapshotError(
  value: unknown,
  fallbackKind: SnapshotStatusTag = "remote_error",
  fallbackScope = ""
): SnapshotErrorInfo | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0
      ? { kind: fallbackKind, scope: fallbackScope, detail: trimmed }
      : null;
  }
  if (value !== null && typeof value === "object") {
    const rec = value as Record<string, unknown>;
    const detail = str(rec.detail).trim();
    const scope = str(rec.scope).trim() || fallbackScope;
    const kind = parseStatus(rec.kind) === "unknown" ? fallbackKind : parseStatus(rec.kind);
    if (detail.length > 0) {
      return { kind, scope, detail };
    }
  }
  return null;
}

/** 整个 LocalFirst 载荷解析；data 行逐个降级。 */
export function parseSkillsPayload(payload: unknown): LocalFirst<MarketSkill[]> {
  const record = (payload ?? {}) as Record<string, unknown>;
  const rows = Array.isArray(record.data) ? record.data : [];
  const status = parseStatus(record.snapshotStatus);
  return {
    data: rows.map(parseSkill),
    snapshotStatus: status,
    snapshotUpdatedAt: optStr(record.snapshotUpdatedAt),
    error: parseSnapshotError(record.error, status),
  };
}

export function parsePublishersPayload(payload: unknown): LocalFirst<PublisherRecord[]> {
  const record = (payload ?? {}) as Record<string, unknown>;
  const rows = Array.isArray(record.data) ? record.data : [];
  const status = parseStatus(record.snapshotStatus);
  return {
    data: rows.map(parsePublisher),
    snapshotStatus: status,
    snapshotUpdatedAt: optStr(record.snapshotUpdatedAt),
    error: parseSnapshotError(record.error, status),
  };
}

export function parseStatus(value: unknown): SnapshotStatusTag {
  switch (value) {
    case "fresh":
    case "stale":
    case "seeding":
    case "miss":
    case "error_fallback":
    case "remote_error":
      return value;
    default:
      return "unknown";
  }
}

/** 装机量/星标格式化（types.rs format_installs）。 */
export function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k`;
  return String(count);
}

/** 卡面 blurb：本地化描述或翻译队列缓存优先，空描述返回 null。 */
export function skillBlurb(skill: MarketSkill): string | null {
  const localized = skill.localizedDescription?.trim();
  if (localized) return localized;
  const cached = marketTranslatedText(skill.description);
  if (cached) return cached;
  const plain = skill.description.trim();
  return plain.length > 0 ? plain : null;
}
