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
 */
export function AccountsProviderMenu(props: { collapsed: boolean }): JSX.Element {
  return (
    <View style={{ width: "100%" }}>
      {props.collapsed ? (
        <View style={{ display: "flex", flexDirection: "column", gap: 4, width: "100%" }}>
          {railAgents().map((agent) => (
            <View
              testID={`rail-${agent.id}`}
              role="button"
              accessibilityLabel={agent.displayName}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                width: "100%",
                height: 32,
                borderRadius: 8,
                borderWidth: 1,
                borderColor:
                  railSelected() === agent.id ? paletteToken("accent") : "transparent",
                backgroundColor:
                  railSelected() === agent.id ? paletteToken("panel_active") : "transparent",
                cursor: "pointer",
              }}
              hoverStyle={
                railSelected() === agent.id
                  ? undefined
                  : { backgroundColor: paletteToken("panel_hover") }
              }
              onPress={() => railSelected.set(agent.id)}
            >
              <AgentIcon testID={`rail-icon-${agent.id}`} id={agent.id} size={18} />
            </View>
          ))}
        </View>
      ) : (
        <View style={{ display: "flex", flexDirection: "column", gap: 4, width: "100%" }}>
          {railAgents().map((agent) => (
            <View
              testID={`rail-${agent.id}`}
              role="button"
              accessibilityLabel={agent.displayName}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 10,
                width: "100%",
                minHeight: 32,
                paddingHorizontal: 10,
                borderRadius: 8,
                borderWidth: 1,
                borderColor:
                  railSelected() === agent.id ? paletteToken("accent") : "transparent",
                backgroundColor:
                  railSelected() === agent.id ? paletteToken("panel_active") : "transparent",
                cursor: "pointer",
              }}
              hoverStyle={
                railSelected() === agent.id
                  ? undefined
                  : { backgroundColor: paletteToken("panel_hover") }
              }
              onPress={() => railSelected.set(agent.id)}
            >
              <AgentIcon testID={`rail-icon-${agent.id}`} id={agent.id} size={16} />
              <Text
                style={{
                  flex: 1,
                  minWidth: 0,
                  numberOfLines: 1,
                  textOverflow: "ellipsis",
                  fontSize: 13,
                  fontWeight: railSelected() === agent.id ? "600" : "400",
                  color:
                    railSelected() === agent.id
                      ? paletteToken("accent_fg")
                      : paletteToken("fg"),
                }}
              >
                {agent.displayName}
              </Text>
              <Text
                style={{
                  fontSize: 11,
                  fontWeight: railSelected() === agent.id ? "600" : "400",
                  fontVariant: "tabular-nums",
                  color:
                    railSelected() === agent.id
                      ? paletteToken("accent_fg")
                      : paletteToken("fg_faint"),
                }}
              >
                {String(railCounts()[agent.id] ?? 0)}
              </Text>
            </View>
          ))}
        </View>
      )}
    </View>
  );
}
