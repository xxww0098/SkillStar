// 账号页状态机 —— 所有 agent 账号、顶部筛选/排序/视图/隐私/批量选择、
// 反馈横幅与三 Tab 添加对话框（OAuth / Token-JSON / 本地导入）。
//
// 工厂形态 createAccountsState(bridge, autoRefreshPrefs)：页面用真桥建一份，
// 测试装置注入假桥与内存偏好（不碰真实 gui_prefs.json）
// 建自己的；组件只拿这份 state，不碰桥。
//
// 【并发纪律】单卡删除与批量删除互斥（busyId 或 batchBusy）。刷新不进
// batchBusy，否则视图会把每一张卡锁住；只禁用 refreshingIds 里的卡。
// 批量刷新带取消 epoch：取消后不再请求剩余账号。reconcile 走 epoch 门，
// 过期结果落地即弃。切换/刷新/新增成功直接就地 patch DTO。
//
// 【start 不读 signal】start() 由组件 effect 调用，内部只写不读。

import { signal, type Signal } from "nsh";
import { openConfirm } from "../../chrome/dialog";
import { t, tf } from "../../services/translation";
import { toast } from "../../shell/events";
import { dropJob, endJob, putJob } from "../../shell/jobBoard";
import {
  createAddFlow,
  FORM_NEED_CALLBACK,
  FORM_NEED_TOKEN,
  isFormError,
  type AddBusy,
  type AddDialogState,
  type AddStep,
  type AddTab,
} from "./addFlow";
import type { AccountsBridge } from "./bridge";
import {
  accountsEqual,
  applyAccountPatches,
  applyActivatedOutcome,
  countsEqual,
  findRolledBackAccountIds,
  liveEqual,
  loginLine,
  maskAccountText,
  mergeAccountSnapshots,
  orderAccountsServingFirst,
  takeNextDistinctAgentAccount,
  upsertAccountRow,
  upsertLiveFromAccount,
  withRequiresReauth,
  type AccountFilter,
  type AccountSortBy,
  type AccountViewMode,
  type SortDirection,
} from "./logic";
import {
  classifyRefreshError,
  createAutoRefreshScheduler,
  createBatchPatchFlusher,
  DEFAULT_MIN_GAP_MS,
  DEFAULT_STALE_MAX_AGE_SECS,
  isEligibleForAutoRefresh,
  pickStaleAccounts,
  type AutoRefreshSlot,
} from "./autoRefresh";
import type { AutoRefreshPrefs } from "./prefs";
import { railAgents, railCounts, railSelected } from "./rail";
import type { AccountView, ActivateOutcome, AgentLive, AgentRow } from "./types";

export {
  FORM_NEED_CALLBACK,
  FORM_NEED_TOKEN,
  isFormError,
  type AddBusy,
  type AddDialogState,
  type AddStep,
  type AddTab,
};

/** 正文里唯一保留的提示：带可选重试的错误。成功一律走 toast。 */
export type BannerState = { tone: "error"; text: string; retryId?: string };

export interface AccountsState {
  readonly bridge: AccountsBridge;
  /** 受管 agent 行（与壳侧栏槽位同源：rail.tsx 的信号）。 */
  readonly agents: Signal<AgentRow[]>;
  readonly accounts: Signal<AccountView[]>;
  readonly live: Signal<AgentLive[]>;
  /** 首次列表拉取尚未结束前为 true，用于渲染加载骨架。 */
  readonly loading: Signal<boolean>;
  /** 选中的 agent id（rail.tsx 的信号；null = 还在等 agents 装载）。 */
  readonly selected: Signal<string | null>;
  readonly error: Signal<string | null>;
  readonly status: Signal<string | null>;
  readonly banner: Signal<BannerState | null>;
  /** 单动作在途的账号 id（该卡动作禁用）。 */
  readonly busyId: Signal<string | null>;
  /** 正在刷新额度的账号 id（这些卡的按钮禁用，其他卡可用）。 */
  readonly refreshingIds: Signal<string[]>;
  /** 批量删除锁。刷新不能进这里：视图把任意 batchBusy 当成整页锁。 */
  readonly batchBusy: Signal<"delete" | null>;
  /** 批量刷新在途。只转刷新图标，不锁删除和页面。 */
  readonly batchRefreshing: Signal<boolean>;
  /** 确认后正在删除的那一张（只转它的垃圾桶）。 */
  readonly deletingId: Signal<string | null>;
  /** 错峰自动刷新配置周期（ms；null = 关闭）。 */
  readonly autoRefreshInterval: Signal<number | null>;
  /** 下次错峰轮询预计触发时刻（Unix ms；null = 未调度）。 */
  readonly autoRefreshNextAt: Signal<number | null>;
  /** 各账号的错峰调度槽位（冷却 / 失效 / 上次刷新时刻）。 */
  readonly autoRefreshSlots: Signal<Record<string, AutoRefreshSlot>>;
  /** 工具栏查询、视图、筛选、排序与隐私开关。 */
  readonly searchQuery: Signal<string>;
  readonly viewMode: Signal<AccountViewMode>;
  readonly filter: Signal<AccountFilter>;
  readonly sortBy: Signal<AccountSortBy>;
  readonly sortDir: Signal<SortDirection>;
  readonly privacyMode: Signal<boolean>;
  readonly selectedIds: Signal<string[]>;
  readonly add: AddDialogState;
  /** 首挂载初始化（组件 effect 调一次；重复调用是空操作）。 */
  start(): void;
  load(): Promise<void>;
  reconcileLive(): void;
  selectAgent(agent: string): void;
  onAgentChanged(): void;
  refresh(id: string): void;
  refreshAll(targetIds?: string[]): void;
  /** 静默补刷超过 maxAgeSecs（默认 60 秒）未刷新的可用账号，不发 toast 与横幅。 */
  refreshStale(maxAgeSecs?: number): void;
  setAutoRefreshInterval(ms: number | null): void;
  autoRefreshEligibleCount(): number;
  autoRefreshStepSeconds(): number | null;
  activate(id: string): void;
  /** 删除走共享确认卡（破坏性动作不自绘遮罩）。 */
  requestDelete(id: string): void;
  requestBatchDelete(ids: string[]): void;
  remove(id: string): void;
  toggleSelect(id: string): void;
  toggleSelectAll(visibleIds: string[]): void;
  clearSelection(): void;
  toggleSortDir(): void;
  togglePrivacy(): void;
  dismissBanner(): void;
  dismissError(): void;
  openAdd(agent: string): void;
  selectAddTab(tab: AddStep): void;
  selectOauthMode(modeId: string): void;
  closeAdd(): void;
  startLogin(): void;
  submitCallback(): void;
  importLocal(): void;
  submitToken(): void;
  copyAuthText(text: string): void;
  /** 重新授权：为该账号的 agent 打开 OAuth 段并立即开始登录。 */
  reauth(id: string): void;
  /** 遮罩/Escape/关闭钮：等同关对话框（有待办登录就取消）。 */
  dismiss(): void;
}

export function createAccountsState(
  bridge: AccountsBridge,
  autoRefreshPrefs: AutoRefreshPrefs,
): AccountsState {
  const agents = railAgents;
  const selected = railSelected;
  const accounts = signal<AccountView[]>([]);
  const live = signal<AgentLive[]>([]);
  const loading = signal(true);
  const error = signal<string | null>(null);
  const status = signal<string | null>(null);
  const banner = signal<BannerState | null>(null);
  const busyId = signal<string | null>(null);
  const refreshingIds = signal<string[]>([]);
  const batchBusy = signal<"delete" | null>(null);
  const batchRefreshing = signal(false);
  const deletingId = signal<string | null>(null);
  let refreshEpoch = 0;
  const searchQuery = signal("");
  const viewMode = signal<AccountViewMode>("grid");
  const filter = signal<AccountFilter>("all");
  const sortBy = signal<AccountSortBy>("default");
  const sortDir = signal<SortDirection>("desc");
  const privacyMode = signal(false);
  const selectedIds = signal<string[]>([]);

  let started = false;
  let hasLoadedOnce = false;
  let pendingStaleMaxAge: number | null = null;
  let staleSweepRunning = false;
  let liveEpoch = 0;

  /** 成功反馈一律走页面顶部那条短 toast；正文横幅只留错误，页面布局不因此跳动。 */
  function succeed(msg: string): void {
    status.set(msg);
    if (banner() !== null) banner.set(null);
    toast(msg, "success");
  }

  /** 全局候选排序：各 Agent 当前在役账号排前，使状态栏下拉窗主槽优先获刷。 */
  function orderedAllCandidates(): AccountView[] {
    return orderAccountsServingFirst(accounts(), live());
  }

  function recordRefreshOutcome(accountId: string, updated: AccountView): void {
    const snapshotErr = (updated.usage?.error ?? "").trim();
    if (snapshotErr.length > 0) {
      autoScheduler.markError(accountId, snapshotErr, Date.now(), {
        allowAuthRequired: false,
      });
    } else {
      autoScheduler.markRefreshed(accountId);
    }
  }

  function handleRefreshFailure(accountId: string, err: unknown): AccountView | null {
    autoScheduler.markError(accountId, err, Date.now(), { allowAuthRequired: true });
    if (classifyRefreshError(err) === "auth_required") {
      return withRequiresReauth(accounts(), accountId);
    }
    return null;
  }

  async function refreshAccountSilently(accountId: string): Promise<void> {
    if (busyId() !== null || batchBusy() !== null) return;
    if (refreshingIds().includes(accountId)) return;
    refreshingIds.set([...refreshingIds(), accountId]);
    try {
      const updated = await bridge.refresh(accountId);
      recordRefreshOutcome(accountId, updated);
      patchAccount(updated);
    } catch (err) {
      const patched = handleRefreshFailure(accountId, err);
      if (patched !== null) patchAccount(patched);
    } finally {
      refreshingIds.set(refreshingIds().filter((entry) => entry !== accountId));
    }
  }

  /** 账号落地的唯一写口：同时同步左栏的每 agent 账号数，并清理已不存在的勾选。 */
  const autoScheduler = createAutoRefreshScheduler({
    prefs: autoRefreshPrefs,
    getCandidates: orderedAllCandidates,
    getInFlightIds: () => refreshingIds(),
    onTickAccount: refreshAccountSilently,
  });

  function setAccounts(rows: AccountView[]): void {
    const merged = mergeAccountSnapshots(accounts(), rows);
    const sameAccounts = merged === accounts() || accountsEqual(accounts(), merged);
    if (!sameAccounts) {
      accounts.set(merged);
    }
    const counts: Record<string, number> = {};
    const validIds = new Set<string>();
    for (const row of merged) {
      counts[row.agent] = (counts[row.agent] ?? 0) + 1;
      validIds.add(row.id);
    }
    if (!countsEqual(railCounts(), counts)) {
      railCounts.set(counts);
    }
    const currentSelected = selectedIds();
    if (currentSelected.length > 0) {
      const kept = currentSelected.filter((id) => validIds.has(id));
      if (kept.length !== currentSelected.length) {
        selectedIds.set(kept);
      }
    }
    if (!sameAccounts) {
      autoScheduler.reschedule();
    }
  }

  function patchAccount(updated: AccountView): void {
    const patches = new Map<string, AccountView>([[updated.id, updated]]);
    setAccounts(applyAccountPatches(accounts(), patches));
  }

  function setLiveRows(rows: AgentLive[]): void {
    if (!liveEqual(live(), rows)) {
      live.set(rows);
    }
  }

  function applyCreatedAccount(created: AccountView | null | undefined): void {
    if (created && typeof created.id === "string") {
      setAccounts(upsertAccountRow(accounts(), created));
      if (created.isActive) {
        setLiveRows(upsertLiveFromAccount(live(), created));
      }
      return;
    }
    void load();
    reconcileLive();
  }

  const addFlow = createAddFlow({
    bridge,
    agents,
    accounts,
    selected,
    succeed,
    applyCreatedAccount,
  });

  function createAccountBatchFlusher(onFlush?: () => void) {
    return createBatchPatchFlusher<AccountView>({
      applyPatches: (patches) => setAccounts(applyAccountPatches(accounts(), patches)),
      completeIds: (doneIds) =>
        refreshingIds.set(refreshingIds().filter((entry) => !doneIds.has(entry))),
      onFlush,
    });
  }

  function flushStaleSweep(maxAgeSecs: number): void {
    if (staleSweepRunning || busyId() !== null || batchBusy() !== null || batchRefreshing()) return;
    const candidates = orderedAllCandidates();
    const staleIds = pickStaleAccounts(
      candidates,
      autoScheduler.slots(),
      Date.now(),
      new Set(refreshingIds()),
      maxAgeSecs,
    );
    if (staleIds.length === 0) return;

    const byId = new Map(candidates.map((row) => [row.id, row]));
    const queue = staleIds
      .map((id) => byId.get(id))
      .filter((row): row is AccountView => row !== undefined);
    if (queue.length === 0) return;

    const inFlightAgents = new Set<string>();
    const flusher = createAccountBatchFlusher();
    const workerCount = Math.min(3, queue.length);
    staleSweepRunning = true;

    const takeNext = (): AccountView | null =>
      busyId() !== null || batchBusy() !== null
        ? null
        : takeNextDistinctAgentAccount(queue, inFlightAgents);

    const worker = async (): Promise<void> => {
      while (true) {
        const next = takeNext();
        if (next === null) break;
        inFlightAgents.add(next.agent);
        if (!refreshingIds().includes(next.id)) {
          refreshingIds.set([...refreshingIds(), next.id]);
        }
        let updated: AccountView | null = null;
        try {
          updated = await bridge.refresh(next.id);
          recordRefreshOutcome(next.id, updated);
        } catch (err) {
          updated = handleRefreshFailure(next.id, err);
        } finally {
          inFlightAgents.delete(next.agent);
          flusher.enqueue(next.id, updated);
        }
      }
    };

    void Promise.all(Array.from({ length: workerCount }, () => worker())).finally(() => {
      flusher.flush();
      staleSweepRunning = false;
    });
  }

  function refreshStale(maxAgeSecs = DEFAULT_STALE_MAX_AGE_SECS): void {
    if (!hasLoadedOnce) {
      pendingStaleMaxAge =
        pendingStaleMaxAge === null
          ? maxAgeSecs
          : Math.min(pendingStaleMaxAge, maxAgeSecs);
      return;
    }
    flushStaleSweep(maxAgeSecs);
  }

  function load(): Promise<void> {
    return bridge
      .list()
      .then(
        (rows) => {
          const rolledBackIds = findRolledBackAccountIds(accounts(), rows);
          if (rolledBackIds.length > 0) {
            autoScheduler.forgetSlots(rolledBackIds);
          }
          setAccounts(rows);
          if (error() !== null) error.set(null);
          hasLoadedOnce = true;
          if (pendingStaleMaxAge !== null) {
            const maxAge = pendingStaleMaxAge;
            pendingStaleMaxAge = null;
            flushStaleSweep(maxAge);
          }
        },
        (err) => error.set(String(err)),
      )
      .finally(() => {
        if (loading()) loading.set(false);
      });
  }

  function reconcileLive(): void {
    liveEpoch += 1;
    const epoch = liveEpoch;
    bridge.reconcile().then(
      (rows) => {
        if (epoch !== liveEpoch) return;
        setLiveRows(rows);
      },
      () => {
        // 读不到客户端就保持旧表：页面照常渲染。
      },
    );
  }

  function isActionLocked(): boolean {
    return busyId() !== null || batchBusy() !== null;
  }

  function refresh(id: string): void {
    if (busyId() !== null || batchBusy() !== null) return;
    if (refreshingIds().includes(id)) return;
    const pending = banner();
    if (pending?.retryId === id) banner.set(null);
    refreshingIds.set([...refreshingIds(), id]);
    void bridge
      .refresh(id)
      .then(
        (updated) => {
          recordRefreshOutcome(id, updated);
          patchAccount(updated);
          succeed(`${t("common.refresh")} ${t("common.done")}`);
        },
        (err) => {
          const patched = handleRefreshFailure(id, err);
          if (patched !== null) patchAccount(patched);
          const msg = `${t("common.refresh")}: ${String(err)}`;
          status.set(msg);
          banner.set({ tone: "error", text: msg, retryId: id });
        },
      )
      .finally(() => {
        refreshingIds.set(refreshingIds().filter((entry) => entry !== id));
      });
  }

  function publishRefreshJob(done: number, total: number, onCancel: () => void): void {
    putJob({
      id: "accounts-refresh",
      title: tf("wait.refreshingAccounts", { done, total }),
      fraction: done / total,
      cancellable: true,
      onCancel,
    });
  }

  function refreshAll(targetIds?: string[]): void {
    if (busyId() !== null || batchBusy() !== null || batchRefreshing()) return;
    const agent = selected();
    if (agent === null && (!targetIds || targetIds.length === 0)) return;
    const already = new Set(refreshingIds());
    const ids = (
      targetIds && targetIds.length > 0
        ? Array.from(new Set(targetIds))
        : accounts()
            .filter((row) => row.agent === agent)
            .map((row) => row.id)
    ).filter((id) => !already.has(id));
    if (ids.length === 0) return;

    const epoch = ++refreshEpoch;
    const total = ids.length;
    const showCornerJob = total > 1;
    const owned = new Set(ids);
    const cancel = () => {
      refreshEpoch += 1;
    };
    let ok = 0;
    let fail = 0;
    let lastErr = "";
    let done = 0;
    const flusher = createAccountBatchFlusher(() => {
      if (showCornerJob && epoch === refreshEpoch && done > 0 && done < total) {
        publishRefreshJob(done, total, cancel);
      }
    });

    batchRefreshing.set(true);
    refreshingIds.set([...refreshingIds(), ...ids]);
    if (showCornerJob) {
      publishRefreshJob(0, total, cancel);
    }

    void (async () => {
      for (const id of ids) {
        if (epoch !== refreshEpoch) break;
        let failed = false;
        let updated: AccountView | null = null;
        try {
          updated = await bridge.refresh(id);
          recordRefreshOutcome(id, updated);
        } catch (err) {
          failed = true;
          lastErr = String(err);
          updated = handleRefreshFailure(id, err);
        }
        owned.delete(id);
        flusher.enqueue(id, updated);
        if (epoch !== refreshEpoch) break;
        if (failed) fail += 1;
        else ok += 1;
        done += 1;
      }
      flusher.flush();
      const cancelled = epoch !== refreshEpoch;
      if (cancelled || epoch !== refreshEpoch) {
        if (showCornerJob) dropJob("accounts-refresh");
        return;
      }
      if (fail === 0) {
        const summary = tf("accountsPage.batchRefreshed", { count: ok });
        succeed(summary);
        if (showCornerJob) endJob("accounts-refresh", summary);
      } else {
        const msg =
          ok > 0
            ? tf("accountsPage.batchRefreshPartial", { ok, fail })
            : `${t("common.refresh")}: ${lastErr}`;
        status.set(msg);
        banner.set({ tone: "error", text: msg });
        if (showCornerJob) {
          if (ok > 0) endJob("accounts-refresh", msg);
          else dropJob("accounts-refresh");
        }
      }
    })().finally(() => {
      flusher.flush();
      if (owned.size > 0) {
        refreshingIds.set(refreshingIds().filter((entry) => !owned.has(entry)));
      }
      batchRefreshing.set(false);
    });
  }

  /** 切换成功就地同步 accounts/live 并报一条 toast；当前生效由卡上的「当前」标记表达。 */
  function activate(id: string): void {
    if (isActionLocked()) return;
    const target = accounts().find((row) => row.id === id);
    const name = target ? loginLine(target) : id;
    busyId.set(id);
    void bridge
      .activate(id)
      .then(
        (outcome: ActivateOutcome | null | undefined) => {
          if (outcome?.account && typeof outcome.account.id === "string") {
            setAccounts(applyActivatedOutcome(accounts(), outcome));
            setLiveRows(upsertLiveFromAccount(live(), outcome.account, outcome.target));
          } else {
            void load();
            reconcileLive();
          }
          succeed(
            tf("accountsPage.switchedBanner", {
              name: maskAccountText(name, privacyMode()),
            }),
          );
        },
        (err) => {
          const msg = `${t("accountsPage.switch")}: ${String(err)}`;
          status.set(msg);
          banner.set({ tone: "error", text: msg });
        },
      )
      .finally(() => {
        busyId.set(null);
      });
  }

  function remove(id: string): void {
    if (isActionLocked()) return;
    busyId.set(id);
    deletingId.set(id);
    const label = t("common.delete");
    void bridge
      .remove(id)
      .then(
        () => {
          setAccounts(accounts().filter((row) => row.id !== id));
          succeed(`${label} ${t("common.done")}`);
        },
        (err) => {
          const msg = `${label}: ${String(err)}`;
          status.set(msg);
          banner.set({ tone: "error", text: msg });
        },
      )
      .finally(() => {
        if (busyId() === id) busyId.set(null);
        if (deletingId() === id) deletingId.set(null);
      });
  }

  function requestDelete(id: string): void {
    if (isActionLocked()) return;
    const account = accounts().find((row) => row.id === id);
    if (account === undefined) return;
    openConfirm({
      title: t("accountsPage.deleteTitle"),
      body: tf("accountsPage.deleteBody", { name: account.displayName }),
      okText: t("common.delete"),
      danger: true,
      onOk: () => {
        remove(id);
      },
    });
  }

  function removeMany(ids: string[]): void {
    if (ids.length === 0 || isActionLocked()) return;
    batchBusy.set("delete");
    void (async () => {
      const deleted = new Set<string>();
      let lastErr = "";
      for (const id of ids) {
        try {
          await bridge.remove(id);
          deleted.add(id);
        } catch (err) {
          lastErr = String(err);
        }
      }
      if (deleted.size > 0) {
        setAccounts(accounts().filter((row) => !deleted.has(row.id)));
        selectedIds.set([]);
      }
      if (lastErr === "") {
        succeed(tf("accountsPage.batchDeleted", { count: deleted.size }));
      } else {
        const msg = `${t("common.delete")}: ${lastErr}`;
        status.set(msg);
        banner.set({ tone: "error", text: msg });
      }
    })().finally(() => {
      batchBusy.set(null);
    });
  }

  function requestBatchDelete(ids: string[]): void {
    const unique = Array.from(new Set(ids));
    if (unique.length === 0 || isActionLocked()) return;
    if (unique.length === 1) {
      requestDelete(unique[0]);
      return;
    }
    openConfirm({
      title: tf("accountsPage.batchDeleteTitle", { count: unique.length }),
      body: t("accountsPage.batchDeleteBody"),
      okText: t("common.delete"),
      danger: true,
      onOk: () => {
        removeMany(unique);
      },
    });
  }

  function onAgentChanged(): void {
    if (selectedIds().length > 0) selectedIds.set([]);
    if (filter() !== "all") filter.set("all");
  }

  function autoRefreshEligibleCount(): number {
    const nowMs = Date.now();
    const slotMap = autoScheduler.slots();
    const emptySet = new Set<string>();
    return accounts().filter((row) =>
      isEligibleForAutoRefresh(row, slotMap[row.id], nowMs, emptySet),
    ).length;
  }

  function autoRefreshStepSeconds(): number | null {
    const cycle = autoScheduler.intervalMs();
    const eligible = autoRefreshEligibleCount();
    if (cycle === null || cycle <= 0 || eligible === 0) return null;
    return Math.max(1, Math.round(Math.max(cycle / eligible, DEFAULT_MIN_GAP_MS) / 1000));
  }

  return {
    bridge,
    agents,
    accounts,
    live,
    loading,
    selected,
    error,
    status,
    banner,
    busyId,
    refreshingIds,
    batchBusy,
    batchRefreshing,
    deletingId,
    autoRefreshInterval: autoScheduler.intervalMs,
    autoRefreshNextAt: autoScheduler.nextTickAt,
    autoRefreshSlots: autoScheduler.slots,
    searchQuery,
    viewMode,
    filter,
    sortBy,
    sortDir,
    privacyMode,
    selectedIds,
    add: addFlow.add,
    start() {
      if (started) return;
      started = true;
      const rows = bridge.agents();
      agents.set(rows);
      selected.set(rows.length > 0 ? rows[0].id : null);
      setTimeout(() => {
        void load();
      }, 0);
    },
    load,
    reconcileLive,
    selectAgent(agent: string) {
      if (selected() !== agent) {
        selected.set(agent);
        onAgentChanged();
      }
    },
    onAgentChanged,
    refresh,
    refreshAll,
    refreshStale,
    setAutoRefreshInterval(ms: number | null) {
      autoScheduler.setIntervalMs(ms);
    },
    autoRefreshEligibleCount,
    autoRefreshStepSeconds,
    activate,
    requestDelete,
    requestBatchDelete,
    remove,
    toggleSelect(id: string) {
      const cur = selectedIds();
      if (cur.includes(id)) {
        selectedIds.set(cur.filter((entry) => entry !== id));
      } else {
        selectedIds.set([...cur, id]);
      }
    },
    toggleSelectAll(visibleIds: string[]) {
      if (visibleIds.length === 0) return;
      const cur = new Set(selectedIds());
      const allSelected = visibleIds.every((id) => cur.has(id));
      if (allSelected) {
        selectedIds.set(selectedIds().filter((id) => !visibleIds.includes(id)));
      } else {
        for (const id of visibleIds) cur.add(id);
        selectedIds.set(Array.from(cur));
      }
    },
    clearSelection() {
      if (selectedIds().length > 0) selectedIds.set([]);
    },
    toggleSortDir() {
      sortDir.set(sortDir() === "desc" ? "asc" : "desc");
    },
    togglePrivacy() {
      privacyMode.set(!privacyMode());
    },
    dismissBanner() {
      banner.set(null);
      status.set(null);
    },
    dismissError() {
      error.set(null);
      banner.set(null);
      status.set(null);
    },
    openAdd: addFlow.openAdd,
    selectAddTab: addFlow.selectAddTab,
    selectOauthMode: addFlow.selectOauthMode,
    closeAdd: addFlow.closeAdd,
    startLogin: addFlow.startLogin,
    submitCallback: addFlow.submitCallback,
    importLocal: addFlow.importLocal,
    submitToken: addFlow.submitToken,
    copyAuthText: addFlow.copyAuthText,
    reauth: addFlow.reauth,
    dismiss() {
      addFlow.closeAdd();
    },
  };
}
