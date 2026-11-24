// 批量选择操作条：全选 / 已选计数 / 取消选择 / 居中「全部 N」+ 添加胶囊 / 批量刷新与删除。
// 从 toolbar.tsx 拆出以遵守单文件长度纪律；颜色全部采用 paletteToken()，不在组件体内预读 signal。

import { Text, View, computed, type JSX } from "nsh";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import type { AccountsState } from "./state";

/** 第二层：全选 + 已选计数 + 居中「全部 N」胶囊 + 右侧批量刷新/删除按钮。 */
export function AccountSelectionBar(props: {
  state: AccountsState;
  agentId: string;
  visibleIds: string[];
}): JSX.Element {
  const state = props.state;
  const selectedSet = computed(() => new Set(state.selectedIds()));
  const selectedVisible = computed(() =>
    props.visibleIds.filter((id) => selectedSet().has(id)),
  );
  const allSelected = computed(
    () => props.visibleIds.length > 0 && selectedVisible().length === props.visibleIds.length,
  );
  const partial = computed(
    () => selectedVisible().length > 0 && !allSelected(),
  );
  const checkGlyph = (): IconName =>
    allSelected() ? "squareCheck" : partial() ? "squareMinus" : "square";
  const refreshHeld = (): boolean => state.batchRefreshing();
  const deleteHeld = (): boolean => state.batchBusy() !== null;
  const singleHeld = (): boolean => state.busyId() !== null;
  const refreshBlocked = (): boolean => refreshHeld() || deleteHeld() || singleHeld();

  return (
    <View
      testID="acct-selection-bar"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        minHeight: 40,
        paddingHorizontal: 14,
        paddingVertical: 6,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        gap: 12,
        width: "100%",
        flexShrink: 0,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
        <View
          testID="acct-select-all"
          role="button"
          accessibilityLabel={t("accountsPage.selectAll")}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            paddingVertical: 3,
            paddingHorizontal: 4,
            borderRadius: 6,
            cursor: props.visibleIds.length === 0 ? "default" : "pointer",
            opacity: props.visibleIds.length === 0 ? 0.45 : 1,
          }}
          hoverStyle={
            props.visibleIds.length === 0
              ? undefined
              : { backgroundColor: paletteToken("panel_hover") }
          }
          onPress={
            props.visibleIds.length === 0
              ? undefined
              : () => state.toggleSelectAll(props.visibleIds)
          }
        >
          <Icon
            name={checkGlyph()}
            size={15}
            color={
              allSelected() || partial() ? paletteToken("accent") : paletteToken("fg_muted")
            }
          />
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {t("accountsPage.selectAll")}
          </Text>
        </View>
        <If when={selectedVisible().length > 0}>
          <>
            <Text
              testID="acct-selected-count"
              style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}
            >
              {tf("accountsPage.selectedCount", { count: selectedVisible().length })}
            </Text>
            <View
              testID="acct-select-clear"
              role="button"
              accessibilityLabel={t("accountsPage.clearSelection")}
              style={{
                paddingHorizontal: 8,
                paddingVertical: 3,
                borderRadius: 6,
                cursor: "pointer",
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
              onPress={() => state.clearSelection()}
            >
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                {t("accountsPage.clearSelection")}
              </Text>
            </View>
          </>
        </If>
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          paddingLeft: 10,
          paddingRight: 4,
          paddingVertical: 3,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("accent_soft_edge"),
          backgroundColor: paletteToken("accent_soft"),
        }}
      >
        <Text
          testID="acct-bar-total"
          style={{
            fontSize: 12,
            fontWeight: "600",
            fontVariant: "tabular-nums",
            color: paletteToken("accent_fg"),
          }}
        >
          {tf("accountsPage.allCount", { count: props.visibleIds.length })}
        </Text>
        <View tooltip={<Tip text={t("accountsPage.addAccount")} />}>
          <View
            testID="acct-bar-add"
            role="button"
            accessibilityLabel={t("accountsPage.addAccount")}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 20,
              height: 20,
              borderRadius: 999,
              backgroundColor: paletteToken("card"),
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            onPress={() => state.openAdd(props.agentId)}
          >
            <Icon name="plus" size={12} color={paletteToken("accent_fg")} />
          </View>
        </View>
      </View>

      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
        <If when={selectedVisible().length > 0}>
          <>
            <View
              testID="acct-batch-refresh"
              role="button"
              accessibilityLabel={t("accountsPage.batchRefresh")}
              accessibilityState={{
                busy: refreshHeld(),
                disabled: refreshBlocked() && !refreshHeld(),
              }}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 5,
                height: 28,
                paddingHorizontal: 10,
                borderRadius: 8,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("bg"),
                opacity: 1,
                cursor: refreshBlocked() ? "default" : "pointer",
              }}
              hoverStyle={
                refreshBlocked() ? undefined : { backgroundColor: paletteToken("panel_hover") }
              }
              onPress={refreshBlocked() ? undefined : () => state.refreshAll(selectedVisible())}
            >
              <IconSpin
                name="refresh"
                size={12}
                color={paletteToken("fg_muted")}
                spin={refreshHeld()}
              />
              <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg") }}>
                {t("accountsPage.batchRefresh")}
              </Text>
            </View>
            <View
              testID="acct-batch-delete"
              role="button"
              accessibilityLabel={t("accountsPage.batchDelete")}
              accessibilityState={{
                busy: deleteHeld(),
                disabled: deleteHeld() || singleHeld(),
              }}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 5,
                height: 28,
                paddingHorizontal: 10,
                borderRadius: 8,
                borderWidth: 1,
                borderColor: paletteToken("danger_border"),
                backgroundColor: paletteToken("danger_bg"),
                opacity: deleteHeld() ? 0.45 : 1,
                cursor: deleteHeld() || singleHeld() ? "default" : "pointer",
              }}
              hoverStyle={
                deleteHeld() || singleHeld()
                  ? undefined
                  : {
                      backgroundColor: paletteToken("danger_bg"),
                      borderColor: paletteToken("danger"),
                    }
              }
              onPress={
                deleteHeld() || singleHeld()
                  ? undefined
                  : () => state.requestBatchDelete(selectedVisible())
              }
            >
              <IconSpin
                name="trash"
                size={12}
                color={paletteToken("danger_fg")}
                spin={deleteHeld()}
              />
              <Text
                style={{
                  fontSize: 11,
                  fontWeight: "600",
                  color: paletteToken("danger_fg"),
                }}
              >
                {t("accountsPage.batchDelete")}
              </Text>
            </View>
          </>
        </If>
      </View>
    </View>
  );
}
