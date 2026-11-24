// 账号页总装：固定悬浮工具栏、虚拟化列表区、批量选择条、可关闭错误横幅、三态视图切换与三标签添加弹窗。
// agent 切换统一由应用左侧栏 AccountsProviderMenu 承担，页内不设页头栏或重复切换条。
// 数据与状态全在 state.ts；组件体内不预读 signal，保证 nsh getter 响应式。

import { Scroll, Text, View, computed, effect, untracked, type JSX } from "nsh";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { Icon } from "../../chrome/icons";
import { CARD_ROW_W, PAGE_PAD, TOOLBAR_H } from "../../layout";
import { PAGE_BAR_BAND_TOP } from "../../chrome/toolbar";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { pageBandPadRight } from "../../shell/caption";
import { currentPage } from "../../shell/nav";
import { AddAccountDialog } from "./addDialog";
import { realAccountsBridge } from "./bridge";
import { filterAndSortAccounts, isServing, liveAccountId } from "./logic";
import { realAutoRefreshPrefs } from "./prefs";
import { railSelected } from "./rail";
import { createAccountsState, type AccountsState, type BannerState } from "./state";
import { AccountSelectionBar } from "./selectionBar";
import {
  AccountsEmptyState,
  AccountsNoMatchState,
  AccountsSkeletonGrid,
} from "./states";
import { AccountsToolbar } from "./toolbar";
import type { AccountView } from "./types";
import {
  AccountCompactView,
  AccountGridView,
  AccountListView,
} from "./views";

const pageState = createAccountsState(realAccountsBridge, realAutoRefreshPrefs);

/** 壳级（系统托盘弹出窗与侧栏快捷入口）与账号页共享同一份响应式状态与轮询实例。 */
export const sharedAccountsState: AccountsState = pageState;

/** 路由入口：进入 accounts 页后再触发惰性初始化与过期配额补刷。 */
export function AccountsPage(): JSX.Element {
  effect(() => {
    if (currentPage() !== "accounts") return;
    pageState.start();
    pageState.reconcileLive();
    pageState.refreshStale();
  });
  return <AccountsWorkbench state={pageState} lazy />;
}

function currentAgentId(state: AccountsState): string {
  return railSelected() ?? state.selected() ?? state.agents()[0]?.id ?? "codex";
}

function sameElements<T>(prev: readonly T[], next: readonly T[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

function clearSearchAndFilter(state: AccountsState): void {
  state.searchQuery.set("");
  state.filter.set("all");
}

/** 可注入 bridge/state 的工作台宿主（生产与测试共用）。 */
export function AccountsWorkbench(props: { state: AccountsState; lazy?: boolean }): JSX.Element {
  const state = props.state;
  const pageActive = (): boolean => props.lazy !== true || currentPage() === "accounts";
  const gated = <T,>(
    read: () => T,
    fallback: T,
    equals?: (prev: T, next: T) => boolean,
  ) =>
    computed<T>(
      (prev) => (!pageActive() ? (prev !== undefined ? prev : fallback) : read()),
      equals ? { equals } : undefined,
    );

  const currentAgent = gated(() => currentAgentId(state), "codex");
  const currentAgentInfo = gated(() => {
    const id = currentAgent();
    return state.agents().find((a) => a.id === id) ?? { id, displayName: id };
  }, { id: "codex", displayName: "codex" });
  const agentRows = gated<AccountView[]>(
    () => {
      const id = currentAgent();
      return state.accounts().filter((a) => a.agent === id);
    },
    [],
    sameElements,
  );
  const servingId = gated<string | null>(() => {
    const id = currentAgent();
    const rows = agentRows();
    const reconcileId = liveAccountId(state.live(), id);
    return rows.find((a) => isServing(a, reconcileId))?.id ?? null;
  }, null);
  const visibleRows = gated<AccountView[]>(
    () =>
      filterAndSortAccounts(agentRows(), {
        query: state.searchQuery(),
        filter: state.filter(),
        sortBy: state.sortBy(),
        sortDir: state.sortDir(),
        servingId: servingId(),
      }),
    [],
    sameElements,
  );
  const visibleIds = gated<string[]>(
    () => visibleRows().map((a) => a.id),
    [],
    sameElements,
  );
  const gatedLoading = gated(() => state.loading(), true);
  const gatedError = gated(() => state.error(), null);
  const gatedBanner = gated(() => state.banner(), null);
  const gatedViewMode = gated(() => state.viewMode(), "grid");
  const gatedBusyId = gated(() => state.busyId(), null);
  const gatedBatchBusy = gated(() => state.batchBusy() !== null, false);
  const gatedRefreshingIds = gated(() => state.refreshingIds(), [], sameElements);
  const gatedDeletingId = gated(() => state.deletingId(), null);
  const gatedSelectedIds = gated(() => state.selectedIds(), [], sameElements);
  const gatedMasked = gated(() => state.privacyMode(), false);

  const showSkeleton = bindDelayedFlag(
    () => pageActive() && gatedLoading() && agentRows().length === 0,
  );

  effect(() => {
    if (props.lazy === true && currentPage() !== "accounts") return;
    void state.start();
  });

  // 侧栏 AccountsProviderMenu 切换 agent 时，统一清空多选与筛选状态。
  effect(() => {
    railSelected();
    untracked(() => state.onAgentChanged());
  });

  const handleToggleSelect = (id: string) => state.toggleSelect(id);
  const handleRefresh = (id: string) => void state.refresh(id);
  const handleActivate = (id: string) => void state.activate(id);
  const handleDelete = (id: string) => void state.requestDelete(id);
  const handleReauth = (id: string) => void state.reauth(id);

  return (
    <View
      testID="acct-page"
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
        // No background of its own: the page rides the shell panel's `panel`
        // surface, exactly like the skills page, so the two pages' gutters are
        // the same colour (`bg` here made the accounts side one step darker).
      }}
    >
      {/* 固定悬浮工具栏带（与 PageBar 同几何，位于滚动视口外以支撑下方 <List> 虚拟滚动）。 */}
      <View
        testID="acct-top-drag"
        desktop={{ windowDrag: true }}
        style={{
          display: "flex",
          flexDirection: "row",
          width: "100%",
          minHeight: TOOLBAR_H,
          flexShrink: 0,
          paddingTop: PAGE_BAR_BAND_TOP,
          paddingLeft: PAGE_PAD,
          // As on PageBar: out from under Windows' caption buttons.
          paddingRight: pageBandPadRight(),
        }}
      >
        <AccountsToolbar
          state={state}
          agentId={currentAgent()}
          totalCount={agentRows().length}
          allAgentRows={agentRows()}
        />
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "column",
          flex: 1,
          minHeight: 0,
          gap: 12,
          width: "100%",
          maxWidth: CARD_ROW_W + PAGE_PAD * 2,
          alignSelf: "center",
          paddingTop: 12,
          paddingHorizontal: PAGE_PAD,
        }}
      >
        <If when={agentRows().length > 0}>
          <AccountSelectionBar
            state={state}
            agentId={currentAgent()}
            visibleIds={visibleIds()}
          />
        </If>

        <If when={Boolean(gatedError())}>
          <ErrorBanner text={gatedError() ?? ""} onClose={() => state.dismissError()} />
        </If>

        <If when={gatedBanner() !== null}>
          <ActionBanner
            banner={gatedBanner()!}
            onClose={() => state.dismissBanner()}
            onRetry={(id) => state.refresh(id)}
          />
        </If>

        <View
          style={{
            position: "relative",
            display: "flex",
            flexDirection: "column",
            flex: 1,
            minHeight: 0,
            width: "100%",
          }}
        >
          <If when={agentRows().length > 0 && visibleRows().length === 0}>
            <View style={{ flex: 1, minHeight: 0, paddingBottom: PAGE_PAD }}>
              <AccountsNoMatchState onClear={() => clearSearchAndFilter(state)} />
            </View>
          </If>

          <If when={agentRows().length > 0 && visibleRows().length > 0 && gatedViewMode() === "list"}>
            <View style={{ display: "flex", flex: 1, minHeight: 0, paddingBottom: PAGE_PAD }}>
              <AccountListView
                accounts={visibleRows()}
                agentId={currentAgent()}
                servingId={servingId()}
                busyId={gatedBusyId()}
                batchBusy={gatedBatchBusy()}
                refreshingIds={gatedRefreshingIds()}
                deletingId={gatedDeletingId()}
                selectedIds={gatedSelectedIds()}
                masked={gatedMasked()}
                onToggleSelect={handleToggleSelect}
                onRefresh={handleRefresh}
                onActivate={handleActivate}
                onDelete={handleDelete}
                onReauth={handleReauth}
              />
            </View>
          </If>

          <If
            when={
              agentRows().length > 0 && visibleRows().length > 0 && gatedViewMode() === "compact"
            }
          >
            <AccountCompactView
              active={pageActive}
              accounts={visibleRows()}
              agentId={currentAgent()}
              servingId={servingId()}
              busyId={gatedBusyId()}
              batchBusy={gatedBatchBusy()}
              refreshingIds={gatedRefreshingIds()}
              deletingId={gatedDeletingId()}
              selectedIds={gatedSelectedIds()}
              masked={gatedMasked()}
              onToggleSelect={handleToggleSelect}
              onRefresh={handleRefresh}
              onActivate={handleActivate}
              onDelete={handleDelete}
              onReauth={handleReauth}
            />
          </If>

          <If when={agentRows().length > 0 && visibleRows().length > 0 && gatedViewMode() === "grid"}>
            <AccountGridView
              active={pageActive}
              accounts={visibleRows()}
              agentId={currentAgent()}
              servingId={servingId()}
              busyId={gatedBusyId()}
              batchBusy={gatedBatchBusy()}
              refreshingIds={gatedRefreshingIds()}
              deletingId={gatedDeletingId()}
              selectedIds={gatedSelectedIds()}
              masked={gatedMasked()}
              onToggleSelect={handleToggleSelect}
              onRefresh={handleRefresh}
              onActivate={handleActivate}
              onDelete={handleDelete}
              onReauth={handleReauth}
            />
          </If>

          <If when={agentRows().length === 0 && showSkeleton()}>
            <View style={{ flex: 1, minHeight: 0, overflow: "hidden", paddingBottom: PAGE_PAD }}>
              <AccountsSkeletonGrid />
            </View>
          </If>

          <If when={agentRows().length === 0 && !showSkeleton() && !gatedLoading()}>
            <Scroll
              testID="acct-scroll"
              style={{ flex: 1, minHeight: 0, width: "100%" }}
              contentStyle={{
                display: "flex",
                flexDirection: "column",
                flexGrow: 1,
                width: "100%",
                paddingBottom: PAGE_PAD,
              }}
            >
              <AccountsEmptyState
                agentId={currentAgentInfo().id}
                agentName={currentAgentInfo().displayName}
                onAdd={() => state.openAdd(currentAgentInfo().id)}
              />
            </Scroll>
          </If>
        </View>
      </View>

      <AddAccountDialog state={state} />
    </View>
  );
}

/** 正文横幅只报错误；带重试的错误在右侧给「重试」。成功反馈走 toast。 */
function ActionBanner(props: {
  banner: BannerState;
  onClose: () => void;
  onRetry: (id: string) => void;
}): JSX.Element {
  return (
    <BannerSurface
      testID="acct-banner"
      closeTestID="acct-banner-close"
      tone={props.banner.tone}
      text={props.banner.text}
      onClose={props.onClose}
      onRetry={
        props.banner.retryId === undefined
          ? undefined
          : () => {
              const id = props.banner.retryId;
              if (id !== undefined) props.onRetry(id);
            }
      }
    />
  );
}

function ErrorBanner(props: { text: string; onClose: () => void }): JSX.Element {
  return (
    <BannerSurface
      testID="acct-error"
      closeTestID="acct-error-close"
      tone="error"
      text={props.text}
      onClose={props.onClose}
    />
  );
}

function BannerSurface(props: {
  testID: string;
  closeTestID: string;
  tone: "info" | "success" | "error";
  text: string;
  onClose: () => void;
  onRetry?: () => void;
}): JSX.Element {
  const colors = () => {
    switch (props.tone) {
      case "success":
        return {
          bg: paletteToken("ok_bg"),
          border: paletteToken("ok_border"),
          icon: paletteToken("ok"),
          name: "checkCircle" as const,
        };
      case "error":
        return {
          bg: paletteToken("danger_bg"),
          border: paletteToken("danger_border"),
          icon: paletteToken("danger"),
          name: "xCircle" as const,
        };
      case "info":
        return {
          bg: paletteToken("info_bg"),
          border: paletteToken("info_border"),
          icon: paletteToken("info"),
          name: "info" as const,
        };
    }
  };

  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
        width: "100%",
        paddingVertical: 9,
        paddingHorizontal: 14,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: colors().border,
        backgroundColor: colors().bg,
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          flex: 1,
          minWidth: 0,
        }}
      >
        <Icon name={colors().name} size={15} color={colors().icon} />
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            numberOfLines: 2,
            textOverflow: "ellipsis",
            fontSize: 13,
            fontWeight: "500",
            color: paletteToken("fg"),
          }}
        >
          {props.text}
        </Text>
      </View>

      {props.onRetry ? (
        <View
          testID="acct-banner-retry"
          role="button"
          accessibilityLabel={t("common.retry")}
          style={{
            paddingHorizontal: 8,
            paddingVertical: 3,
            borderRadius: 6,
            cursor: "pointer",
            flexShrink: 0,
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={props.onRetry}
        >
          <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
            {t("common.retry")}
          </Text>
        </View>
      ) : null}

      <View
        testID={props.closeTestID}
        role="button"
        accessibilityLabel={t("common.close")}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 24,
          height: 24,
          borderRadius: 999,
          cursor: "pointer",
          flexShrink: 0,
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={props.onClose}
      >
        <Icon name="x" size={13} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}
