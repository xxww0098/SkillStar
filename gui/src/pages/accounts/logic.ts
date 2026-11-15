// 账号页的纯逻辑 —— 卡片配色档位、额度窗口排序、进度条数值、重置倒计时与
// 账号排序。全部无副作用、不读 signal（主题色由调用方 paletteToken 现取），
// 因此可以直接单测（logic 层的既有纪律：nsh 测试装置的假桥只覆盖交互，
// 纯语义在这一层钉死）。

import type {
  AccountView,
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

/** 套餐徽标的色档（配色在 card.tsx 落 paletteToken，这里只做分类）。 */
export type PlanKind =
  | "ultra"
  | "pro"
  | "plus"
  | "team"
  | "enterprise"
  | "free"
  | "none";

/**
 * 套餐归一：大小写/空白无关，含对应词根即归该档（"Pro 20x"、"Claude Team"
 * 都认），其余按中性灰（none）。先查 enterprise/ultra/team，避免复合词串档。
 */
export function planKind(plan: string | null | undefined): PlanKind {
  const raw = (plan ?? "").trim().toLowerCase();
  if (raw.length === 0) return "none";
  if (raw.includes("enterprise")) return "enterprise";
  if (raw.includes("ultra")) return "ultra";
  if (raw.includes("team")) return "team";
  if (raw.includes("plus")) return "plus";
  if (raw.includes("pro")) return "pro";
  if (raw.includes("free")) return "free";
  return "none";
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

/** 已知窗口 key 的短标签（供应商自己的 label 只做兜底）。 */
export function windowLabel(window: UsageWindow): string {
  if (window.key === "five_hour") return "5h";
  if (window.key === "week") return "Weekly";
  if (window.key === "month") return "Monthly";
  const label = window.label.trim();
  return label.length === 0 ? window.key : label;
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
  const date = new Date(unixSecs * 1000);
  if (Number.isNaN(date.getTime())) return "";
  return `${pad2(date.getMonth() + 1)}/${pad2(date.getDate())} ${pad2(date.getHours())}:${pad2(
    date.getMinutes()
  )}`;
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

/** 额度区是否有可画的窗口或 credits（决定空态行）。 */
export function hasQuota(usage: UsageSnapshot | null | undefined): boolean {
  if (usage === null || usage === undefined) return false;
  return usage.windows.length > 0 || (usage.credits !== null && usage.credits !== undefined);
}

/** 账号行的次要身份行：email 优先，退账号 id。 */
export function loginLine(account: AccountView): string {
  const email = (account.email ?? "").trim();
  if (email.length > 0) return email;
  return account.id;
}

/** 卡片顶部套餐文案：usage.plan 优先（实时读到的），退账号行上的 plan。 */
export function planOf(account: AccountView): string {
  const live = (account.usage?.plan ?? "").trim();
  if (live.length > 0) return live;
  return (account.plan ?? "").trim();
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
  return planKind(planOf(row)) === filter;
}

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
  const filtered = rows.filter((row) => {
    if (!matchesAccountFilter(row, opts.filter)) return false;
    if (needle.length === 0) return true;
    const haystack = [row.displayName, row.email ?? "", row.id, planOf(row)]
      .join("\0")
      .toLowerCase();
    return haystack.includes(needle);
  });

  const dirMul = opts.sortDir === "desc" ? -1 : 1;
  return filtered
    .map((row, index) => ({ row, index }))
    .sort((a, b) => {
      if (opts.sortBy === "default") {
        const ra = isServing(a.row, opts.servingId) ? 0 : 1;
        const rb = isServing(b.row, opts.servingId) ? 0 : 1;
        if (ra !== rb) return ra - rb;
        if (a.row.createdAt !== b.row.createdAt) {
          return (a.row.createdAt - b.row.createdAt) * (opts.sortDir === "asc" ? 1 : -1);
        }
        return a.index - b.index;
      }
      if (opts.sortBy === "quota") {
        const qa = overallQuota(a.row);
        const qb = overallQuota(b.row);
        if (qa < 0 && qb >= 0) return 1;
        if (qb < 0 && qa >= 0) return -1;
        if (qa !== qb) return (qa - qb) * dirMul;
      } else if (opts.sortBy === "last_used") {
        const ta = a.row.lastUsedAt ?? a.row.createdAt;
        const tb = b.row.lastUsedAt ?? b.row.createdAt;
        if (ta !== tb) return (ta - tb) * dirMul;
      } else if (opts.sortBy === "created_at") {
        if (a.row.createdAt !== b.row.createdAt) {
          return (a.row.createdAt - b.row.createdAt) * dirMul;
        }
      }
      const sa = isServing(a.row, opts.servingId) ? 0 : 1;
      const sb = isServing(b.row, opts.servingId) ? 0 : 1;
      if (sa !== sb) return sa - sb;
      if (a.row.createdAt !== b.row.createdAt) return a.row.createdAt - b.row.createdAt;
      return a.index - b.index;
    })
    .map((entry) => entry.row);
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
