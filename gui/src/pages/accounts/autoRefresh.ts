// 账号页错峰自动刷新调度器（Staggered Round-Robin）。
//
// 语义：用户选择的周期 T（如 30s）代表「单个可用账号的刷新周期」。
// 调度器按当前可用账号数 N 动态切分步进：Δt = max(T / N, minGap) ± jitter，
// 每次只从队列里挑选「最久未刷新（LRU）」的 1 个账号发起请求，与手动刷新共享
// refreshingIds 单飞锁，并在遇到 429 / 403 / 瞬时错误时保留旧配额并局部冷却。
//
// 【持久化】周期跨重启保存在 gui_prefs.json 的 accounts_auto_refresh_ms
// （./prefs.ts 的接缝 → native Settings facade）。这里不碰 localStorage：
// nsh 的 JS 环境没有 DOM（gui/.nsh/tsconfig.json 的 lib 只有 ES2023），
// 浏览器式存储连类型检查都过不去。
//
// 【窗口隐藏不暂停】nsh 没有 document.visibility，而关闭主窗口只隐藏窗口、
// 进程与后台唤醒继续（D-115）——自动刷新正是该继续跑的那类工作。

import { signal, type Signal } from "nsh";
import type { AutoRefreshPrefs } from "./prefs";
import type { AccountView } from "./types";

export const AUTO_REFRESH_OPTIONS: readonly (number | null)[] = [
  null,
  30_000,
  60_000,
  120_000,
  300_000,
  900_000,
];

export const DEFAULT_AUTO_REFRESH_MS = 300_000;
export const DEFAULT_STALE_MAX_AGE_SECS = 60;
export const DEFAULT_MIN_GAP_MS = 2_000;
export const DEFAULT_JITTER_RATIO = 0.15;
export const DEFAULT_BASE_BACKOFF_MS = 60_000;
export const DEFAULT_MAX_BACKOFF_MS = 600_000;
export const DEFAULT_TRANSIENT_BACKOFF_MS = 15_000;

const QUICK_CYCLE_RING: readonly (number | null)[] = [
  60_000,
  300_000,
  900_000,
  null,
];

export type RefreshErrorKind = "rate_limited" | "auth_required" | "transient";

export interface MarkErrorOptions {
  /** 是否允许将该错误判定为重授权并停止后台轮询；resolved 的快照降级错误必须为 false。 */
  allowAuthRequired?: boolean;
}

export interface AutoRefreshSlot {
  lastRefreshedAt: number | null;
  lastAttemptedAt?: number | null;
  cooldownUntil: number | null;
  consecutiveErrors: number;
  isForbidden: boolean;
}

export interface AutoRefreshScheduler {
  readonly intervalMs: Signal<number | null>;
  readonly nextTickAt: Signal<number | null>;
  readonly slots: Signal<Record<string, AutoRefreshSlot>>;
  setIntervalMs(ms: number | null): void;
  markRefreshed(accountId: string, atMs?: number): void;
  markError(accountId: string, err: unknown, atMs?: number, opts?: MarkErrorOptions): void;
  forgetSlots(accountIds: readonly string[]): void;
  reschedule(): void;
  dispose(): void;
}

export interface BatchPatchFlusher<T> {
  enqueue(id: string, updated: T | null): void;
  flush(): void;
}

/** 批量刷新/后台补刷结果按窗口合并提交，降低高频信号写入引起的重算与视图抖动。 */
export function createBatchPatchFlusher<T extends { id: string }>(opts: {
  delayMs?: number;
  applyPatches(patches: ReadonlyMap<string, T>): void;
  completeIds(ids: ReadonlySet<string>): void;
  onFlush?(): void;
}): BatchPatchFlusher<T> {
  const pendingPatches = new Map<string, T>();
  const pendingCompletedIds = new Set<string>();
  const delayMs = opts.delayMs ?? 120;
  let flushTimer: ReturnType<typeof setTimeout> | null = null;

  function flush(): void {
    if (flushTimer !== null) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    if (pendingPatches.size > 0) {
      opts.applyPatches(pendingPatches);
      pendingPatches.clear();
    }
    if (pendingCompletedIds.size > 0) {
      opts.completeIds(pendingCompletedIds);
      pendingCompletedIds.clear();
    }
    opts.onFlush?.();
  }

  function enqueue(id: string, updated: T | null): void {
    if (updated !== null) {
      pendingPatches.set(updated.id, updated);
    }
    pendingCompletedIds.add(id);
    if (flushTimer === null) {
      flushTimer = setTimeout(flush, delayMs);
    }
  }

  return { enqueue, flush };
}

/** 只接受 AUTO_REFRESH_OPTIONS 里的周期：选项表改过或手改过文件的旧值一律按关闭处理。 */
export function normalizeAutoRefreshMs(raw: number | null): number | null {
  return raw !== null && AUTO_REFRESH_OPTIONS.includes(raw) ? raw : null;
}

/** 下拉窗底栏快捷切换下一档自动刷新周期（关闭态点击直接开启默认 5 分钟档）。 */
export function nextQuickAutoRefreshMs(current: number | null): number | null {
  if (current === null) return DEFAULT_AUTO_REFRESH_MS;
  const idx = QUICK_CYCLE_RING.indexOf(current);
  if (idx < 0) return DEFAULT_AUTO_REFRESH_MS;
  return QUICK_CYCLE_RING[(idx + 1) % QUICK_CYCLE_RING.length];
}

export function classifyRefreshError(err: unknown): RefreshErrorKind {
  const msg = String(err).toLowerCase();
  if (
    msg.includes("429") ||
    msg.includes("rate limit") ||
    msg.includes("rate_limit") ||
    msg.includes("too many requests") ||
    msg.includes("quota exceeded")
  ) {
    return "rate_limited";
  }
  // 严格与 Rust 后端 AuthRequired（401 / invalid_grant / credential rejected / login required）对齐；
  // HTTP 403 / forbidden 按 usage/fetch/mod.rs:171-177 属于 Unreadable，不得标记重授权。
  if (
    msg.includes("login required:") ||
    msg.includes("credential rejected:") ||
    /\b401\b/.test(msg) ||
    msg.includes("unauthorized") ||
    msg.includes("invalid_grant") ||
    msg.includes("reauth")
  ) {
    return "auth_required";
  }
  return "transient";
}

export function computeBackoffMs(
  kind: Exclude<RefreshErrorKind, "auth_required">,
  consecutiveErrors: number,
): number {
  const baseMs =
    kind === "rate_limited" ? DEFAULT_BASE_BACKOFF_MS : DEFAULT_TRANSIENT_BACKOFF_MS;
  const exp = Math.max(0, Math.min(consecutiveErrors - 1, 10));
  return Math.min(baseMs * 2 ** exp, DEFAULT_MAX_BACKOFF_MS);
}

export function isEligibleForAutoRefresh(
  account: AccountView,
  slot: AutoRefreshSlot | undefined,
  nowMs: number,
  inFlightIds: ReadonlySet<string>,
): boolean {
  if (account.requiresReauth) return false;
  if (slot?.isForbidden) return false;
  if (inFlightIds.has(account.id)) return false;
  if (slot?.cooldownUntil !== null && slot?.cooldownUntil !== undefined && slot.cooldownUntil > nowMs) {
    return false;
  }
  return true;
}

/**
 * 筛选需要静默补刷的过期账号：
 * - 跳过需重授权、401/403 钉住、退避冷却中或已在途的账号；
 * - 距上次成功刷新（优先内存 `lastRefreshedAt`，回退快照 `fetchedAt`）达到 `maxAgeSecs`
 *   或尚无有效时间戳时纳入返回列表，并保留传入候选顺序（调用方将在役账号置前）。
 */
export function pickStaleAccounts(
  candidates: readonly AccountView[],
  slots: Readonly<Record<string, AutoRefreshSlot>>,
  nowMs: number,
  inFlightIds: ReadonlySet<string>,
  maxAgeSecs = DEFAULT_STALE_MAX_AGE_SECS,
): string[] {
  const cutoffMs = nowMs - Math.max(0, maxAgeSecs) * 1000;
  const out: string[] = [];
  for (const account of candidates) {
    const slot = slots[account.id];
    if (!isEligibleForAutoRefresh(account, slot, nowMs, inFlightIds)) continue;
    const lastMs =
      slot?.lastRefreshedAt ??
      (account.usage?.fetchedAt && account.usage.fetchedAt > 0
        ? account.usage.fetchedAt * 1000
        : null);
    if (lastMs === null || lastMs <= cutoffMs) {
      out.push(account.id);
    }
  }
  return out;
}

/** LRU 选人：从未尝试且未刷新过的优先，其次按最近尝试/刷新时刻（或快照 fetchedAt）最早者优先。 */
export function pickNextLruAccount(
  candidates: readonly AccountView[],
  slots: Readonly<Record<string, AutoRefreshSlot>>,
  nowMs: number,
  inFlightIds: ReadonlySet<string>,
): string | null {
  let bestId: string | null = null;
  let oldestAt = Infinity;

  for (const account of candidates) {
    const slot = slots[account.id];
    if (!isEligibleForAutoRefresh(account, slot, nowMs, inFlightIds)) continue;
    const attemptedAt = slot?.lastAttemptedAt ?? null;
    const refreshedAt = slot?.lastRefreshedAt ?? null;
    const slotAt =
      attemptedAt !== null && refreshedAt !== null
        ? Math.max(attemptedAt, refreshedAt)
        : (attemptedAt ?? refreshedAt);
    const knownAt =
      slotAt ??
      (account.usage?.fetchedAt && account.usage.fetchedAt > 0
        ? account.usage.fetchedAt * 1000
        : null);
    const rank = knownAt ?? -1;
    if (rank < oldestAt) {
      oldestAt = rank;
      bestId = account.id;
    }
  }
  return bestId;
}

/** 计算本次步进间隔：Δt = max(T / N, minGap) * (1 ± jitter)。 */
export function computeStaggeredDelayMs(
  cycleMs: number,
  eligibleCount: number,
  opts?: {
    minGapMs?: number;
    jitterRatio?: number;
    random?: () => number;
  },
): number {
  const minGapMs = opts?.minGapMs ?? DEFAULT_MIN_GAP_MS;
  const jitterRatio = opts?.jitterRatio ?? DEFAULT_JITTER_RATIO;
  const rand = opts?.random ?? Math.random;

  const divisor = Math.max(1, eligibleCount);
  const baseMs = Math.max(cycleMs / divisor, minGapMs);
  const factor = jitterRatio > 0 ? 1 + (rand() * 2 - 1) * jitterRatio : 1;
  return Math.max(500, Math.round(baseMs * factor));
}

export function createAutoRefreshScheduler(deps: {
  /** 周期的持久化接缝（gui_prefs.json 的 accounts_auto_refresh_ms）。 */
  prefs: AutoRefreshPrefs;
  getCandidates: () => AccountView[];
  getInFlightIds: () => string[];
  onTickAccount: (accountId: string) => Promise<void>;
  minGapMs?: number;
  jitterRatio?: number;
}): AutoRefreshScheduler {
  const intervalMs = signal<number | null>(normalizeAutoRefreshMs(deps.prefs.load()));
  const nextTickAt = signal<number | null>(null);
  const slots = signal<Record<string, AutoRefreshSlot>>({});

  let timer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;
  let scheduledMode: "active" | "cooldown" | null = null;
  let scheduledCycleMs: number | null = null;
  let scheduledEligibleCount = 0;

  function clearTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    scheduledMode = null;
    scheduledCycleMs = null;
    scheduledEligibleCount = 0;
    if (nextTickAt() !== null) {
      nextTickAt.set(null);
    }
  }

  function scheduleTimer(
    mode: "active" | "cooldown",
    cycle: number,
    eligibleCount: number,
    delayMs: number,
    tickAt: number,
  ): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    scheduledMode = mode;
    scheduledCycleMs = cycle;
    scheduledEligibleCount = eligibleCount;
    nextTickAt.set(tickAt);

    timer = setTimeout(() => {
      timer = null;
      scheduledMode = null;
      scheduledCycleMs = null;
      scheduledEligibleCount = 0;
      if (disposed) return;

      const tickNow = Date.now();
      const targetId = pickNextLruAccount(
        deps.getCandidates(),
        slots(),
        tickNow,
        new Set(deps.getInFlightIds()),
      );
      if (targetId !== null) {
        void deps.onTickAccount(targetId);
      }
      scheduleNext(true);
    }, delayMs);
  }

  function scheduleNext(force = false): void {
    if (disposed) {
      clearTimer();
      return;
    }

    const cycle = intervalMs();
    if (cycle === null || cycle <= 0) {
      clearTimer();
      return;
    }

    const nowMs = Date.now();
    const candidates = deps.getCandidates();
    const slotMap = slots();

    let activeCount = 0;
    let coolingCount = 0;
    let earliestCooldownUntil = Infinity;
    for (const acc of candidates) {
      const slot = slotMap[acc.id];
      if (acc.requiresReauth || slot?.isForbidden) continue;
      if (slot?.cooldownUntil !== null && slot?.cooldownUntil !== undefined && slot.cooldownUntil > nowMs) {
        coolingCount += 1;
        if (slot.cooldownUntil < earliestCooldownUntil) {
          earliestCooldownUntil = slot.cooldownUntil;
        }
      } else {
        activeCount += 1;
      }
    }

    if (activeCount === 0 && coolingCount === 0) {
      clearTimer();
      return;
    }

    if (activeCount > 0) {
      // 仍在倒计时且周期与可用数均未变化时保留现有定时器，避免刷卡期间反复清空 nextTickAt。
      if (
        !force &&
        timer !== null &&
        scheduledMode === "active" &&
        scheduledCycleMs === cycle &&
        scheduledEligibleCount === activeCount
      ) {
        return;
      }

      const delayMs = computeStaggeredDelayMs(cycle, activeCount, {
        minGapMs: deps.minGapMs,
        jitterRatio: deps.jitterRatio,
      });
      scheduleTimer("active", cycle, activeCount, delayMs, nowMs + delayMs);
      return;
    }

    const existingTickAt = nextTickAt();
    if (!force && timer !== null && existingTickAt !== null && existingTickAt >= earliestCooldownUntil) {
      return;
    }
    const wakeAt = Math.max(nowMs + 500, earliestCooldownUntil);
    scheduleTimer("cooldown", cycle, 0, wakeAt - nowMs, wakeAt);
  }

  return {
    intervalMs,
    nextTickAt,
    slots,
    setIntervalMs(ms: number | null) {
      // 落盘的值和内存里的值走同一条校验：文件里只可能出现选项表里的周期。
      const next = normalizeAutoRefreshMs(ms);
      if (intervalMs() !== next) {
        intervalMs.set(next);
      }
      deps.prefs.save(next);
      scheduleNext(true);
    },
    markRefreshed(accountId: string, atMs = Date.now()) {
      slots.set({
        ...slots(),
        [accountId]: {
          lastRefreshedAt: atMs,
          lastAttemptedAt: atMs,
          cooldownUntil: null,
          consecutiveErrors: 0,
          isForbidden: false,
        },
      });
      scheduleNext(false);
    },
    markError(accountId: string, err: unknown, atMs = Date.now(), opts?: MarkErrorOptions) {
      const cur = slots()[accountId] ?? {
        lastRefreshedAt: null,
        lastAttemptedAt: null,
        cooldownUntil: null,
        consecutiveErrors: 0,
        isForbidden: false,
      };
      const kind = classifyRefreshError(err);
      if (kind === "auth_required" && opts?.allowAuthRequired !== false) {
        slots.set({
          ...slots(),
          [accountId]: {
            ...cur,
            lastAttemptedAt: atMs,
            isForbidden: true,
            cooldownUntil: null,
          },
        });
        scheduleNext(false);
        return;
      }
      const nextErrors = cur.consecutiveErrors + 1;
      const backoffKind: Exclude<RefreshErrorKind, "auth_required"> =
        kind === "rate_limited" ? "rate_limited" : "transient";
      const backoffMs = computeBackoffMs(backoffKind, nextErrors);
      slots.set({
        ...slots(),
        [accountId]: {
          ...cur,
          lastAttemptedAt: atMs,
          consecutiveErrors: nextErrors,
          cooldownUntil: atMs + backoffMs,
        },
      });
      scheduleNext(false);
    },
    forgetSlots(accountIds: readonly string[]) {
      if (accountIds.length === 0) return;
      const current = slots();
      let changed = false;
      const next: Record<string, AutoRefreshSlot> = { ...current };
      for (const id of accountIds) {
        if (Object.prototype.hasOwnProperty.call(next, id)) {
          delete next[id];
          changed = true;
        }
      }
      if (changed) {
        slots.set(next);
        scheduleNext(false);
      }
    },
    reschedule() {
      scheduleNext();
    },
    dispose() {
      disposed = true;
      clearTimer();
    },
  };
}
