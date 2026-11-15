// 账号页悬浮圆角工具栏（搜索 / 三态视图 / 筛选 / 排序 / + / 全部刷新 / 隐私）。
// 批量选择条见 selectionBar.tsx；颜色全走 paletteToken()，下拉弹层复用 Overlay 锚定模式。

import { Overlay, Text, View, signal, type JSX, type ViewRef } from "nsh";
import { Input } from "nsh/base";
import { Icon, type IconName } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { PAGE_BAR_CARD_H, PAGE_BAR_PAD_X, PAGE_BAR_RADIUS } from "../../chrome/toolbar";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  matchesAccountFilter,
  type AccountFilter,
  type AccountSortBy,
  type AccountViewMode,
} from "./logic";
import type { AccountsState } from "./state";
import type { AccountView } from "./types";

const PLAN_FILTERS: AccountFilter[] = ["ultra", "pro", "plus", "team", "enterprise", "free"];
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

/** 悬浮圆角工具栏：对齐 cockpit-tools 的顶部操作胶囊。 */
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
        style={{ flex: 1, minWidth: 16, height: 32 }}
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
        <View tooltip={<Tip text={t("accountsPage.addAccount")} />}>
          <View
            testID="acct-toolbar-add"
            role="button"
            accessibilityLabel={t("accountsPage.addAccount")}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 32,
              height: 32,
              borderRadius: 10,
              cursor: "pointer",
              backgroundColor: paletteToken("accent"),
            }}
            hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
            onPress={() => state.openAdd(props.agentId)}
          >
            <Icon name="plus" size={16} color={paletteToken("on_accent")} />
          </View>
        </View>

        <View
          tooltip={
            <Tip
              text={
                state.batchBusy() === "refresh"
                  ? t("accountsPage.refreshingAll")
                  : t("accountsPage.refreshAll")
              }
            />
          }
        >
          <View
            testID="acct-toolbar-refresh-all"
            role="button"
            accessibilityLabel={t("accountsPage.refreshAll")}
            desktop={{ windowDrag: false }}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 32,
              height: 32,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("bg"),
              opacity:
                props.totalCount === 0 || state.busyId() !== null || state.batchBusy() === "delete"
                  ? 0.45
                  : 1,
              cursor:
                props.totalCount === 0 || state.busyId() !== null || state.batchBusy() !== null
                  ? "default"
                  : "pointer",
            }}
            hoverStyle={
              props.totalCount === 0 || state.busyId() !== null || state.batchBusy() !== null
                ? undefined
                : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
            }
            onPress={
              props.totalCount === 0 || state.busyId() !== null || state.batchBusy() !== null
                ? undefined
                : () => state.refreshAll()
            }
          >
            <IconSpin
              name="refresh"
              size={14}
              color={
                state.batchBusy() === "refresh" ? paletteToken("accent") : paletteToken("fg_muted")
              }
              spin={state.batchBusy() === "refresh"}
            />
          </View>
        </View>

        <View
          tooltip={
            <Tip
              text={
                state.privacyMode()
                  ? t("accountsPage.privacyShow")
                  : t("accountsPage.privacyHide")
              }
            />
          }
        >
          <View
            testID="acct-toolbar-privacy"
            role="button"
            accessibilityLabel={
              state.privacyMode() ? t("accountsPage.privacyShow") : t("accountsPage.privacyHide")
            }
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 32,
              height: 32,
              borderRadius: 10,
              borderWidth: 1,
              cursor: "pointer",
              borderColor: state.privacyMode()
                ? paletteToken("accent_soft_edge")
                : paletteToken("border"),
              backgroundColor: state.privacyMode()
                ? paletteToken("accent_soft")
                : paletteToken("bg"),
            }}
            hoverStyle={{
              backgroundColor: state.privacyMode()
                ? paletteToken("accent_soft")
                : paletteToken("panel_hover"),
            }}
            onPress={() => state.togglePrivacy()}
          >
            <Icon
              name={state.privacyMode() ? "eyeOff" : "eye"}
              size={14}
              color={state.privacyMode() ? paletteToken("accent") : paletteToken("fg_muted")}
            />
          </View>
        </View>
      </View>
    </View>
  );
}

function SearchBox(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="acct-search"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        width: 190,
        height: 32,
        paddingHorizontal: 10,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("input"),
      }}
    >
      <Icon name="search" size={14} color={paletteToken("fg_muted")} />
      <View style={{ flex: 1, minWidth: 0 }}>
        <Input
          testID="acct-search-input"
          value={state.searchQuery()}
          placeholder={t("accountsPage.searchPlaceholder")}
          onChange={(e: { value: string }) => state.searchQuery.set(e.value)}
          style={{
            fontSize: 12,
            color: paletteToken("fg"),
            padding: 0,
            backgroundColor: "transparent",
            borderWidth: 0,
          }}
        />
      </View>
      {state.searchQuery().length > 0 ? (
        <View
          testID="acct-search-clear"
          role="button"
          accessibilityLabel={t("common.clear")}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 18,
            height: 18,
            borderRadius: 999,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => state.searchQuery.set("")}
        >
          <Icon name="x" size={11} color={paletteToken("fg_muted")} />
        </View>
      ) : null}
    </View>
  );
}

function ViewModeSwitch(props: { state: AccountsState }): JSX.Element {
  const modes: { mode: AccountViewMode; icon: IconName; label: string }[] = [
    { mode: "compact", icon: "rows3", label: t("accountsPage.viewCompact") },
    { mode: "list", icon: "list", label: t("accountsPage.viewList") },
    { mode: "grid", icon: "layoutGrid", label: t("accountsPage.viewGrid") },
  ];
  return (
    <View
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 2,
        height: 32,
        padding: 2,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("well"),
        flexShrink: 0,
      }}
    >
      {modes.map((entry) => (
        <View tooltip={<Tip text={entry.label} />}>
          <View
            testID={`acct-view-${entry.mode}`}
            role="button"
            accessibilityLabel={entry.label}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 26,
              borderRadius: 7,
              cursor: "pointer",
              backgroundColor:
                props.state.viewMode() === entry.mode ? paletteToken("accent") : "transparent",
            }}
            hoverStyle={{
              backgroundColor:
                props.state.viewMode() === entry.mode
                  ? paletteToken("accent_hover")
                  : paletteToken("panel_hover"),
            }}
            onPress={() => props.state.viewMode.set(entry.mode)}
          >
            <Icon
              name={entry.icon}
              size={14}
              color={
                props.state.viewMode() === entry.mode
                  ? paletteToken("on_accent")
                  : paletteToken("fg_muted")
              }
            />
          </View>
        </View>
      ))}
    </View>
  );
}

function FilterMenuButton(props: {
  state: AccountsState;
  totalCount: number;
  allAgentRows: AccountView[];
}): JSX.Element {
  const menuOpen = signal(false);
  const triggerRef = signal<ViewRef | null>(null);
  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <View
          testID="acct-filter-trigger"
          role="button"
          accessibilityLabel={t("accountsPage.filterPlan")}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 32,
            paddingHorizontal: 10,
            borderRadius: 999,
            borderWidth: 1,
            cursor: "pointer",
            flexShrink: 0,
            borderColor:
              props.state.filter() !== "all"
                ? paletteToken("accent_soft_edge")
                : paletteToken("border"),
            backgroundColor:
              props.state.filter() !== "all" ? paletteToken("accent_soft") : paletteToken("bg"),
          }}
          hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
          onPress={() => menuOpen.set(!menuOpen())}
        >
          <Icon
            name="listFilter"
            size={13}
            color={
              props.state.filter() !== "all" ? paletteToken("accent") : paletteToken("fg_muted")
            }
          />
          <Text
            style={{
              fontSize: 12,
              fontWeight: "500",
              color: props.state.filter() !== "all" ? paletteToken("accent") : paletteToken("fg"),
            }}
          >
            {filterLabel(props.state.filter(), props.totalCount)}
          </Text>
          <Icon
            name="chevronDown"
            size={11}
            color={
              props.state.filter() !== "all" ? paletteToken("accent") : paletteToken("fg_muted")
            }
          />
        </View>
      </View>

      <If when={menuOpen()}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="acct-filter-dismiss"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => menuOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="bottom" align="start" offset={6}>
          <View
            testID="acct-filter-panel"
            style={{
              width: 196,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("card"),
              boxShadow: "0 8px 30px rgba(0,0,0,0.28)",
              display: "flex",
              flexDirection: "column",
              padding: 6,
              gap: 2,
            }}
          >
            {(() => {
              const rows = props.allAgentRows;
              const options: { key: AccountFilter; label: string; count: number }[] = [
                {
                  key: "all",
                  label: tf("accountsPage.allCount", { count: props.totalCount }),
                  count: props.totalCount,
                },
              ];
              for (const tier of PLAN_FILTERS) {
                const count = rows.filter((row) => matchesAccountFilter(row, tier)).length;
                if (count > 0) {
                  options.push({ key: tier, label: tier.toUpperCase(), count });
                }
              }
              options.push({
                key: "reauth",
                label: t("accountsPage.filterReauth"),
                count: rows.filter((row) => matchesAccountFilter(row, "reauth")).length,
              });
              options.push({
                key: "error",
                label: t("accountsPage.filterQuotaError"),
                count: rows.filter((row) => matchesAccountFilter(row, "error")).length,
              });
              return options.map((opt) => (
                <DropdownRow
                  testID={`acct-filter-opt-${opt.key}`}
                  label={opt.label}
                  count={opt.key === "all" ? undefined : opt.count}
                  active={props.state.filter() === opt.key}
                  onPress={() => {
                    props.state.filter.set(opt.key);
                    menuOpen.set(false);
                  }}
                />
              ));
            })()}
          </View>
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
        <View
          testID="acct-sort-trigger"
          role="button"
          accessibilityLabel={sortLabel(props.state.sortBy())}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 32,
            paddingHorizontal: 10,
            borderRadius: 999,
            borderWidth: 1,
            cursor: "pointer",
            flexShrink: 0,
            borderColor:
              props.state.sortBy() !== "default"
                ? paletteToken("accent_soft_edge")
                : paletteToken("border"),
            backgroundColor:
              props.state.sortBy() !== "default" ? paletteToken("accent_soft") : paletteToken("bg"),
          }}
          hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
          onPress={() => menuOpen.set(!menuOpen())}
        >
          <Icon
            name="arrowUpDown"
            size={13}
            color={
              props.state.sortBy() !== "default" ? paletteToken("accent") : paletteToken("fg_muted")
            }
          />
          <Text
            style={{
              fontSize: 12,
              fontWeight: "500",
              color:
                props.state.sortBy() !== "default" ? paletteToken("accent") : paletteToken("fg"),
            }}
          >
            {sortLabel(props.state.sortBy())}
          </Text>
          <Icon
            name="chevronDown"
            size={11}
            color={
              props.state.sortBy() !== "default" ? paletteToken("accent") : paletteToken("fg_muted")
            }
          />
        </View>
      </View>

      <If when={menuOpen()}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="acct-sort-dismiss"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => menuOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="bottom" align="start" offset={6}>
          <View
            testID="acct-sort-panel"
            style={{
              width: 196,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("card"),
              boxShadow: "0 8px 30px rgba(0,0,0,0.28)",
              display: "flex",
              flexDirection: "column",
              padding: 6,
              gap: 2,
            }}
          >
            {SORT_OPTIONS.map((option) => (
              <DropdownRow
                testID={`acct-sort-opt-${option}`}
                label={sortLabel(option)}
                active={props.state.sortBy() === option}
                onPress={() => {
                  props.state.sortBy.set(option);
                  menuOpen.set(false);
                }}
              />
            ))}
          </View>
        </Overlay>
      </If>
    </View>
  );
}

function SortDirButton(props: { state: AccountsState }): JSX.Element {
  return (
    <View
      tooltip={
        <Tip
          text={
            props.state.sortDir() === "desc" ? t("accountsPage.sortDesc") : t("accountsPage.sortAsc")
          }
        />
      }
    >
      <View
        testID="acct-sort-dir"
        role="button"
        accessibilityLabel={
          props.state.sortDir() === "desc" ? t("accountsPage.sortDesc") : t("accountsPage.sortAsc")
        }
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 32,
          height: 32,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("bg"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }}
        onPress={() => props.state.toggleSortDir()}
      >
        <Icon
          name={props.state.sortDir() === "desc" ? "arrowDown" : "arrowUp"}
          size={14}
          color={paletteToken("fg_muted")}
        />
      </View>
    </View>
  );
}

function DropdownRow(props: {
  testID: string;
  label: string;
  count?: number;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 30,
        paddingHorizontal: 8,
        gap: 6,
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <View style={{ width: 14, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {props.active ? <Icon name="check" size={12} color={paletteToken("accent")} /> : null}
      </View>
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          fontWeight: props.active ? "600" : "400",
          color: props.active ? paletteToken("accent") : paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
      {props.count !== undefined ? (
        <Text
          style={{
            fontSize: 11,
            fontVariant: "tabular-nums",
            color: paletteToken("fg_faint"),
          }}
        >
          {String(props.count)}
        </Text>
      ) : null}
    </View>
  );
}

export { AccountSelectionBar } from "./selectionBar";

