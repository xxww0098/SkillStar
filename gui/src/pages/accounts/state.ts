// 账号页状态机 —— 所有 agent 账号、顶部筛选/排序/视图/隐私/批量选择、
// 反馈横幅与三 Tab 添加对话框（OAuth / Token-JSON / 本地导入）。
//
// 工厂形态 createAccountsState(bridge)：页面用真桥建一份，测试装置注入假桥
// 建自己的；组件只拿这份 state，不碰桥。
//
// 【并发纪律】单动作与批动作互斥（busyId 或 batchBusy 非空即拒绝新动作）；
// reconcile 走 epoch 门，过期结果落地即弃。切换成功走「重 list」（磁盘真相）；
// 刷新只旋转正在请求的那几张卡。
//
// 【start 不读 signal】start() 由组件 effect 调用，内部只写不读。

import { Clipboard, signal, type Signal } from "nsh";
import { openConfirm } from "../../chrome/dialog";
import { t, tf } from "../../services/translation";
import type { AccountsBridge } from "./bridge";
import {
  loginLine,
  type AccountFilter,
  type AccountSortBy,
  type AccountViewMode,
  type SortDirection,
} from "./logic";
import { railAgents, railCounts, railSelected } from "./rail";
import type { AccountView, AgentLive, AgentRow, LoginStart } from "./types";

/** 空输入的表单错误键（渲染时 t()）。 */
export const FORM_NEED_CALLBACK = "accountsPage.needCallback";
export const FORM_NEED_TOKEN = "accountsPage.needToken";

export function isFormError(message: string | null): boolean {
  return message === FORM_NEED_CALLBACK || message === FORM_NEED_TOKEN;
}

/** 添加对话框顶部三 Tab。 */
export type AddStep = "oauth" | "token" | "local";
export type AddTab = AddStep;

/** 添加对话框的在途动作（非空 = 该动作的 spinner）。 */
export type AddBusy = "start" | "callback" | "finish" | "import" | "token" | null;

export type BannerState =
  | { tone: "info" | "success" | "error"; kind: "text"; text: string }
  | { tone: "info" | "success"; kind: "switched"; rawName: string };

export interface AddDialogState {
  /** 非空 = 对话框为该 agent 打开。 */
  agent: Signal<string | null>;
  step: Signal<AddStep>;
  /** 等待中的登录（OAuth 段的全部渲染输入）。 */
  login: Signal<LoginStart | null>;
  /** 回调粘贴框（不是令牌框）。 */
  callback: Signal<string>;
  /** 令牌导入框。 */
  token: Signal<string>;
  error: Signal<string | null>;
  busy: Signal<AddBusy>;
  /** 复制链接/配对码后的短暂确认。 */
  copied: Signal<boolean>;
}

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
  /** 正在刷新额度的账号 id 集合（用于单卡或批量刷新时的局域旋钮）。 */
  readonly refreshingIds: Signal<string[]>;
  /** 批量动作锁（刷新全部/批量删除）。 */
  readonly batchBusy: Signal<"refresh" | "delete" | null>;
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
  openAdd(agent: string): void;
  selectAddTab(tab: AddStep): void;
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

export function createAccountsState(bridge: AccountsBridge): AccountsState {
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
  const batchBusy = signal<"refresh" | "delete" | null>(null);
  const searchQuery = signal("");
  const viewMode = signal<AccountViewMode>("grid");
  const filter = signal<AccountFilter>("all");
  const sortBy = signal<AccountSortBy>("default");
  const sortDir = signal<SortDirection>("desc");
  const privacyMode = signal(false);
  const selectedIds = signal<string[]>([]);

  const add: AddDialogState = {
    agent: signal<string | null>(null),
    step: signal<AddStep>("oauth"),
    login: signal<LoginStart | null>(null),
    callback: signal(""),
    token: signal(""),
    error: signal<string | null>(null),
    busy: signal<AddBusy>(null),
    copied: signal(false),
  };

  let started = false;
  let liveEpoch = 0;
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function clearCopyFeedback(): void {
    if (copyTimer !== null) {
      clearTimeout(copyTimer);
      copyTimer = null;
    }
    add.copied.set(false);
  }

  /** 账号落地的唯一写口：同时同步左栏的每 agent 账号数，并清理已不存在的勾选。 */
  function setAccounts(rows: AccountView[]): void {
    accounts.set(rows);
    const counts: Record<string, number> = {};
    const validIds = new Set<string>();
    for (const row of rows) {
      counts[row.agent] = (counts[row.agent] ?? 0) + 1;
      validIds.add(row.id);
    }
    railCounts.set(counts);
    const kept = selectedIds().filter((id) => validIds.has(id));
    if (kept.length !== selectedIds().length) {
      selectedIds.set(kept);
    }
  }

  function load(): Promise<void> {
    return bridge
      .list()
      .then(
        (rows) => {
          setAccounts(rows);
          error.set(null);
        },
        (err) => error.set(String(err)),
      )
      .finally(() => {
        loading.set(false);
      });
  }

  function reconcileLive(): void {
    liveEpoch += 1;
    const epoch = liveEpoch;
    bridge.reconcile().then(
      (rows) => {
        if (epoch !== liveEpoch) return;
        live.set(rows);
      },
      () => {
        // 读不到客户端就保持旧表：页面照常渲染。
      },
    );
  }

  function supportsOauthFor(agentId: string): boolean {
    return agents().find((a) => a.id === agentId)?.supportsOauth !== false;
  }

  function defaultTabFor(agent: string): AddStep {
    return supportsOauthFor(agent) ? "oauth" : "local";
  }

  function cancelPendingOAuth(): void {
    const pending = add.login();
    if (pending !== null) {
      try {
        bridge.cancelOAuthLogin(pending.loginId);
      } catch {
        // 会话已消失：同样无事可做。
      }
    }
  }

  function resetAdd(): void {
    clearCopyFeedback();
    add.agent.set(null);
    add.step.set("oauth");
    add.login.set(null);
    add.callback.set("");
    add.token.set("");
    add.error.set(null);
    add.busy.set(null);
  }

  function closeAdd(): void {
    cancelPendingOAuth();
    resetAdd();
  }

  function isActionLocked(): boolean {
    return busyId() !== null || batchBusy() !== null;
  }

  /** 动作的公共壳：互斥、状态与横幅、finally 清 busy。 */
  function runOne(
    id: string,
    label: string,
    call: () => Promise<unknown>,
    onDone?: () => void,
    onSuccessBanner?: () => BannerState,
  ): void {
    if (isActionLocked()) return;
    busyId.set(id);
    void call()
      .then(
        () => {
          const msg = `${label} ${t("common.done")}`;
          status.set(msg);
          banner.set(onSuccessBanner ? onSuccessBanner() : { tone: "success", kind: "text", text: msg });
          onDone?.();
        },
        (err) => {
          const msg = `${label}: ${String(err)}`;
          status.set(msg);
          banner.set({ tone: "error", kind: "text", text: msg });
        },
      )
      .finally(() => {
        busyId.set(null);
      });
  }

  function refresh(id: string): void {
    if (isActionLocked()) return;
    busyId.set(id);
    refreshingIds.set([id]);
    void bridge
      .refresh(id)
      .then(
        () => {
          const msg = `${t("common.refresh")} ${t("common.done")}`;
          status.set(msg);
          banner.set({ tone: "success", kind: "text", text: msg });
          void load();
        },
        (err) => {
          const msg = `${t("common.refresh")}: ${String(err)}`;
          status.set(msg);
          banner.set({ tone: "error", kind: "text", text: msg });
        },
      )
      .finally(() => {
        busyId.set(null);
        refreshingIds.set([]);
      });
  }

  function refreshAll(targetIds?: string[]): void {
    if (isActionLocked()) return;
    const agent = selected();
    if (agent === null) return;
    const ids =
      targetIds && targetIds.length > 0
        ? Array.from(new Set(targetIds))
        : accounts()
            .filter((row) => row.agent === agent)
            .map((row) => row.id);
    if (ids.length === 0) return;

    batchBusy.set("refresh");
    refreshingIds.set([...ids]);
    void (async () => {
      let ok = 0;
      let fail = 0;
      let lastErr = "";
      for (const id of ids) {
        try {
          await bridge.refresh(id);
          ok += 1;
        } catch (err) {
          fail += 1;
          lastErr = String(err);
        } finally {
          refreshingIds.set(refreshingIds().filter((entry) => entry !== id));
        }
      }
      await load();
      if (fail === 0) {
        const msg = tf("accountsPage.batchRefreshed", { count: ok });
        status.set(msg);
        banner.set({ tone: "success", kind: "text", text: msg });
      } else {
        const msg =
          ok > 0
            ? tf("accountsPage.batchRefreshPartial", { ok, fail })
            : `${t("common.refresh")}: ${lastErr}`;
        status.set(msg);
        banner.set({ tone: "error", kind: "text", text: msg });
      }
    })().finally(() => {
      refreshingIds.set([]);
      batchBusy.set(null);
    });
  }

  function activate(id: string): void {
    const target = accounts().find((row) => row.id === id);
    const rawName = target ? loginLine(target) : id;
    runOne(
      id,
      t("accountsPage.switch"),
      () => bridge.activate(id),
      () => {
        void load();
        reconcileLive();
      },
      () => ({ tone: "info", kind: "switched", rawName }),
    );
  }

  function remove(id: string): void {
    runOne(
      id,
      t("common.delete"),
      () => bridge.remove(id),
      () => {
        setAccounts(accounts().filter((row) => row.id !== id));
      },
    );
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
        const msg = tf("accountsPage.batchDeleted", { count: deleted.size });
        status.set(msg);
        banner.set({ tone: "success", kind: "text", text: msg });
      } else {
        const msg = `${t("common.delete")}: ${lastErr}`;
        status.set(msg);
        banner.set({ tone: "error", kind: "text", text: msg });
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

  function openAdd(agent: string): void {
    resetAdd();
    selected.set(agent);
    add.agent.set(agent);
    add.step.set(defaultTabFor(agent));
  }

  function selectAddTab(tab: AddStep): void {
    const curBusy = add.busy();
    if (curBusy !== null && curBusy !== "finish") return;
    if (tab !== add.step() && add.login() !== null) {
      cancelPendingOAuth();
      add.login.set(null);
      add.busy.set(null);
      add.callback.set("");
    }
    clearCopyFeedback();
    add.error.set(null);
    add.step.set(tab);
  }

  /** 登录器：起 OAuth → 等待段 → finish 落地。 */
  function startLogin(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null) return;
    add.busy.set("start");
    add.error.set(null);
    add.step.set("oauth");
    bridge.startLogin(agent).then(
      (start) => {
        add.busy.set("finish");
        add.login.set(start);
        bridge.finishLogin(start.loginId).then(
          () => {
            resetAdd();
            const msg = t("accountsPage.added");
            status.set(msg);
            banner.set({ tone: "success", kind: "text", text: msg });
            void load();
            reconcileLive();
          },
          (err) => {
            if (add.agent() === null) return;
            add.busy.set(null);
            add.error.set(String(err));
          },
        );
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
        add.step.set("oauth");
      },
    );
  }

  function submitCallback(): void {
    const pending = add.login();
    if (pending === null || add.busy() !== "finish") return;
    const text = add.callback().trim();
    if (text.length === 0) {
      add.error.set(FORM_NEED_CALLBACK);
      return;
    }
    add.busy.set("callback");
    add.error.set(null);
    bridge.submitOAuthCallback(pending.loginId, text).then(
      () => {
        add.busy.set("finish");
      },
      (err) => {
        add.busy.set("finish");
        add.error.set(String(err));
      },
    );
  }

  function importLocal(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null) return;
    add.busy.set("import");
    add.error.set(null);
    bridge.importLocal(agent).then(
      () => {
        resetAdd();
        const msg = t("accountsPage.importedLocal");
        status.set(msg);
        banner.set({ tone: "success", kind: "text", text: msg });
        void load();
        reconcileLive();
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
      },
    );
  }

  function submitToken(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null) return;
    const payload = add.token().trim();
    if (payload.length === 0) {
      add.error.set(FORM_NEED_TOKEN);
      return;
    }
    add.busy.set("token");
    add.error.set(null);
    bridge.importToken(agent, payload).then(
      () => {
        resetAdd();
        const msg = t("accountsPage.importedToken");
        status.set(msg);
        banner.set({ tone: "success", kind: "text", text: msg });
        void load();
        reconcileLive();
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
      },
    );
  }

  function copyAuthText(text: string): void {
    const trimmed = text.trim();
    if (trimmed.length === 0) return;
    void Clipboard.writeText(trimmed).then(
      () => {
        clearCopyFeedback();
        add.copied.set(true);
        copyTimer = setTimeout(() => {
          copyTimer = null;
          add.copied.set(false);
        }, 1500);
      },
      () => {
        // 剪贴板不可用时静默忽略。
      },
    );
  }

  function reauth(id: string): void {
    const account = accounts().find((row) => row.id === id);
    if (account === undefined) return;
    openAdd(account.agent);
    if (supportsOauthFor(account.agent)) {
      add.step.set("oauth");
      startLogin();
    }
  }

  function onAgentChanged(): void {
    selectedIds.set([]);
    filter.set("all");
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
    searchQuery,
    viewMode,
    filter,
    sortBy,
    sortDir,
    privacyMode,
    selectedIds,
    add,
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
      selectedIds.set([]);
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
    openAdd,
    selectAddTab,
    closeAdd,
    startLogin,
    submitCallback,
    importLocal,
    submitToken,
    copyAuthText,
    reauth,
    dismiss() {
      closeAdd();
    },
  };
}
