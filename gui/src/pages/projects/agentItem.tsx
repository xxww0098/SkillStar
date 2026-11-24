// Agent 行 — crates/ss-gpui/src/projects/agent_item.rs 对位：可折叠行 +
// 部署路径（安静 mono 文本，不做输入框）+ 模式胶囊（symlink/copy 原地换
// 色）+ 激活开关（开启态 ok 绿）。胶囊与开关都在可折叠行内：nsh 里带自
// 己 onPress 的子视图不冒泡（addDialog 同款），点击不会连带折叠行——
// GPUI 的 cx.stop_propagation()。
//
// 【nsh 纪律】本组件只收 stable 的 state/profile；一切动态量在 JSX 表达
// 式里经 state 信号读取（组件体只执行一次）。

import { Text, View, computed, type Accessor, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentBadge } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { settleTransition } from "../../chrome/motion";
import type { AgentProfileRecord } from "./bridge";
import type { ProjectsState } from "./state";

const EMPTY_SKILLS: string[] = [];

function sameElements(prev: readonly string[], next: readonly string[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

/** agent_item.rs render_switch：开启态显式 ok 绿；滑块只用 transform 回弹
 * 到另一端（motion.ts settleTransition），降低动态效果时直接落位。 */
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
          ...settleTransition(),
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

/** agent_item.rs render_agent_item：一行 agent 的全部交互。
 * 行级状态经 computed 收窄：单行开关、技能增删或模式切换不再扇出到其余行。 */
export function AgentItem(props: {
  state: ProjectsState;
  profile: AgentProfileRecord;
}): JSX.Element {
  const state = props.state;
  const profile = props.profile;
  const agentId = profile.id;
  const relPath = profile.projectSkillsRel;
  const expanded = computed(() => state.expandedAgent() === agentId);
  const enabled = computed(() => agentId in state.agentSkills());
  const assignedSkills = computed(
    () => state.agentSkills()[agentId] ?? EMPTY_SKILLS,
    { equals: sameElements },
  );
  const assignedCount = computed(() => assignedSkills().length);
  const isCopy = computed(() => state.deployModes()[relPath] === "copy");
  const available = computed(
    () => {
      const assigned = new Set(assignedSkills());
      return state.hubSkills().filter((name) => !assigned.has(name));
    },
    { equals: sameElements },
  );

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        borderRadius: 10,
        borderWidth: 1,
        borderColor: expanded() ? paletteToken("accent") : paletteToken("edge"),
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
          state.expandedAgent.set(expanded() ? null : agentId)
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
            name={expanded() ? "chevronDown" : "chevronRight"}
            size={14}
            color={paletteToken("fg_muted")}
          />
          <BrandGlyph profile={profile} enabled={enabled()} />
          <Text
            style={{
              fontSize: 14,
              fontWeight: "600",
              color: enabled() ? paletteToken("fg") : paletteToken("fg_muted"),
              minWidth: 0,
              numberOfLines: 1,
            }}
          >
            {profile.displayName}
          </Text>
          <If when={enabled()}>
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
                  count: assignedCount(),
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
          <If when={enabled()}>
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
                borderColor: isCopy()
                  ? paletteToken("warn_border")
                  : paletteToken("info_border"),
                backgroundColor: isCopy()
                  ? paletteToken("warn_bg")
                  : paletteToken("info_bg"),
                cursor: "pointer",
                flexShrink: 0,
              }}
              hoverStyle={{ opacity: 0.9 }}
              onPress={() => state.toggleDeployMode(relPath)}
            >
              <Icon
                name={isCopy() ? "copy" : "link"}
                size={11}
                color={isCopy() ? paletteToken("warn") : paletteToken("accent_fg")}
              />
              <Text
                style={{
                  fontSize: 12,
                  color: isCopy() ? paletteToken("warn") : paletteToken("accent_fg"),
                }}
              >
                {isCopy()
                  ? t("projects.deployCopy")
                  : t("projects.deploySymlink")}
              </Text>
            </View>
          </If>
          <AgentSwitch state={state} agentId={agentId} enabled={enabled()} />
        </View>
      </View>
      {/* 展开体：已分配 chips + Hub 添加区（仅展开且启用）。 */}
      <If when={expanded() && enabled()}>
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
            when={assignedCount() > 0}
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
                of={assignedSkills()}
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
          <If when={available().length > 0}>
            <AddFromHub state={state} agentId={agentId} available={available} />
          </If>
        </View>
      </If>
    </View>
  );
}

/** agent_item.rs 添加区：「+ 添加全部可用」与可用 chips。 */
function AddFromHub(props: {
  state: ProjectsState;
  agentId: string;
  available: Accessor<string[]>;
}): JSX.Element {
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
          onPress={() => state.addAllSkillsToAgent(agentId, props.available())}
        >
          <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>
            {t("projects.addAllAvailable")}
          </Text>
        </View>
      </View>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 4 }}>
        <Each of={props.available()} key={(skill: string) => skill}>
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
