// 左侧 agent 竖排 —— 账号页与侧栏共用的受管 agent 菜单（复用共用 SVG <AgentIcon> +
// 名称 + 账号数），选中态样式与 SkillsNav 对齐（panel_active 底色 + 1px accent 边框 +
// accent_fg 文字），未选中无底色、hover 加浅底。
//
// 三个信号是本模块的出口：账号页（index/card 栅格）读 railSelected 过滤，
// state 在每次 list 落地时同步 railCounts 与 railAgents；壳的侧栏槽位
// （AccountsProviderMenu，Sidebar.tsx 的 accounts 模式）复用同一批信号读同
// 一份清单——两处永远同源，不各存一份。

import { Text, View, signal, type JSX } from "nsh";
import { AgentIcon } from "../../chrome/agentRail";
import { paletteToken } from "../../services/themeMode";
import type { AgentRow } from "./types";

/** 桥给的受管 agent 行（state.start 写入）。 */
export const railAgents = signal<AgentRow[]>([]);
/** 当前选中的 agent id（左栏与壳侧栏共读）。 */
export const railSelected = signal<string | null>(null);
/** 每个 agent 的账号数（state.load 写入）。 */
export const railCounts = signal<Record<string, number>>({});

/**
 * 壳侧栏的 accounts 模式槽位（Sidebar.tsx 已接线）：collapsed 时只留图标。
 * 使用按 id 键控的 <Each> + <If> 保活每行及其 <AgentIcon>，折叠切换不再销毁重建 SVG 纹理。
 */
export function AccountsProviderMenu(props: { collapsed: boolean }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4, width: "100%" }}>
      <Each of={railAgents()} key={(agent) => agent.id}>
        {(agent) => <RailAgentRow agent={agent} collapsed={() => props.collapsed} />}
      </Each>
    </View>
  );
}

function RailAgentRow(props: {
  agent: () => AgentRow;
  collapsed: () => boolean;
}): JSX.Element {
  const active = () => railSelected() === props.agent().id;
  return (
    <View
      testID={`rail-${props.agent().id}`}
      role="button"
      accessibilityLabel={props.agent().displayName}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: props.collapsed() ? "center" : "flex-start",
        gap: props.collapsed() ? 0 : 10,
        width: "100%",
        height: props.collapsed() ? 32 : undefined,
        minHeight: 32,
        paddingHorizontal: props.collapsed() ? 0 : 10,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: active() ? paletteToken("accent") : "transparent",
        backgroundColor: active() ? paletteToken("panel_active") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={
        active() ? undefined : { backgroundColor: paletteToken("panel_hover") }
      }
      onPress={() => railSelected.set(props.agent().id)}
    >
      <AgentIcon
        testID={`rail-icon-${props.agent().id}`}
        id={props.agent().id}
        size={props.collapsed() ? 18 : 16}
      />
      <View
        style={{
          display: props.collapsed() ? "none" : "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          flex: 1,
          minWidth: 0,
        }}
      >
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 13,
            fontWeight: active() ? "600" : "400",
            color: active() ? paletteToken("accent_fg") : paletteToken("fg"),
          }}
        >
          {props.agent().displayName}
        </Text>
        <Text
          style={{
            fontSize: 11,
            fontWeight: active() ? "600" : "400",
            fontVariant: "tabular-nums",
            color: active() ? paletteToken("accent_fg") : paletteToken("fg_faint"),
          }}
        >
          {String(railCounts()[props.agent().id] ?? 0)}
        </Text>
      </View>
    </View>
  );
}
