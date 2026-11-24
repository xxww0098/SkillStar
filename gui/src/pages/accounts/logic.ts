// 账号页的纯逻辑 —— 卡片配色档位、额度窗口排序、进度条数值、重置倒计时与
// 账号排序。全部无副作用、不读 signal（主题色由调用方 paletteToken 现取），
// 因此可以直接单测（logic 层的既有纪律：nsh 测试装置的假桥只覆盖交互，
// 纯语义在这一层钉死）。

import type {
  AccountView,
  ActivateOutcome,
  AgentLive,
  Credits,
  UsageSnapshot,
  UsageWindow,
} from "./types";

/** hex + 两位 alpha：把 palette 的实色降到 x% 底（GPUI rgb().alpha() 对位）。 */
export function withAlpha(hex: string, alpha: number): string {
  const byte = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0");
  return `${hex}${byte}`;
}

/**
 * 套餐徽标的色档（配色在 card.tsx 落 paletteToken，这里只做分类）。
 * 分档只认**等级**，不分厂商：Claude 的 Max、Cursor / Antigravity 的 Ultra、
 * Grok 的 Heavy 同属顶档，因此同色。色档名沿用最初的两档叫法——
 * `ultra` 是顶档、`ultra_lite` 是次顶档，都不代表某家厂商的套餐名。
 */
export type PlanKind =
  | "ultra"
  | "ultra_lite"
  | "pro"
  | "pro_lite"
  | "plus"
  | "lite"
  | "team"
  | "enterprise"
  | "free"
  | "none";

/** 按档位从高到低匹配的词根；先命中的档位决定徽标颜色。 */
const PLAN_LEVEL_WORDS: { kind: PlanKind; words: readonly string[] }[] = [
  { kind: "enterprise", words: ["enterprise"] },
  { kind: "team", words: ["team", "business"] },
  { kind: "ultra", words: ["ultra", "max", "heavy"] },
  { kind: "pro", words: ["pro", "prolite", "professional", "supergrok"] },
  { kind: "plus", words: ["plus", "premium"] },
  { kind: "lite", words: ["lite", "basic", "starter", "standard", "go"] },
  { kind: "free", words: ["free"] },
];

/** 词根表里表示「次顶」的修饰词：顶档带它，或 pro 带「+」，都算 pro 之上、顶档之下。 */
const LOW_TOP_WORDS = ["lite", "5x"] as const;

/** 套餐名切词：小写，按非字母数字（保留 `+`）分段，`G1-ULTRA-LITE-TIER` → g1/ultra/lite/tier。 */
function planTokens(raw: string): string[] {
  return raw
    .toLowerCase()
    .split(/[^a-z0-9+]+/)
    .filter((token) => token.length > 0);
}

/** 词根命中：`plus` 既认 `PLUS` 也认 `Premium+` 这类带加号写法。 */
function hasWord(tokens: string[], word: string): boolean {
  return tokens.some((token) => token === word || token === `${word}+`);
}

/**
 * 套餐归一：大小写、分隔符与空白无关，按 {@link PLAN_LEVEL_WORDS} 从高到低认词根
 * （"Claude Team"、"GLM Coding Max"、"KIRO PRO+" 都认），认不出按中性灰（none）。
 *
 * 次顶档（`ultra_lite`）单独一支：顶档词根再带 `lite` / `5x`（Ultra Lite、
 * `G1-ULTRA-LITE-TIER`、`MAX 5X`），或 pro 带加号（`PRO+`、`PRO_PLUS`）。
 */
const MAX_LOGIC_CACHE = 512;
const planKindCache = new Map<string, PlanKind>();
const formatStampCache = new Map<number, string>();
const planLabelCache = new Map<string, string>();

function setBoundedCache<K, V>(cache: Map<K, V>, key: K, value: V, limit = MAX_LOGIC_CACHE): void {
  if (cache.size >= limit) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, value);
}

/** 从原始套餐名解析视觉档位。先命中高档关键词再判降配词，防止 "Pro Lite" 落到 "Pro"。 */
export function planKind(plan: string | null | undefined): PlanKind {
  const key = (plan ?? "").trim();
  if (key.length === 0) return "none";
  const cached = planKindCache.get(key);
  if (cached !== undefined) return cached;

  const tokens = planTokens(key);
  let resolved: PlanKind = "none";
  if (tokens.length > 0) {
    const word = (...words: readonly string[]) => words.some((w) => hasWord(tokens, w));
    for (const level of PLAN_LEVEL_WORDS) {
      if (!word(...level.words)) continue;
      // 次顶档：顶档的低配版（Ultra Lite / Max 5X），或 pro 的加量版（Pro+ / PRO_PLUS）。
      if (level.kind === "ultra" && word(...LOW_TOP_WORDS)) {
        resolved = "ultra_lite";
        break;
      }
      if (level.kind === "pro" && (word("pro+") || word("plus"))) {
        resolved = "ultra_lite";
        break;
      }
      // pro 的降配（Pro Lite / pro 5x）：同色系软底，和满档 Pro 的实底分得开。
      if (level.kind === "pro" && (word("prolite") || word(...LOW_TOP_WORDS))) {
        resolved = "pro_lite";
        break;
      }
      resolved = level.kind;
      break;
    }
  }
  setBoundedCache(planKindCache, key, resolved);
  return resolved;
}

/** 剩余率分档：≥80 ok、≥40 warn、≥10 橙（amber）、<10 danger。 */
export type RemainingTone = "ok" | "warn" | "amber" | "danger";

export function remainingTone(percent: number): RemainingTone {
  if (percent >= 80) return "ok";
  if (percent >= 40) return "warn";
  if (percent >= 10) return "amber";
  return "danger";
}

/** 0..=100 夹紧（域的契约之外的值也不画出格条）。 */
export function clampPercent(percent: number): number {
  if (!Number.isFinite(percent)) return 0;
  return Math.min(100, Math.max(0, percent));
}

/** 进度条宽度与右侧百分比用同一读数：四舍五入到整数百分比。 */
export function percentText(remainingPercent: number): string {
  return `${Math.round(clampPercent(remainingPercent))}%`;
}

/** 窗口顺序：five_hour → week → 其它（保持域侧顺序）。 */
export function windowOrder(windows: UsageWindow[]): UsageWindow[] {
  const rank = (key: string): number => {
    if (key === "five_hour") return 0;
    if (key === "week") return 1;
    return 2;
  };
  return windows
    .map((window, index) => ({ window, index }))
    .sort((a, b) => {
      const delta = rank(a.window.key) - rank(b.window.key);
      return delta === 0 ? a.index - b.index : delta;
    })
    .map((entry) => entry.window);
}

const BUCKET_SUFFIXES = [" remaining", " left", " available", " limit"] as const;

/** 剥离厂商文案末尾状态词并归一常见周期短名（兼容已落盘快照）。 */
function compactBucketSegment(segment: string): string {
  const trimmed = segment.trim();
  if (trimmed.length === 0) return "";
  const lower = trimmed.toLowerCase();
  let stripped = trimmed;
  for (const suffix of BUCKET_SUFFIXES) {
    if (lower.endsWith(suffix)) {
      stripped = trimmed.slice(0, trimmed.length - suffix.length).trim();
      break;
    }
  }
  if (stripped.length === 0) return "";
  const norm = stripped.toLowerCase();
  if (
    norm === "five hour" ||
    norm === "five-hour" ||
    norm === "5 hour" ||
    norm === "5-hour" ||
    norm === "5 hours"
  ) {
    return "5h";
  }
  if (norm === "weekly" || norm === "week" || norm === "7 day" || norm === "7-day" || norm === "7 days") {
    return "Weekly";
  }
  return stripped;
}

/** 已知窗口 key 的短标签（供应商自己的 label 做兜底与旧快照后缀归一）。 */
export function windowLabel(window: UsageWindow): string {
  if (window.key === "five_hour") return "5h";
  if (window.key === "week") return "Weekly";
  if (window.key === "month") return "Monthly";
  const raw = window.label.trim();
  if (raw.length === 0) return window.key;
  const sep = raw.lastIndexOf(" · ");
  if (sep > 0) {
    const head = raw.slice(0, sep).trim();
    const tail = compactBucketSegment(raw.slice(sep + 3));
    return tail.length > 0 ? `${head} · ${tail}` : head;
  }
  const compacted = compactBucketSegment(raw);
  return compacted.length > 0 ? compacted : raw;
}

/**
 * 窗口 head 右侧的可选 chip（域同时给了 used 与 total 时才有）：紧凑的
 * "已用/总量"，单位随窗口（金额折美元）。没有数值就不画 chip。
 */
export function windowChip(window: UsageWindow): string | null {
  const used = window.used;
  const total = window.total;
  if (used === null || used === undefined || total === null || total === undefined) {
    return null;
  }
  if (window.unit === "usd-cents") {
    return `${amountText(used, window.unit)}/${amountText(total, window.unit)}`;
  }
  if (window.unit === "percent") return null;
  return `${amountText(used, "count")}/${amountText(total, "count")}`;
}

/** 数值文本：count 最多两位小数（去尾零），percent 带 %，usd-cents 折美元。 */
export function amountText(value: number, unit: Credits["unit"]): string {
  if (!Number.isFinite(value)) return "—";
  if (unit === "usd-cents") {
    const sign = value < 0 ? "-" : "";
    return `${sign}$${(Math.abs(value) / 100).toFixed(2)}`;
  }
  if (unit === "percent") return `${Math.round(value)}%`;
  return `${Number(value.toFixed(2))}`;
}

/** credits 行：unlimited 交给调用方出「无限」，其余给数值文本。 */
export function creditText(credits: Credits): { unlimited: boolean; text: string } {
  if (credits.unlimited || credits.remaining === null || credits.remaining === undefined) {
    return { unlimited: credits.unlimited, text: credits.unlimited ? "" : "—" };
  }
  return { unlimited: false, text: amountText(credits.remaining, credits.unit) };
}

/** 将 Credits 对象格式化为直接可展示文本，避免对象插值成 `[object Object]`。 */
export function formatCreditValue(credits: Credits, unlimitedLabel: string): string {
  const value = creditText(credits);
  return value.unlimited ? unlimitedLabel : value.text;
}

/** 剩余时长（"3d 4h" / "11m"）：单位与卡面文案一致，末尾零单位省略。 */
export function formatSpan(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0m";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  if (hours > 0) return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  return `${Math.max(1, minutes)}m`;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/** 重置时刻的日期戳（本地时区）："11/18 09:30"。 */
export function formatStamp(unixSecs: number): string {
  const cached = formatStampCache.get(unixSecs);
  if (cached !== undefined) return cached;
  const date = new Date(unixSecs * 1000);
  if (Number.isNaN(date.getTime())) return "";
  const formatted = `${pad2(date.getMonth() + 1)}/${pad2(date.getDate())} ${pad2(date.getHours())}:${pad2(
    date.getMinutes()
  )}`;
  setBoundedCache(formatStampCache, unixSecs, formatted, 1024);
  return formatted;
}

/** 重置行的三态：无时刻 / 已重置 / 剩余 span + 时刻。 */
export type ResetState =
  | { kind: "none" }
  | { kind: "done" }
  | { kind: "in"; span: string; when: string };

export function resetState(
  resetsAt: number | null | undefined,
  nowSecs: number
): ResetState {
  if (resetsAt === null || resetsAt === undefined || resetsAt <= 0) return { kind: "none" };
  const remaining = resetsAt - nowSecs;
  if (remaining <= 0) return { kind: "done" };
  return { kind: "in", span: formatSpan(remaining), when: formatStamp(resetsAt) };
}

/** 当前秒（组件与 state 共用；测试装置注入假时钟时也读它）。 */
export function nowSecs(): number {
  return Math.floor(Date.now() / 1000);
}

/**
 * 卡片排序：在役在前，其余按域侧顺序（list 的 created_at 升序）稳定排列。
 * 只排序，不改行；切换后重 list 的落点即「当前卡升到首位」。
 */
export function sortAccounts(rows: AccountView[], servingId: string | null): AccountView[] {
  const rank = (row: AccountView): number => (servingId !== null && row.id === servingId ? 0 : 1);
  return rows
    .map((row, index) => ({ row, index }))
    .sort((a, b) => {
      const delta = rank(a.row) - rank(b.row);
      if (delta !== 0) return delta;
      if (a.row.createdAt !== b.row.createdAt) return a.row.createdAt - b.row.createdAt;
      return a.index - b.index;
    })
    .map((entry) => entry.row);
}

/** 某个 agent 此刻在服务的账号 id（reconcile 的读法；读不到给 null）。 */
export function liveAccountId(live: AgentLive[], agent: string): string | null {
  const row = live.find((entry) => entry.agent === agent);
  const id = row?.accountId ?? null;
  return id === null || id.trim().length === 0 ? null : id;
}

/**
 * 卡片的「当前」判定：账号行的 isActive（写入时的磁盘真相）优先，reconcile
 * 的表作为第二读数（客户端被外部改过时仍能标出真正在服务的那张）。
 */
export function isServing(account: AccountView, reconcileId: string | null): boolean {
  return account.isActive || (reconcileId !== null && reconcileId === account.id);
}

/** 可用重置卡数量（如 Codex rate_limit_reset_credits.available_count）；未知时返回 null。 */
export function resetCardCount(usage: UsageSnapshot | null | undefined): number | null {
  const raw = usage?.resetCredits;
  if (typeof raw !== "number" || !Number.isFinite(raw) || raw < 0) return null;
  return Math.floor(raw);
}

/** 额度区是否有可画的窗口、credits 或重置卡（决定空态行）。 */
export function hasQuota(usage: UsageSnapshot | null | undefined): boolean {
  if (usage === null || usage === undefined) return false;
  return (
    usage.windows.length > 0 ||
    (usage.credits !== null && usage.credits !== undefined) ||
    resetCardCount(usage) !== null
  );
}

/** 用户名缺失时不把邮箱、客户端名或账号 ID 当作用户名。 */
export function accountName(account: AccountView): string {
  const name = account.displayName.trim();
  if (!name) return "";
  const placeholders = [account.agent, account.email, account.accountId, account.id];
  return placeholders.some((value) => value?.trim().toLowerCase() === name.toLowerCase()) ? "" : name;
}

/** 操作提示中的账号标识：email 优先，其次上游 accountId，最后退回内部 id。 */
export function loginLine(account: AccountView): string {
  const email = (account.email ?? "").trim();
  if (email.length > 0) return email;
  const accountId = (account.accountId ?? "").trim();
  if (accountId.length > 0) return accountId;
  return account.id;
}

/** 卡片顶部套餐文案：usage.plan 优先（实时读到的），退账号行上的 plan。 */
export function planOf(account: AccountView): string {
  const live = (account.usage?.plan ?? "").trim();
  if (live.length > 0) return live;
  return (account.plan ?? "").trim();
}

/** provider 套餐代号形态：字母数字分段加分隔符（`G1-ULTRA-LITE-TIER`、`standard-tier`）。 */
const TIER_CODE = /^[a-z0-9]+(?:[-_][a-z0-9]+)+$/i;

/**
 * Antigravity tier 代号 → 徽标文案。代号只写档位和倍率，卡片上要的是用户认得的
 * 那一层：Ultra 是 20 倍，Ultra Lite 是 5 倍。
 */
const AG_TIER_LABELS: Record<string, string> = {
  "G1-ULTRA-TIER": "ULTRA 20X",
  "G1-ULTRA-LITE-TIER": "ULTRA 5X",
};

/**
 * 套餐徽标文案：Antigravity 的 tier 代号按 {@link AG_TIER_LABELS} 换成同一套短名
 * （`G1-ULTRA-TIER` → `ULTRA 20X`、`G1-ULTRA-LITE-TIER` → `ULTRA 5X`）；
 * 表里没有的代号去掉世代前缀 `G1` 和后缀 `TIER`、分隔符转空格再大写
 * （`standard-tier` → `STANDARD`）；其余账号与已是人话的套餐名原样返回。
 */
export function planLabel(account: AccountView): string {
  const plan = planOf(account);
  if (plan.length === 0) return plan;
  const isAntigravity = account.agent === "antigravity";
  const cacheKey = `${isAntigravity ? "1" : "0"}:${plan}`;
  const cached = planLabelCache.get(cacheKey);
  if (cached !== undefined) return cached;

  let formatted = plan;
  // 上游 slug 里的拼接词补个空格，徽标不读成 "PROLITE"。
  if (/^prolite$/i.test(plan)) {
    formatted = "PRO LITE";
  } else if (isAntigravity) {
    const known = AG_TIER_LABELS[plan.toUpperCase()];
    if (known) {
      formatted = known;
    } else if (TIER_CODE.test(plan)) {
      const words = plan
        .split(/[-_\s]+/)
        .filter((word) => word.length > 0)
        .filter((word, index) => !(index === 0 && /^g\d+$/i.test(word)))
        .filter((word) => word.toLowerCase() !== "tier");
      if (words.length > 0) {
        formatted = words.join(" ").toUpperCase();
      }
    }
  }
  setBoundedCache(planLabelCache, cacheKey, formatted);
  return formatted;
}

/** 隐私打码：开启后对邮箱本地部分或账号名称/ID 脱敏，关闭时原样返回。 */
export function maskAccountText(value: string, masked: boolean): string {
  if (!masked) return value;
  const trimmed = value.trim();
  if (trimmed.length === 0) return value;
  const at = trimmed.indexOf("@");
  if (at > 0) {
    const local = trimmed.slice(0, at);
    const domain = trimmed.slice(at);
    const keep = local.length <= 4 ? 1 : 4;
    return `${local.slice(0, keep)}***${domain}`;
  }
  if (trimmed.length <= 3) return `${trimmed.slice(0, 1)}***`;
  if (trimmed.length <= 6) return `${trimmed.slice(0, 2)}***${trimmed.slice(-1)}`;
  return `${trimmed.slice(0, 3)}***${trimmed.slice(-2)}`;
}

/** 综合配额（0..100 整数均值；无限 credits 且无窗口给 100；无数据给 -1）。 */
export function overallQuota(account: AccountView): number {
  const windows = account.usage?.windows ?? [];
  if (windows.length > 0) {
    const sum = windows.reduce((acc, w) => acc + clampPercent(w.remainingPercent), 0);
    return Math.round(sum / windows.length);
  }
  if (account.usage?.credits?.unlimited) return 100;
  return -1;
}

export type AccountViewMode = "compact" | "list" | "grid";
export type AccountFilter =
  | "all"
  | "ultra"
  | "pro"
  | "plus"
  | "lite"
  | "team"
  | "enterprise"
  | "free"
  | "reauth"
  | "error";
export type AccountSortBy = "default" | "quota" | "last_used" | "created_at";
export type SortDirection = "desc" | "asc";

export function matchesAccountFilter(row: AccountView, filter: AccountFilter): boolean {
  if (filter === "all") return true;
  if (filter === "reauth") return row.requiresReauth;
  if (filter === "error") return (row.usage?.error ?? "").trim().length > 0;
  const kind = planKind(planOf(row));
  // 「Ultra」筛选覆盖两档 Ultra：20X 与 5X 只在徽标上分色，档位仍属 Ultra。
  if (filter === "ultra") return kind === "ultra" || kind === "ultra_lite";
  return kind === filter;
}

function matchesNeedle(row: AccountView, needle: string): boolean {
  return (
    row.displayName.toLowerCase().includes(needle) ||
    (row.email !== null && row.email !== undefined && row.email.toLowerCase().includes(needle)) ||
    (row.accountId !== null &&
      row.accountId !== undefined &&
      row.accountId.toLowerCase().includes(needle)) ||
    row.id.toLowerCase().includes(needle) ||
    planOf(row).toLowerCase().includes(needle)
  );
}

/** 先按筛选与关键字过滤，再排序（在役账号优先，再按配额/最近使用/创建时间）。 */
export function filterAndSortAccounts(
  rows: AccountView[],
  opts: {
    query: string;
    filter: AccountFilter;
    sortBy: AccountSortBy;
    sortDir: SortDirection;
    servingId: string | null;
  },
): AccountView[] {
  const needle = opts.query.trim().toLowerCase();
  const hasFilter = opts.filter !== "all";
  const hasNeedle = needle.length > 0;
  const filtered: AccountView[] = [];
  for (const row of rows) {
    if (hasFilter && !matchesAccountFilter(row, opts.filter)) continue;
    if (hasNeedle && !matchesNeedle(row, needle)) continue;
    filtered.push(row);
  }
  if (filtered.length <= 1) return filtered;

  const dirMul = opts.sortDir === "desc" ? -1 : 1;
  const needQuota = opts.sortBy === "quota";
  const decorated = filtered.map((row, index) => ({
    row,
    index,
    servingRank: isServing(row, opts.servingId) ? 0 : 1,
    quota: needQuota ? overallQuota(row) : 0,
  }));

  decorated.sort((a, b) => {
    if (opts.sortBy === "default") {
      if (a.servingRank !== b.servingRank) return a.servingRank - b.servingRank;
      if (a.row.createdAt !== b.row.createdAt) {
        return (a.row.createdAt - b.row.createdAt) * (opts.sortDir === "asc" ? 1 : -1);
      }
      return a.index - b.index;
    }
    if (opts.sortBy === "quota") {
      if (a.quota < 0 && b.quota >= 0) return 1;
      if (b.quota < 0 && a.quota >= 0) return -1;
      if (a.quota !== b.quota) return (a.quota - b.quota) * dirMul;
    } else if (opts.sortBy === "last_used") {
      const ta = a.row.lastUsedAt ?? a.row.createdAt;
      const tb = b.row.lastUsedAt ?? b.row.createdAt;
      if (ta !== tb) return (ta - tb) * dirMul;
    } else if (opts.sortBy === "created_at") {
      if (a.row.createdAt !== b.row.createdAt) {
        return (a.row.createdAt - b.row.createdAt) * dirMul;
      }
    }
    if (a.servingRank !== b.servingRank) return a.servingRank - b.servingRank;
    if (a.row.createdAt !== b.row.createdAt) return a.row.createdAt - b.row.createdAt;
    return a.index - b.index;
  });

  return decorated.map((entry) => entry.row);
}

/** 336px 卡片双列半宽下，顶行（标签+chip）不截断周期后缀的字符预算。 */
export const MAX_PAIRED_HEAD_CHARS = 16;

/** 窗口顶行左侧文案总字符长度（标签 + 可选 chip 及其间距）。 */
export function windowHeadLength(window: UsageWindow): number {
  const chip = windowChip(window);
  return windowLabel(window).length + (chip === null ? 0 : chip.length + 2);
}

/**
 * 卡片内框窗口分组：先按 five_hour → week → 其它排序；相邻两项均紧凑时并排
 * 双列，长标签或带超宽 chip 的窗口独占单列，避免半宽下把周期后缀截掉。
 */
export function windowRows(windows: UsageWindow[]): UsageWindow[][] {
  if (windows.length === 0) return [];
  if (windows.length === 1) return [[windows[0]]];
  const ordered = windowOrder(windows);
  const rows: UsageWindow[][] = [];
  for (let i = 0; i < ordered.length; i += 1) {
    const current = ordered[i];
    const next = ordered[i + 1];
    if (
      next !== undefined &&
      windowHeadLength(current) <= MAX_PAIRED_HEAD_CHARS &&
      windowHeadLength(next) <= MAX_PAIRED_HEAD_CHARS
    ) {
      rows.push([current, next]);
      i += 1;
    } else {
      rows.push([current]);
    }
  }
  return rows;
}

export function countsEqual(a: Record<string, number>, b: Record<string, number>): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}

function usageEqual(a: UsageSnapshot | null | undefined, b: UsageSnapshot | null | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (
    a.fetchedAt !== b.fetchedAt ||
    a.plan !== b.plan ||
    a.error !== b.error ||
    (a.resetCredits ?? null) !== (b.resetCredits ?? null)
  ) {
    return false;
  }
  if (a.windows.length !== b.windows.length) return false;
  for (let i = 0; i < a.windows.length; i += 1) {
    const wa = a.windows[i];
    const wb = b.windows[i];
    if (
      wa.key !== wb.key ||
      wa.label !== wb.label ||
      wa.remainingPercent !== wb.remainingPercent ||
      wa.used !== wb.used ||
      wa.total !== wb.total ||
      wa.unit !== wb.unit ||
      wa.resetsAt !== wb.resetsAt
    ) {
      return false;
    }
  }
  const ca = a.credits;
  const cb = b.credits;
  if (ca === cb) return true;
  if (!ca || !cb) return false;
  return (
    ca.label === cb.label &&
    ca.remaining === cb.remaining &&
    ca.unlimited === cb.unlimited &&
    ca.unit === cb.unit
  );
}

export function accountViewEqual(a: AccountView, b: AccountView): boolean {
  if (a === b) return true;
  return (
    a.id === b.id &&
    a.agent === b.agent &&
    a.displayName === b.displayName &&
    a.email === b.email &&
    a.accountId === b.accountId &&
    a.plan === b.plan &&
    a.createdAt === b.createdAt &&
    a.lastUsedAt === b.lastUsedAt &&
    a.requiresReauth === b.requiresReauth &&
    a.isActive === b.isActive &&
    usageEqual(a.usage, b.usage)
  );
}

export function accountsEqual(a: AccountView[], b: AccountView[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (!accountViewEqual(a[i], b[i])) return false;
  }
  return true;
}

export function liveEqual(a: AgentLive[], b: AgentLive[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const la = a[i];
    const lb = b[i];
    if (
      la.agent !== lb.agent ||
      la.identity !== lb.identity ||
      la.displayName !== lb.displayName ||
      la.accountId !== lb.accountId ||
      la.source !== lb.source
    ) {
      return false;
    }
  }
  return true;
}

/** 列表整包刷新时保留未变动行对象引用，降低下游 signal 与视图重算范围。 */
export function mergeAccountSnapshots(prev: AccountView[], next: AccountView[]): AccountView[] {
  if (prev.length === 0) return next;
  if (accountsEqual(prev, next)) return prev;
  const prevById = new Map(prev.map((row) => [row.id, row]));
  return next.map((row) => {
    const existing = prevById.get(row.id);
    return existing !== undefined && accountViewEqual(existing, row) ? existing : row;
  });
}

export function applyAccountPatches(
  rows: AccountView[],
  patches: ReadonlyMap<string, AccountView>,
): AccountView[] {
  if (patches.size === 0) return rows;
  let changed = false;
  const next = rows.map((row) => {
    const patch = patches.get(row.id);
    if (patch === undefined || accountViewEqual(row, patch)) return row;
    changed = true;
    return patch;
  });
  return changed ? next : rows;
}

export function applyActivatedOutcome(
  rows: AccountView[],
  outcome: ActivateOutcome,
): AccountView[] {
  const targetId = outcome.account.id;
  const agent = outcome.account.agent;
  const nextTarget: AccountView = outcome.account.isActive
    ? outcome.account
    : { ...outcome.account, isActive: true };
  let found = false;
  let changed = false;
  const next = rows.map((row) => {
    if (row.id === targetId) {
      found = true;
      if (!accountViewEqual(row, nextTarget)) {
        changed = true;
        return nextTarget;
      }
      return row;
    }
    if (row.agent === agent && row.isActive) {
      changed = true;
      return { ...row, isActive: false };
    }
    return row;
  });
  if (!found) return [...next, nextTarget];
  return changed ? next : rows;
}

export function upsertAccountRow(rows: AccountView[], created: AccountView): AccountView[] {
  let found = false;
  const next = rows.map((row) => {
    if (row.id === created.id) {
      found = true;
      return created;
    }
    if (created.isActive && row.agent === created.agent && row.isActive) {
      return { ...row, isActive: false };
    }
    return row;
  });
  if (!found) return [...next, created];
  return next;
}

export function upsertLiveFromAccount(
  liveRows: AgentLive[],
  account: AccountView,
  targetIdentity?: string | null,
): AgentLive[] {
  const existingIndex = liveRows.findIndex((entry) => entry.agent === account.agent);
  const existing = existingIndex >= 0 ? liveRows[existingIndex] : undefined;
  const identity =
    (targetIdentity && targetIdentity.trim().length > 0 ? targetIdentity : null) ??
    account.email ??
    account.displayName ??
    null;
  const updated: AgentLive = {
    agent: account.agent,
    identity,
    displayName: account.displayName,
    accountId: account.accountId ?? null,
    source: existing?.source ?? "switch",
  };
  if (existingIndex < 0) return [...liveRows, updated];
  if (
    existing !== undefined &&
    existing.identity === updated.identity &&
    existing.displayName === updated.displayName &&
    existing.accountId === updated.accountId &&
    existing.source === updated.source
  ) {
    return liveRows;
  }
  const next = liveRows.slice();
  next[existingIndex] = updated;
  return next;
}

export function orderAccountsServingFirst(
  rows: AccountView[],
  liveRows: AgentLive[],
): AccountView[] {
  const liveByAgent = new Map<string, string | null>();
  const serving: AccountView[] = [];
  const standby: AccountView[] = [];
  for (const row of rows) {
    let servingId = liveByAgent.get(row.agent);
    if (servingId === undefined) {
      servingId = liveAccountId(liveRows, row.agent);
      liveByAgent.set(row.agent, servingId);
    }
    if (isServing(row, servingId)) {
      serving.push(row);
    } else {
      standby.push(row);
    }
  }
  return [...serving, ...standby];
}

export function findRolledBackAccountIds(
  prevRows: AccountView[],
  nextRows: AccountView[],
): string[] {
  if (prevRows.length === 0 || nextRows.length === 0) return [];
  const prevById = new Map(prevRows.map((row) => [row.id, row]));
  const rolledBackIds: string[] = [];
  for (const nextRow of nextRows) {
    const prevRow = prevById.get(nextRow.id);
    if (prevRow === undefined) continue;
    const prevFetched = prevRow.usage?.fetchedAt;
    const nextFetched = nextRow.usage?.fetchedAt ?? 0;
    const errorCleared =
      (prevRow.usage?.error ?? "") !== "" && (nextRow.usage?.error ?? "") === "";
    if ((prevFetched !== undefined && prevFetched > nextFetched) || errorCleared) {
      rolledBackIds.push(nextRow.id);
    }
  }
  return rolledBackIds;
}

export function takeNextDistinctAgentAccount(
  queue: AccountView[],
  inFlightAgents: ReadonlySet<string>,
): AccountView | null {
  for (let i = 0; i < queue.length; i += 1) {
    const candidate = queue[i];
    if (!inFlightAgents.has(candidate.agent)) {
      queue.splice(i, 1);
      return candidate;
    }
  }
  return null;
}

export function withRequiresReauth(rows: AccountView[], accountId: string): AccountView | null {
  const row = rows.find((entry) => entry.id === accountId);
  if (row === undefined || row.requiresReauth) return null;
  return { ...row, requiresReauth: true };
}

/** 将数组按 2 个一组切分，用于卡片内框双列配额行。 */
export function chunkPairs<T>(items: T[]): T[][] {
  const pairs: T[][] = [];
  for (let i = 0; i < items.length; i += 2) {
    pairs.push(items.slice(i, i + 2));
  }
  return pairs;
}

/** 最近使用（优先 lastUsedAt，退回 createdAt）；<=0 时返回 null。 */
export function lastUsedStamp(account: AccountView): string | null {
  const ts = account.lastUsedAt ?? account.createdAt;
  if (!Number.isFinite(ts) || ts <= 0) return null;
  const stamp = formatStamp(ts);
  return stamp.length > 0 ? stamp : null;
}
