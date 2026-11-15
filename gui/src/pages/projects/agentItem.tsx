// Agent 行 — crates/ss-gpui/src/projects/agent_item.rs 对位：可折叠行 +
// 部署路径（安静 mono 文本，不做输入框）+ 模式胶囊（symlink/copy 原地换
// 色）+ 激活开关（开启态 ok 绿）。胶囊与开关都在可折叠行内：nsh 里带自
// 己 onPress 的子视图不冒泡（addDialog 同款），点击不会连带折叠行——
// GPUI 的 cx.stop_propagation()。
//
// 【nsh 纪律】本组件只收 stable 的 state/profile；一切动态量在 JSX 表达
// 式里经 state 信号读取（组件体只执行一次）。

import { Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentBadge } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { motionTransition } from "../../chrome/motion";
import type { AgentProfileRecord } from "./bridge";
import type { ProjectsState } from "./state";

/** agent_item.rs available_skills：Hub 里还没分配给该 agent 的技能。 */
export function availableSkills(state: ProjectsState, agentId: string): string[] {
  const assigned = state.agentSkills()[agentId] ?? [];
  return state.hubSkills().filter((name) => !assigned.includes(name));
}

/** agent_item.rs render_switch：开启态显式 ok 绿；滑块只用 transform 的
 * 80ms 过渡滑到另一端（motion.ts 通道二），降低动态效果时直接落位。 */
function AgentSwitch(props: { state: ProjectsState; agentId: string; enabled: boolean }): JSX.Element {
  return (
    <View
      testID={`switch-${props.agentId}`}
      style={{
        display: "flex",
        width: 32,
        height: 18,
        borderRadius: 9,
        borderWidth: 1,
        borderColor: props.enabled ? paletteToken("ok") : paletteToken("edge"),
        backgroundColor: props.enabled ? paletteToken("ok") : paletteToken("well"),
        alignItems: "center",
        padding: 2,
        cursor: "pointer",
        flexShrink: 0,
      }}
      hoverStyle={{
        backgroundColor: props.enabled ? paletteToken("ok") : paletteToken("panel_hover"),
      }}
      onPress={() => props.state.toggleAgent(props.agentId)}
    >
      <View
        style={{
          width: 12,
          height: 12,
          borderRadius: 6,
          backgroundColor: props.enabled ? paletteToken("panel") : paletteToken("fg_muted"),
          transform: props.enabled ? "translateX(14px)" : "translateX(0px)",
          ...motionTransition(),
        }}
      />
    </View>
  );
}

/** agent 品牌 SVG 徽标；未启用时压暗并去色。 */
function BrandGlyph(props: { profile: AgentProfileRecord; enabled: boolean }): JSX.Element {
  return (
    <View style={{ opacity: props.enabled ? 1 : 0.6, flexShrink: 0 }}>
      <AgentBadge
        id={props.profile.id}
        name={props.profile.displayName}
        icon={props.profile.icon}
        active={props.enabled}
        size={20}
      />
    </View>
  );
}

/** agent_item.rs render_agent_item：一行 agent 的全部交互。 */
export function AgentItem(props: {
  state: ProjectsState;
  profile: AgentProfileRecord;
}): JSX.Element {
  const state = props.state;
  const profile = props.profile;
  const agentId = profile.id;
  const relPath = profile.projectSkillsRel;
  // 派生读取放在 JSX 里（函数体只执行一次）：下面的 (() => {...})() 即时
  // 求值块只在首渲染算一次，响应式读取必须写成 JSX 内的表达式。
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        borderRadius: 10,
        borderWidth: 1,
        borderColor:
          state.expandedAgent() === agentId ? paletteToken("accent") : paletteToken("edge"),
        backgroundColor: paletteToken("card"),
        flexShrink: 0,
      }}
    >
      {/* 折叠头：点击整行展开/收起（agent_item.rs top）。 */}
      <View
        testID={`agent-top-${agentId}`}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          paddingHorizontal: 12,
          paddingVertical: 8,
          cursor: "pointer",
          gap: 12,
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={() =>
          state.expandedAgent.set(state.expandedAgent() === agentId ? null : agentId)
        }
      >
        <View
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            flex: 1,
            minWidth: 0,
          }}
        >
          <Icon
            name={state.expandedAgent() === agentId ? "chevronDown" : "chevronRight"}
            size={14}
            color={paletteToken("fg_muted")}
          />
          <BrandGlyph profile={profile} enabled={agentId in state.agentSkills()} />
          <Text
            style={{
              fontSize: 14,
              fontWeight: "600",
              color:
                agentId in state.agentSkills() ? paletteToken("fg") : paletteToken("fg_muted"),
              minWidth: 0,
              numberOfLines: 1,
            }}
          >
            {profile.displayName}
          </Text>
          <If when={agentId in state.agentSkills()}>
            <View
              style={{
                display: "flex",
                paddingHorizontal: 8,
                paddingVertical: 1,
                borderRadius: 999,
                backgroundColor: paletteToken("info_bg"),
                flexShrink: 0,
              }}
            >
              <Text
                style={{ fontSize: 12, fontWeight: "700", color: paletteToken("accent_fg") }}
              >
                {tf("projects.skillsCount", {
                  count: (state.agentSkills()[agentId] ?? []).length,
                })}
              </Text>
            </View>
          </If>
        </View>
        <View
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            flexShrink: 0,
          }}
        >
          {/* 部署目标：识别泳道的安静 mono 文本（agent_item.rs:158-169）。 */}
          <Text
            style={{
              fontSize: 12,
              maxWidth: 200,
              minWidth: 0,
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
            }}
          >
            {relPath}
          </Text>
          <If when={agentId in state.agentSkills()}>
            <View
              testID={`mode-toggle-${agentId}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                paddingHorizontal: 8,
                paddingVertical: 2,
                borderRadius: 6,
                borderWidth: 1,
                borderColor:
                  state.deployModes()[relPath] === "copy"
                    ? paletteToken("warn_border")
                    : paletteToken("info_border"),
                backgroundColor:
                  state.deployModes()[relPath] === "copy"
                    ? paletteToken("warn_bg")
                    : paletteToken("info_bg"),
                cursor: "pointer",
                flexShrink: 0,
              }}
              hoverStyle={{ opacity: 0.9 }}
              onPress={() => state.toggleDeployMode(relPath)}
            >
              <Icon
                name={state.deployModes()[relPath] === "copy" ? "copy" : "link"}
                size={11}
                color={
                  state.deployModes()[relPath] === "copy"
                    ? paletteToken("warn")
                    : paletteToken("accent_fg")
                }
              />
              <Text
                style={{
                  fontSize: 12,
                  color:
                    state.deployModes()[relPath] === "copy"
                      ? paletteToken("warn")
                      : paletteToken("accent_fg"),
                }}
              >
                {state.deployModes()[relPath] === "copy"
                  ? t("projects.deployCopy")
                  : t("projects.deploySymlink")}
              </Text>
            </View>
          </If>
          <AgentSwitch state={state} agentId={agentId} enabled={agentId in state.agentSkills()} />
        </View>
      </View>
      {/* 展开体：已分配 chips + Hub 添加区（仅展开且启用）。 */}
      <If when={state.expandedAgent() === agentId && agentId in state.agentSkills()}>
        <View
          testID={`agent-detail-${agentId}`}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 12,
            padding: 12,
            borderTopWidth: 1,
            borderTopColor: paletteToken("edge"),
            backgroundColor: paletteToken("panel"),
          }}
        >
          <If
            when={(state.agentSkills()[agentId] ?? []).length > 0}
            else={
              <Text
                style={{ fontSize: 12, fontStyle: "italic", color: paletteToken("fg_muted") }}
              >
                {t("projects.noSkillsAssigned")}
              </Text>
            }
          >
            <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 4 }}>
              <Each
                of={state.agentSkills()[agentId] ?? []}
                key={(skill: string) => skill}
              >
                {(skill: () => string) => (
                  <View
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 4,
                      paddingHorizontal: 8,
                      paddingVertical: 3,
                      borderRadius: 6,
                      backgroundColor: paletteToken("card"),
                      borderWidth: 1,
                      borderColor: paletteToken("border"),
                    }}
                  >
                    <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{skill()}</Text>
                    <View
                      testID={`rem-btn-${agentId}-${skill()}`}
                      style={{
                        padding: 1,
                        borderRadius: 4,
                        cursor: "pointer",
                      }}
                      hoverStyle={{ backgroundColor: paletteToken("danger_bg") }}
                      onPress={() => state.removeSkillFromAgent(agentId, skill())}
                    >
                      <Icon name="x" size={11} color={paletteToken("fg_muted")} />
                    </View>
                  </View>
                )}
              </Each>
            </View>
          </If>
          <If when={availableSkills(state, agentId).length > 0}>
            <AddFromHub state={state} agentId={agentId} />
          </If>
        </View>
      </If>
    </View>
  );
}

/** agent_item.rs 添加区：「+ 添加全部可用」与可用 chips。 */
function AddFromHub(props: { state: ProjectsState; agentId: string }): JSX.Element {
  const state = props.state;
  const agentId = props.agentId;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        paddingTop: 8,
        borderTopWidth: 1,
        borderTopColor: paletteToken("well"),
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
          {t("projects.addFromHub")}
        </Text>
        <View
          testID={`add-all-btn-${agentId}`}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            cursor: "pointer",
          }}
          hoverStyle={{ opacity: 0.8 }}
          onPress={() => {
            for (const name of availableSkills(state, agentId)) {
              state.addSkillToAgent(agentId, name);
            }
          }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>
            {t("projects.addAllAvailable")}
          </Text>
        </View>
      </View>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 4 }}>
        <Each of={availableSkills(state, agentId)} key={(skill: string) => skill}>
          {(skill: () => string) => (
            <View
              testID={`add-chip-${agentId}-${skill()}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                paddingHorizontal: 8,
                paddingVertical: 2,
                borderRadius: 6,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
                cursor: "pointer",
              }}
              hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
              onPress={() => state.addSkillToAgent(agentId, skill())}
            >
              <Icon name="plus" size={10} color={paletteToken("accent")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{skill()}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}
