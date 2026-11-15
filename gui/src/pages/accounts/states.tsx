// 账号页状态占位区：首次加载脉冲骨架、空态引导（全高虚线面板 + 品牌 SVG 图标）、无匹配筛选提示。
// 所有受管 agent 共用这一套骨架与空态原语，不按供应商分支。

import { Text, View, type JSX } from "nsh";
import { AgentIcon } from "../../chrome/agentRail";
import { Pulse } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { CARD_GAP, SKILL_CARD_W } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";

/** 首次列表拉取期间的 3 列脉冲骨架，与真实卡片共用同一 `SKILL_CARD_W` / `CARD_GAP` 轨道。 */
export function AccountsSkeletonGrid(): JSX.Element {
  return (
    <View
      testID="acct-skeleton"
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        width: "100%",
      }}
    >
      {[0, 1, 2].map((index) => (
        <View
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 10,
            width: SKILL_CARD_W,
            paddingVertical: 12,
            paddingHorizontal: 14,
            borderRadius: 16,
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
              justifyContent: "space-between",
              gap: 8,
            }}
          >
            <Pulse id={`acct-skel-${index}-title`} style={{ width: "56%", height: 14, borderRadius: 6 }} />
            <Pulse
              id={`acct-skel-${index}-badge`}
              secondary
              style={{ width: 46, height: 18, borderRadius: 999 }}
            />
          </View>
          <Pulse
            id={`acct-skel-${index}-sub`}
            secondary
            style={{ width: "74%", height: 11, borderRadius: 5 }}
          />
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 10,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <Pulse id={`acct-skel-${index}-q1`} style={{ width: "86%", height: 10, borderRadius: 5 }} />
            <Pulse
              id={`acct-skel-${index}-q2`}
              secondary
              style={{ width: "68%", height: 8, borderRadius: 4 }}
            />
          </View>
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
              paddingTop: 6,
            }}
          >
            <Pulse
              id={`acct-skel-${index}-meta`}
              secondary
              style={{ width: "40%", height: 10, borderRadius: 5 }}
            />
            <Pulse id={`acct-skel-${index}-act`} style={{ width: 58, height: 22, borderRadius: 999 }} />
          </View>
        </View>
      ))}
    </View>
  );
}

/** 某 agent 尚无任何账号时的居中空态引导卡片（全高虚线面板 + 品牌 SVG 图标）。 */
export function AccountsEmptyState(props: {
  agentId: string;
  agentName: string;
  onAdd: () => void;
}): JSX.Element {
  return (
    <View
      testID="acct-empty"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        width: "100%",
        // 撑满工具栏以下的整个窗口（页面列随滚动视口长高）；
        // 240 只是极矮窗口的下限。
        flexGrow: 1,
        minHeight: 240,
        paddingVertical: 32,
        paddingHorizontal: 24,
        borderRadius: 16,
        borderWidth: 1,
        borderStyle: "dashed",
        borderColor: paletteToken("edge"),
        backgroundColor: paletteToken("panel"),
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 44,
          height: 44,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
        }}
      >
        <AgentIcon testID={`acct-empty-icon-${props.agentId}`} id={props.agentId} size={22} />
      </View>

      <Text
        testID="acct-add-label"
        style={{
          fontSize: 15,
          fontWeight: "600",
          color: paletteToken("fg"),
          textAlign: "center",
        }}
      >
        {tf("accountsPage.emptyTitle", { agent: props.agentName })}
      </Text>

      <Text
        style={{
          maxWidth: 420,
          fontSize: 12,
          color: paletteToken("fg_muted"),
          textAlign: "center",
          lineHeight: 18,
        }}
      >
        {t("accountsPage.emptyHint")}
      </Text>

      <View
        testID={`acct-add-${props.agentId}`}
        role="button"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          height: 34,
          paddingHorizontal: 14,
          borderRadius: 999,
          backgroundColor: paletteToken("accent"),
          cursor: "pointer",
          marginTop: 4,
        }}
        hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
        onPress={props.onAdd}
      >
        <Icon name="plus" size={14} color={paletteToken("on_accent")} />
        <Text
          style={{
            fontSize: 13,
            fontWeight: "600",
            color: paletteToken("on_accent"),
          }}
        >
          {t("accountsPage.addAccount")}
        </Text>
      </View>
    </View>
  );
}

/** 搜索词或筛选条件未命中任何账号时的提示与清除按钮。 */
export function AccountsNoMatchState(props: { onClear: () => void }): JSX.Element {
  return (
    <View
      testID="acct-no-match"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        width: "100%",
        minHeight: 200,
        paddingVertical: 28,
        paddingHorizontal: 24,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
    >
      <Icon name="search" size={22} color={paletteToken("fg_faint")} />
      <Text
        style={{
          fontSize: 14,
          fontWeight: "600",
          color: paletteToken("fg"),
        }}
      >
        {t("accountsPage.noMatchingTitle")}
      </Text>
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg_muted"),
        }}
      >
        {t("accountsPage.noMatchingHint")}
      </Text>
      <View
        testID="acct-clear-filters"
        role="button"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: 30,
          paddingHorizontal: 12,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("panel"),
          cursor: "pointer",
          marginTop: 4,
        }}
        hoverStyle={{
          backgroundColor: paletteToken("panel_hover"),
          borderColor: paletteToken("edge"),
        }}
        onPress={props.onClear}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {t("accountsPage.clearFilters")}
        </Text>
      </View>
    </View>
  );
}
