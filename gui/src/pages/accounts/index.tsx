// 账号页总装：全宽滚动区、悬浮工具栏、批量选择条、可关闭状态横幅、三态视图切换与三标签添加弹窗。
// agent 切换统一由应用左侧栏 AccountsProviderMenu 承担，页内不设页头栏或重复切换条。
// 数据与状态全在 state.ts；组件体内不预读 signal，保证 nsh getter 响应式。

import { Scroll, Text, View, effect, untracked, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { CARD_ROW_W, PAGE_PAD, TOOLBAR_H } from "../../layout";
import { PAGE_BAR_BAND_TOP } from "../../chrome/toolbar";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { pageBandPadRight } from "../../shell/caption";
import { currentPage } from "../../shell/nav";
import { AddAccountDialog } from "./addDialog";
import { realAccountsBridge } from "./bridge";
import {
  filterAndSortAccounts,
  isServing,
  liveAccountId,
  maskAccountText,
} from "./logic";
import { railSelected } from "./rail";
import { createAccountsState, type AccountsState, type BannerState } from "./state";
import { AccountSelectionBar } from "./selectionBar";
import {
  AccountsEmptyState,
  AccountsNoMatchState,
  AccountsSkeletonGrid,
} from "./states";
import { AccountsToolbar } from "./toolbar";
import type { AccountView, AgentRow } from "./types";
import {
  AccountCompactView,
  AccountGridView,
  AccountListView,
} from "./views";

const pageState = createAccountsState(realAccountsBridge);

/** 路由入口：进入 accounts 页后再触发惰性初始化。 */
export function AccountsPage(): JSX.Element {
  return <AccountsWorkbench state={pageState} lazy />;
}

function currentAgentId(state: AccountsState): string {
  return railSelected() ?? state.selected() ?? state.agents()[0]?.id ?? "codex";
}

function currentAgentRow(state: AccountsState): AgentRow {
  const id = currentAgentId(state);
  return state.agents().find((a) => a.id === id) ?? { id, displayName: id };
}

function agentAccountsNow(state: AccountsState): AccountView[] {
  const id = currentAgentId(state);
  return state.accounts().filter((a) => a.agent === id);
}

function servingIdNow(state: AccountsState, rows: AccountView[]): string | null {
  const id = currentAgentId(state);
  const reconcileId = liveAccountId(state.live(), id);
  return rows.find((a) => isServing(a, reconcileId))?.id ?? null;
}

function visibleAccountsNow(state: AccountsState): AccountView[] {
  const rows = agentAccountsNow(state);
  return filterAndSortAccounts(rows, {
    query: state.searchQuery(),
    filter: state.filter(),
    sortBy: state.sortBy(),
    sortDir: state.sortDir(),
    servingId: servingIdNow(state, rows),
  });
}

function clearSearchAndFilter(state: AccountsState): void {
  state.searchQuery.set("");
  state.filter.set("all");
}

/** 可注入 bridge/state 的工作台宿主（生产与测试共用）。 */
export function AccountsWorkbench(props: { state: AccountsState; lazy?: boolean }): JSX.Element {
  const state = props.state;

  effect(() => {
    if (props.lazy === true && currentPage() !== "accounts") return;
    void state.start();
  });

  // 侧栏 AccountsProviderMenu 切换 agent 时，统一清空多选与筛选状态。
  effect(() => {
    railSelected();
    untracked(() => state.onAgentChanged());
  });

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
      <Scroll
        testID="acct-scroll"
        style={{
          flex: 1,
          minHeight: 0,
          width: "100%",
        }}
        // 滚动内容至少与视口等高：空态虚线面板才撑得满工具栏以下的整个窗口，
        // 而不是在面板下方留一条空白带（nsh Scroll 的 contentStyle 契约）。
        contentStyle={{
          display: "flex",
          flexDirection: "column",
          flexGrow: 1,
          width: "100%",
        }}
      >
        {/* The toolbar band shares its geometry with PageBar (chrome/toolbar.tsx):
            TOOLBAR_H tall, PAGE_PAD gutters, the card PAGE_BAR_BAND_TOP below the
            panel edge — the six pages must line up pixel for pixel. */}
        <View
          testID="acct-top-drag"
          desktop={{ windowDrag: true }}
          style={{
            display: "flex",
            flexDirection: "row",
            width: "100%",
            minHeight: TOOLBAR_H,
            paddingTop: PAGE_BAR_BAND_TOP,
            paddingLeft: PAGE_PAD,
            // As on PageBar: out from under Windows' caption buttons.
            paddingRight: pageBandPadRight(),
          }}
        >
          <AccountsToolbar
            state={state}
            agentId={currentAgentId(state)}
            totalCount={agentAccountsNow(state).length}
            allAgentRows={agentAccountsNow(state)}
          />
        </View>

        <View
          style={{
            display: "flex",
            flexDirection: "column",
            // 与滚动内容一起长高：空态才有剩余高度可吃。
            flexGrow: 1,
            gap: 12,
            width: "100%",
            maxWidth: CARD_ROW_W + PAGE_PAD * 2,
            alignSelf: "center",
            paddingTop: 12,
            paddingHorizontal: PAGE_PAD,
            paddingBottom: PAGE_PAD,
          }}
        >
          {agentAccountsNow(state).length > 0 ? (
            <AccountSelectionBar
              state={state}
              agentId={currentAgentId(state)}
              visibleIds={visibleAccountsNow(state).map((a) => a.id)}
            />
          ) : null}

          {state.error() ? (
            <ErrorBanner text={state.error() ?? ""} onClose={() => state.dismiss()} />
          ) : null}

          {state.banner() ? (
            <ActionBanner
              banner={state.banner()!}
              masked={state.privacyMode()}
              onClose={() => state.dismissBanner()}
            />
          ) : null}

          {state.loading() && agentAccountsNow(state).length === 0 ? (
            <AccountsSkeletonGrid />
          ) : agentAccountsNow(state).length === 0 ? (
            <AccountsEmptyState
              agentId={currentAgentRow(state).id}
              agentName={currentAgentRow(state).displayName}
              onAdd={() => state.openAdd(currentAgentRow(state).id)}
            />
          ) : visibleAccountsNow(state).length === 0 ? (
            <AccountsNoMatchState onClear={() => clearSearchAndFilter(state)} />
          ) : state.viewMode() === "list" ? (
            <AccountListView
              accounts={visibleAccountsNow(state)}
              servingId={servingIdNow(state, agentAccountsNow(state))}
              busyId={state.busyId()}
              batchBusy={state.batchBusy() !== null}
              refreshingIds={state.refreshingIds()}
              selectedIds={state.selectedIds()}
              masked={state.privacyMode()}
              onToggleSelect={(id) => state.toggleSelect(id)}
              onRefresh={(id) => void state.refresh(id)}
              onActivate={(id) => void state.activate(id)}
              onDelete={(id) => void state.requestDelete(id)}
              onReauth={(id) => void state.reauth(id)}
            />
          ) : state.viewMode() === "compact" ? (
            <AccountCompactView
              accounts={visibleAccountsNow(state)}
              servingId={servingIdNow(state, agentAccountsNow(state))}
              busyId={state.busyId()}
              batchBusy={state.batchBusy() !== null}
              refreshingIds={state.refreshingIds()}
              selectedIds={state.selectedIds()}
              masked={state.privacyMode()}
              onToggleSelect={(id) => state.toggleSelect(id)}
              onRefresh={(id) => void state.refresh(id)}
              onActivate={(id) => void state.activate(id)}
              onDelete={(id) => void state.requestDelete(id)}
              onReauth={(id) => void state.reauth(id)}
            />
          ) : (
            <AccountGridView
              accounts={visibleAccountsNow(state)}
              servingId={servingIdNow(state, agentAccountsNow(state))}
              busyId={state.busyId()}
              batchBusy={state.batchBusy() !== null}
              refreshingIds={state.refreshingIds()}
              selectedIds={state.selectedIds()}
              masked={state.privacyMode()}
              onToggleSelect={(id) => state.toggleSelect(id)}
              onRefresh={(id) => void state.refresh(id)}
              onActivate={(id) => void state.activate(id)}
              onDelete={(id) => void state.requestDelete(id)}
              onReauth={(id) => void state.reauth(id)}
            />
          )}
        </View>
      </Scroll>

      <AddAccountDialog state={state} />
    </View>
  );
}

function bannerMessage(banner: BannerState, masked: boolean): string {
  if (banner.kind === "switched") {
    return tf("accountsPage.switchedBanner", {
      name: maskAccountText(banner.rawName, masked),
    });
  }
  return banner.text;
}

function ActionBanner(props: {
  banner: BannerState;
  masked: boolean;
  onClose: () => void;
}): JSX.Element {
  return (
    <BannerSurface
      testID="acct-banner"
      closeTestID="acct-banner-close"
      tone={props.banner.tone}
      text={bannerMessage(props.banner, props.masked)}
      onClose={props.onClose}
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
