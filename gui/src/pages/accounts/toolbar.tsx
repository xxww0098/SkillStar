// 账号页悬浮圆角工具栏（搜索 / 三态视图 / 筛选 / 排序 / + / 全部刷新 / 隐私）。
// 每个控件都来自 chrome/toolbar.tsx 的统一工具栏尺度（BAR_H 32 / BAR_RADIUS
// 10 / BAR_FONT 12 / BAR_ICON 14）：账号页不再自绘按钮，尺寸与「我的技能」
// 等其余五页逐像素一致。批量选择条见 selectionBar.tsx；下拉弹层复用
// Overlay 锚定模式与共享的 BarMenuPanel / BarMenuRow。

import { Overlay, Text, View, computed, signal, type JSX, type ViewRef } from "nsh";
import { BarMenuPanel, BarMenuRow, BarMenuTrigger } from "../../chrome/barMenu";
import {
  BAR_H,
  BarIconButton,
  BarSearchInput,
  BarSegmentSlot,
  BarSegmented,
  PAGE_BAR_CARD_H,
  PAGE_BAR_PAD_X,
  PAGE_BAR_RADIUS,
} from "../../chrome/toolbar";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  planKind,
  planOf,
  type AccountFilter,
  type AccountSortBy,
  type AccountViewMode,
} from "./logic";
import { AUTO_REFRESH_OPTIONS } from "./autoRefresh";
import type { AccountsState } from "./state";
import type { AccountView } from "./types";

const PLAN_FILTERS: AccountFilter[] = [
  "ultra",
  "pro",
  "plus",
  "lite",
  "team",
  "enterprise",
  "free",
];
const SORT_OPTIONS: AccountSortBy[] = ["default", "quota", "last_used", "created_at"];

function filterLabel(filter: AccountFilter, totalCount: number): string {
  switch (filter) {
    case "all":
      return tf("accountsPage.filterAll", { count: totalCount });
    case "reauth":
      return t("accountsPage.filterReauth");
    case "error":
      return t("accountsPage.filterQuotaError");
    default:
      return filter.toUpperCase();
  }
}

function sortLabel(sortBy: AccountSortBy): string {
  switch (sortBy) {
    case "default":
      return t("accountsPage.sortDefault");
    case "quota":
      return t("accountsPage.sortQuota");
    case "last_used":
      return t("accountsPage.sortLastUsed");
    case "created_at":
      return t("accountsPage.sortCreatedAt");
  }
}

/** 悬浮圆角工具栏：与 PageBar 共用卡片几何，控件全部走 BAR_* 尺度。 */
export function AccountsToolbar(props: {
  state: AccountsState;
  agentId: string;
  totalCount: number;
  allAgentRows: AccountView[];
}): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="acct-toolbar"
      desktop={{ windowDrag: true }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        flexWrap: "wrap",
        gap: 10,
        flex: 1,
        minWidth: 0,
        minHeight: PAGE_BAR_CARD_H,
        paddingHorizontal: PAGE_BAR_PAD_X,
        paddingVertical: 6,
        borderRadius: PAGE_BAR_RADIUS,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          flexWrap: "wrap",
          gap: 8,
          flexShrink: 1,
          minWidth: 0,
        }}
      >
        <SearchBox state={state} />
        <ViewModeSwitch state={state} />
        <FilterMenuButton
          state={state}
          totalCount={props.totalCount}
          allAgentRows={props.allAgentRows}
        />
        <SortMenuButton state={state} />
        <SortDirButton state={state} />
      </View>

      <View
        testID="accounts-toolbar-drag"
        style={{ flex: 1, minWidth: 16, height: BAR_H }}
      />

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          flexShrink: 0,
        }}
      >
        <BarIconButton
          testID="acct-toolbar-add"
          glyph="plus"
          tone="primary"
          tip={t("accountsPage.addAccount")}
          onPress={() => state.openAdd(props.agentId)}
        />

        <AutoRefreshMenuButton state={state} />

        <BarIconButton
          testID="acct-toolbar-refresh-all"
          glyph="refresh"
          label={t("accountsPage.refreshAll")}
          tip={() =>
            state.batchRefreshing()
              ? t("accountsPage.refreshingAll")
              : t("accountsPage.refreshAll")
          }
          loading={state.batchRefreshing()}
          disabled={
            props.totalCount === 0 ||
            state.busyId() !== null ||
            state.batchBusy() !== null ||
            state.batchRefreshing()
          }
          onPress={() => state.refreshAll()}
        />

        <BarIconButton
          testID="acct-toolbar-privacy"
          glyph={state.privacyMode() ? "eyeOff" : "eye"}
          label={state.privacyMode() ? t("accountsPage.privacyShow") : t("accountsPage.privacyHide")}
          tip={() =>
            state.privacyMode()
              ? t("accountsPage.privacyShow")
              : t("accountsPage.privacyHide")
          }
          active={state.privacyMode()}
          onPress={() => state.togglePrivacy()}
        />
      </View>
    </View>
  );
}

function SearchBox(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <BarSearchInput
      boxTestID="acct-search"
      inputTestID="acct-search-input"
      clearTestID="acct-search-clear"
      value={state.searchQuery()}
      placeholder={t("accountsPage.searchPlaceholder")}
      onChange={(next) => state.searchQuery.set(next)}
    />
  );
}

function ViewModeSwitch(props: { state: AccountsState }): JSX.Element {
  const modes: { mode: AccountViewMode; icon: "rows3" | "list" | "layoutGrid"; label: string }[] = [
    { mode: "compact", icon: "rows3", label: t("accountsPage.viewCompact") },
    { mode: "list", icon: "list", label: t("accountsPage.viewList") },
    { mode: "grid", icon: "layoutGrid", label: t("accountsPage.viewGrid") },
  ];
  return (
    <BarSegmented
      slots={modes.map((entry) => (
        <BarSegmentSlot
          testID={`acct-view-${entry.mode}`}
          glyph={entry.icon}
          accessibilityLabel={entry.label}
          selected={props.state.viewMode() === entry.mode}
          onPress={() => props.state.viewMode.set(entry.mode)}
        />
      ))}
    />
  );
}

function FilterMenuButton(props: {
  state: AccountsState;
  totalCount: number;
  allAgentRows: AccountView[];
}): JSX.Element {
  const menuOpen = signal(false);
  const triggerRef = signal<ViewRef | null>(null);
  const filterOptions = computed(() => {
    if (!menuOpen()) return [];
    const rows = props.allAgentRows;
    const tierCounts = new Map<AccountFilter, number>();
    let reauthCount = 0;
    let errorCount = 0;
    for (const row of rows) {
      if (row.requiresReauth) reauthCount += 1;
      if ((row.usage?.error ?? "").trim().length > 0) errorCount += 1;
      const kind = planKind(planOf(row));
      const bucket: AccountFilter | null =
        kind === "ultra" || kind === "ultra_lite"
          ? "ultra"
          : kind === "pro_lite" || kind === "none"
            ? null
            : kind;
      if (bucket !== null) {
        tierCounts.set(bucket, (tierCounts.get(bucket) ?? 0) + 1);
      }
    }
    const options: { key: AccountFilter; label: string; count?: number }[] = [
      {
        key: "all",
        label: tf("accountsPage.allCount", { count: props.totalCount }),
      },
    ];
    for (const tier of PLAN_FILTERS) {
      const count = tierCounts.get(tier) ?? 0;
      if (count > 0) {
        options.push({ key: tier, label: tier.toUpperCase(), count });
      }
    }
    options.push({
      key: "reauth",
      label: t("accountsPage.filterReauth"),
      count: reauthCount,
    });
    options.push({
      key: "error",
      label: t("accountsPage.filterQuotaError"),
      count: errorCount,
    });
    return options;
  });

  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <BarMenuTrigger
          testID="acct-filter-trigger"
          glyph="listFilter"
          label={() => filterLabel(props.state.filter(), props.totalCount)}
          active={() => props.state.filter() !== "all"}
          onPress={() => menuOpen.update((open) => !open)}
        />
      </View>

      <If when={menuOpen()}>
        <Overlay
          anchor={triggerRef()}
          placement="bottom"
          align="start"
          offset={6}
          onPressOutside={() => menuOpen.set(false)}
          onCancel={() => menuOpen.set(false)}
        >
          <BarMenuPanel testID="acct-filter-panel">
            <Each of={filterOptions()} key={(opt) => opt.key}>
              {(opt) => (
                <BarMenuRow
                  testID={`acct-filter-opt-${opt().key}`}
                  label={opt().label}
                  count={opt().count}
                  active={props.state.filter() === opt().key}
                  onPress={() => {
                    props.state.filter.set(opt().key);
                    menuOpen.set(false);
                  }}
                />
              )}
            </Each>
          </BarMenuPanel>
        </Overlay>
      </If>
    </View>
  );
}

function SortMenuButton(props: { state: AccountsState }): JSX.Element {
  const menuOpen = signal(false);
  const triggerRef = signal<ViewRef | null>(null);
  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <BarMenuTrigger
          testID="acct-sort-trigger"
          glyph="arrowUpDown"
          label={() => sortLabel(props.state.sortBy())}
          active={() => props.state.sortBy() !== "default"}
          onPress={() => menuOpen.update((open) => !open)}
        />
      </View>

      <If when={menuOpen()}>
        <Overlay
          anchor={triggerRef()}
          placement="bottom"
          align="start"
          offset={6}
          onPressOutside={() => menuOpen.set(false)}
          onCancel={() => menuOpen.set(false)}
        >
          <BarMenuPanel testID="acct-sort-panel">
            {SORT_OPTIONS.map((option) => (
              <BarMenuRow
                testID={`acct-sort-opt-${option}`}
                label={sortLabel(option)}
                active={props.state.sortBy() === option}
                onPress={() => {
                  props.state.sortBy.set(option);
                  menuOpen.set(false);
                }}
              />
            ))}
          </BarMenuPanel>
        </Overlay>
      </If>
    </View>
  );
}

function SortDirButton(props: { state: AccountsState }): JSX.Element {
  return (
    <BarIconButton
      testID="acct-sort-dir"
      glyph={props.state.sortDir() === "desc" ? "arrowDown" : "arrowUp"}
      label={props.state.sortDir() === "desc" ? t("accountsPage.sortDesc") : t("accountsPage.sortAsc")}
      tip={() =>
        props.state.sortDir() === "desc" ? t("accountsPage.sortDesc") : t("accountsPage.sortAsc")
      }
      onPress={() => props.state.toggleSortDir()}
    />
  );
}

function formatAutoRefreshOption(ms: number | null): string {
  if (ms === null) return t("accountsPage.autoRefreshMenuOff");
  if (ms < 60_000) return tf("accountsPage.autoRefreshSec", { n: Math.round(ms / 1000) });
  return tf("accountsPage.autoRefreshMin", { n: Math.round(ms / 60_000) });
}

function autoRefreshTriggerLabel(ms: number | null): string {
  if (ms === null) return t("accountsPage.autoRefreshOff");
  return tf("accountsPage.autoRefreshLabel", { label: formatAutoRefreshOption(ms) });
}

function AutoRefreshMenuButton(props: { state: AccountsState }): JSX.Element {
  const menuOpen = signal(false);
  const triggerRef = signal<ViewRef | null>(null);
  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <BarMenuTrigger
          testID="acct-auto-refresh-trigger"
          glyph="clock"
          label={() => autoRefreshTriggerLabel(props.state.autoRefreshInterval())}
          active={() => props.state.autoRefreshInterval() !== null}
          onPress={() => menuOpen.update((open) => !open)}
        />
      </View>

      <If when={menuOpen()}>
        <Overlay
          anchor={triggerRef()}
          placement="bottom"
          align="end"
          offset={6}
          onPressOutside={() => menuOpen.set(false)}
          onCancel={() => menuOpen.set(false)}
        >
          <BarMenuPanel testID="acct-auto-refresh-panel">
            <Each of={AUTO_REFRESH_OPTIONS} key={(opt) => String(opt ?? "off")}>
              {(opt) => (
                <BarMenuRow
                  testID={`acct-auto-refresh-opt-${opt() ?? "off"}`}
                  label={formatAutoRefreshOption(opt())}
                  active={props.state.autoRefreshInterval() === opt()}
                  onPress={() => {
                    props.state.setAutoRefreshInterval(opt());
                    menuOpen.set(false);
                  }}
                />
              )}
            </Each>
            <If when={props.state.autoRefreshInterval() !== null}>
              <View
                testID="acct-auto-refresh-preview"
                style={{
                  paddingHorizontal: 10,
                  paddingTop: 6,
                  paddingBottom: 4,
                  borderTopWidth: 1,
                  borderTopColor: paletteToken("border_soft"),
                }}
              >
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                  {(() => {
                    const step = props.state.autoRefreshStepSeconds();
                    const count = props.state.autoRefreshEligibleCount();
                    return step !== null
                      ? tf("accountsPage.autoRefreshPreview", { count, step })
                      : t("accountsPage.autoRefreshNoneEligible");
                  })()}
                </Text>
              </View>
            </If>
          </BarMenuPanel>
        </Overlay>
      </If>
    </View>
  );
}

export { AccountSelectionBar } from "./selectionBar";
