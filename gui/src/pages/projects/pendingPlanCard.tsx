import { Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { errorAlert } from "../../chrome/alerts";
import { Icon } from "../../chrome/icons";
import type { PendingPlanOperation } from "./bridge";
import { pt } from "./projectsStrings";
import type { ProjectsState } from "./state";

/**
 * 选中项目存在未过期的项目技能 MCP 计划时，在详情列表上方展示规范路径、
 * 是否注册、shared path owner、受影响 Agent 与每项计划操作，并提供
 * 「批准计划」按钮（仅写入 SkillStar 批准，不触发部署）。无计划时不渲染。
 */
export function PendingSkillPlanSection(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <If when={state.pendingPlan() !== null}>
      {() => (
        <View
          testID="project-pending-plan"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 10,
            width: "100%",
            flexShrink: 0,
            padding: 14,
            borderRadius: 16,
            borderWidth: 1,
            borderColor: paletteToken("accent"),
            backgroundColor: paletteToken("panel"),
          }}
        >
          <View
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
            }}
          >
            <View style={{ display: "flex", alignItems: "center", gap: 8, flex: 1, minWidth: 0 }}>
              <Icon name="badgeCheck" size={16} color={paletteToken("accent_fg")} />
              <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
                {pt("projects.plan.title")}
              </Text>
            </View>
            <If
              when={state.pendingPlan()?.approved === true}
              else={
                <View
                  testID="project-plan-approve"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    height: 28,
                    paddingHorizontal: 12,
                    borderRadius: 8,
                    cursor: state.planApproving() ? "default" : "pointer",
                    backgroundColor: state.planApproving()
                      ? paletteToken("panel_hover")
                      : paletteToken("accent"),
                    opacity: state.planApproving() ? 0.7 : 1,
                  }}
                  disabled={state.planApproving()}
                  onPress={() => {
                    if (state.planApproving()) return;
                    void state.approvePendingPlan();
                  }}
                >
                  <Icon
                    name="check"
                    size={13}
                    color={
                      state.planApproving() ? paletteToken("fg_muted") : paletteToken("on_accent")
                    }
                  />
                  <Text
                    style={{
                      fontSize: 12,
                      fontWeight: "600",
                      color: state.planApproving()
                        ? paletteToken("fg_muted")
                        : paletteToken("on_accent"),
                    }}
                  >
                    {state.planApproving()
                      ? pt("projects.plan.approvingBtn")
                      : pt("projects.plan.approveBtn")}
                  </Text>
                </View>
              }
            >
              <View
                testID="project-plan-approved-badge"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  height: 24,
                  paddingHorizontal: 10,
                  borderRadius: 999,
                  backgroundColor: paletteToken("accent_soft"),
                  borderWidth: 1,
                  borderColor: paletteToken("accent"),
                }}
              >
                <Icon name="check" size={12} color={paletteToken("accent_fg")} />
                <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>
                  {pt("projects.plan.approvedBadge")}
                </Text>
              </View>
            </If>
          </View>

          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {state.pendingPlan()?.approved === true
              ? pt("projects.plan.approvedHint")
              : pt("projects.plan.hint")}
          </Text>

          <If when={state.planError() !== null}>
            {errorAlert(state.planError() ?? "")}
          </If>

          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              padding: 10,
              borderRadius: 10,
              backgroundColor: paletteToken("card"),
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
            }}
          >
            <MetaRow
              testID="project-plan-root"
              label={pt("projects.plan.canonicalRoot")}
              value={state.pendingPlan()?.canonicalRoot ?? ""}
            />
            <MetaRow
              testID="project-plan-registered"
              label={pt("projects.plan.registered")}
              value={
                state.pendingPlan()?.isRegistered === true
                  ? pt("projects.plan.registeredYes")
                  : pt("projects.plan.registeredNo")
              }
            />
            <MetaRow
              testID="project-plan-owner"
              label={pt("projects.plan.owner")}
              value={state.pendingPlan()?.owner ?? ""}
            />
            <MetaRow
              testID="project-plan-agents"
              label={pt("projects.plan.affectedAgents")}
              value={(state.pendingPlan()?.affectedAgents ?? []).join(", ")}
            />
          </View>

          <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {pt("projects.plan.operations")}
            </Text>
            <Each
              of={state.pendingPlan()?.operations ?? []}
              key={(op: PendingPlanOperation) => `${op.name}:${op.skillPath}`}
            >
              {(op: () => PendingPlanOperation) => (
                <View
                  testID={`project-plan-op-${op().name}`}
                  style={{
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                    paddingVertical: 6,
                    paddingHorizontal: 8,
                    borderRadius: 8,
                    backgroundColor: paletteToken("card"),
                    borderWidth: 1,
                    borderColor: paletteToken("border_soft"),
                  }}
                >
                  <View style={{ display: "flex", flexDirection: "column", flex: 1, minWidth: 0 }}>
                    <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                      {op().name}
                    </Text>
                    <Text
                      style={{
                        fontSize: 11,
                        color: paletteToken("fg_muted"),
                        numberOfLines: 1,
                      }}
                    >
                      {op().skillPath}
                    </Text>
                  </View>
                  <Text style={{ fontSize: 11, color: paletteToken("accent_fg") }}>
                    {op().action === "already"
                      ? pt("projects.plan.actionAlready")
                      : pt("projects.plan.actionCreate")}
                  </Text>
                </View>
              )}
            </Each>
          </View>
        </View>
      )}
    </If>
  );
}

function MetaRow(props: { testID: string; label: string; value: string }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
      }}
    >
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flexShrink: 0 }}>
        {props.label}
      </Text>
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg"),
          flex: 1,
          textAlign: "right",
          numberOfLines: 1,
        }}
      >
        {props.value}
      </Text>
    </View>
  );
}
