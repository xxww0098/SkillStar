// 项目详情列 — crates/ss-gpui/src/projects/detail.rs（头/三卡/页脚）+
// upkeep.rs（副本看护区）对位。滚动体是 flex_1 视口框：头部与页脚不滚
// （detail.rs 的 col/scroll/footer 三段）。三卡顺序硬契约：主任务卡（按
// 智能体管理技能）最上、部署方式卡只做静态图例、检测规则卡垫底；看护
// 区在最上（GPUI body 的第一个 child）。

import { Scroll, Text, View, computed, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { Icon, type IconName } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { Tip } from "../../chrome/tooltip";
import { errorAlert } from "../../chrome/alerts";
import { openChangePathDialog, openForgetCopyConfirm, openRemoveDialog } from "./dialogs";
import { AgentItem } from "./agentItem";
import { PendingSkillPlanSection } from "./pendingPlanCard";
import { copyStateKey, type ProjectsState } from "./state";
import type { CopyRecord } from "./bridge";

/** detail.rs render_project_detail 的整列。 */
export function ProjectDetail(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minWidth: 0,
        minHeight: 0,
        height: "100%",
      }}
    >
      <DetailHeader state={state} />
      {/* 滚动视口：列内滚轮不外泄由 Scroll 承担（overflow_y_scrollbar）。 */}
      <Scroll
        testID="projects-detail-scroll"
        style={{ width: "100%", flex: 1, minHeight: 0 }}
        contentStyle={{
          display: "flex",
          flexDirection: "column",
          justifyContent: "flex-start",
          gap: 16,
          padding: 16,
          width: "100%",
        }}
      >
        <PendingSkillPlanSection state={state} />
        <CopyStatusSection state={state} />
        <AgentsCard state={state} />
        <DeployModeCard />
        <RulesCard state={state} />
      </Scroll>
      <DetailFooter state={state} />
    </View>
  );
}

/** 详情头：徽标 + 名称/路径 + 更改路径/移除（busy 或 dirty 时锁）。 */
function DetailHeader(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "space-between",
        padding: 16,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        gap: 12,
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          minWidth: 0,
          flex: 1,
        }}
      >
        <View
          style={{
            width: 40,
            height: 40,
            borderRadius: 12,
            backgroundColor: paletteToken("accent_soft"),
            borderWidth: 1,
            borderColor: paletteToken("accent"),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <Icon name="folderOpen" size={20} color={paletteToken("accent_fg")} />
        </View>
        <View style={{ display: "flex", flex: 1, minWidth: 0 }}>
          <Text
            style={{
              fontSize: 14,
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {state.selected()?.name ?? ""}
          </Text>
          <Text
            style={{
              fontSize: 12,
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
            }}
          >
            {state.selected()?.path ?? ""}
          </Text>
        </View>
      </View>
      <View style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
        <SmallButton
          testID="proj-change-path"
          glyph="folder"
          label={t("projects.changePath")}
          disabled={state.mutationBlocked()}
          onPress={() => {
            const entry = state.selected();
            if (entry !== null) openChangePathDialog(state, entry);
          }}
        />
        <View tooltip={<Tip text={t("projects.removeProject")} />}>
          <SmallButton
            testID="proj-remove"
            glyph="trash"
            label={t("projects.removeProject")}
            danger
            disabled={state.mutationBlocked()}
            onPress={() => {
              const entry = state.selected();
              if (entry !== null) openRemoveDialog(state, entry.name);
            }}
          />
        </View>
      </View>
    </View>
  );
}

/** kit Button::small 的手绘对位：28px 高、图标 + 文案。disabled 经 props
 * 在 JSX 里读（调用方传的是 state 派生表达式，body 快照会冻死解锁时机）。 */
function SmallButton(props: {
  testID: string;
  glyph?: IconName;
  label: string;
  disabled?: boolean;
  danger?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        height: 28,
        paddingHorizontal: 10,
        borderRadius: 8,
        cursor: (props.disabled ?? false) ? "default" : "pointer",
        borderWidth: 1,
        borderColor:
          (props.disabled ?? false)
            ? paletteToken("border_soft")
            : props.danger === true
              ? paletteToken("danger_border")
              : paletteToken("border"),
        opacity: (props.disabled ?? false) ? 0.6 : 1,
      }}
      disabled={props.disabled ?? false}
      hoverStyle={
        (props.disabled ?? false) ? undefined : { backgroundColor: paletteToken("panel_hover") }
      }
      onPress={props.onPress}
    >
      <If when={Boolean(props.glyph)}>
        <Icon
          name={props.glyph!}
          size={13}
          color={
            (props.disabled ?? false)
              ? paletteToken("fg_faint")
              : props.danger === true
                ? paletteToken("danger")
                : paletteToken("fg")
          }
        />
      </If>
      <Text
        style={{
          fontSize: 12,
          color:
            (props.disabled ?? false)
              ? paletteToken("fg_faint")
              : props.danger === true
                ? paletteToken("danger")
                : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/** upkeep.rs render_copy_status：副本看护区。 */
function CopyStatusSection(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="project-copy-status"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
        flexShrink: 0,
        padding: 14,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
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
        <Text style={{ fontSize: 14, color: paletteToken("fg") }}>
          {t("projects.copyStatusTitle")}
        </Text>
        <View style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <SmallButton
            testID="project-copy-check"
            label={t("projects.checkCopies")}
            disabled={state.busy() || state.copyLoading()}
            onPress={() => state.refreshCopyStatus()}
          />
          <SmallButton
            testID="project-copy-retry"
            label={t("projects.retryCopies")}
            disabled={
              state.mutationBlocked() ||
              state.copyLoading() ||
              state.copyRecords().length === 0
            }
            onPress={() => state.retryCopies()}
          />
        </View>
      </View>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
        {t("projects.copyStatusHint")}
      </Text>
      <CopyProbeMark state={state} />
      <If when={state.dirty()}>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t("projects.copySaveFirst")}
        </Text>
      </If>
      <If
        when={state.copyRecords().some((record) => record.state === "project_missing")}
      >
        {errorAlert(
          tf("projects.copyProjectMissing", {
            name: state.selected()?.name ?? "",
            path: state.selected()?.path ?? "",
          })
        )}
      </If>
      <Each of={state.copyRecords()} key={(record: CopyRecord) => `${record.relativePath}/${record.skillName}`}>
        {(record: () => CopyRecord) => <CopyRecordRow state={state} record={record()} />}
      </Each>
    </View>
  );
}

/** 检查 / 重试的转圈晚 300ms 才出现。IconSpin 与 bindDelayedFlag 都在这个
 * 子组件体里（effect 不能写在父组件的 JSX 回调中）。失败留在本区域。 */
function CopyProbeMark(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  const show = bindDelayedFlag(() => state.copyLoading());
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
      <If when={state.copyError() !== null}>{errorAlert(state.copyError() ?? "")}</If>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
        <IconSpin
          name="loader"
          size={14}
          color={paletteToken("fg_muted")}
          spin={state.copyLoading()}
        />
        <If
          when={show() && state.copyError() === null}
          else={
            <If when={state.copyError() === null}>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {tf("projects.copyStatusCount", {
                  count: state.copyRecords().length,
                  pending: state.pendingCopyCount(),
                })}
              </Text>
            </If>
          }
        >
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {t("projects.checkingCopies")}
          </Text>
        </If>
      </View>
    </View>
  );
}

/** upkeep.rs 单条部署记录行：技能名/路径/状态/明细 + 待处理时的清理。
 * 记录的 state 会随重试/保存翻转：全部经 props 在 JSX 里读，body 不留快照。 */
function CopyRecordRow(props: { state: ProjectsState; record: CopyRecord }): JSX.Element {
  const state = props.state;
  // nsh 的 testID 选择器把 `.` 与 `/` 当选择器语法（GPUI 的 debug_selector
  // 无此限制）：记录键消毒成 [-A-Za-z0-9_] 才能被 id: 选中。
  const recordSlug = (record: CopyRecord): string =>
    `${record.relativePath}/${record.skillName}`
      .replace(/[^A-Za-z0-9_-]/g, "-")
      .replace(/^-+/, "");
  return (
    <View
      testID={`project-copy-row-${recordSlug(props.record)}`}
      style={{
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: 12,
        paddingVertical: 8,
        width: "100%",
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 4, flex: 1, minWidth: 0 }}>
        <Text style={{ fontSize: 14, color: paletteToken("fg") }}>{props.record.skillName}</Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {`${props.record.relativePath}/${props.record.skillName}`}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t(copyStateKey(props.record.state))}
        </Text>
        <If when={props.record.detail !== null}>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {props.record.detail}
          </Text>
        </If>
      </View>
      <If when={props.record.state !== "current"}>
        {() => (
          <SmallButton
            testID={`project-copy-forget-${recordSlug(props.record)}`}
            label={t("projects.forgetCopy")}
            disabled={state.mutationBlocked() || state.copyLoading()}
            onPress={() => openForgetCopyConfirm(state, props.record)}
          />
        )}
      </If>
    </View>
  );
}

/** detail.rs Section 1：主任务卡（按智能体管理技能）。 */
function AgentsCard(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="agents-card"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
        flexShrink: 0,
        padding: 14,
        borderRadius: 16,
        backgroundColor: paletteToken("card"),
        borderWidth: 1,
        borderColor: paletteToken("border"),
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {t("projects.perAgentTitle")}
        </Text>
        <If when={state.visibleAgents().length > 0}>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {tf("projects.agentsEnabledTotal", {
              enabled: Object.keys(state.agentSkills()).length,
              total: state.visibleAgents().length,
            })}
          </Text>
        </If>
      </View>
      <If
        when={state.visibleAgents().length === 0}
        else={
          <View style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
            <Each
              of={state.visibleAgents()}
              key={(profile) => profile.id}
            >
              {(profile) => <AgentItem state={state} profile={profile()} />}
            </Each>
          </View>
        }
      >
        <Text style={{ fontSize: 12, fontStyle: "italic", color: paletteToken("fg_muted") }}>
          {t("projects.noAgentsEnabled")}
        </Text>
      </If>
    </View>
  );
}

/** detail.rs Section 2：部署方式静态图例（无可交互件）。 */
function DeployModeCard(): JSX.Element {
  const modeRow = (
    glyph: IconName,
    iconColor: string,
    nameKey: string,
    hintKey: string
  ): JSX.Element => (
    <View style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <Icon name={glyph} size={13} color={iconColor} />
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {t(nameKey)}
      </Text>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{t(hintKey)}</Text>
    </View>
  );
  return (
    <View
      testID="projects-deploy-mode-card"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        width: "100%",
        flexShrink: 0,
        paddingHorizontal: 14,
        paddingVertical: 10,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
    >
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{t("projects.deployMode")}</Text>
      {modeRow("link", paletteToken("accent_fg"), "projects.deploySymlink", "projects.deployModeSymlinkHint")}
      {modeRow("copy", paletteToken("warn"), "projects.deployCopy", "projects.deployModeCopyHint")}
    </View>
  );
}

/** detail.rs Section 3：检测到的智能体规则与配置（垫底）。 */
function RulesCard(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="projects-rules-card"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
        flexShrink: 0,
        padding: 14,
        borderRadius: 16,
        backgroundColor: paletteToken("card"),
        borderWidth: 1,
        borderColor: paletteToken("border"),
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <View style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <Icon name="scan" size={14} color={paletteToken("accent_fg")} />
          <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
            {t("projects.rulesTitle")}
          </Text>
        </View>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {tf("projects.rulesFound", { count: state.detectedRules().length })}
        </Text>
      </View>
      <If
        when={state.detectedRules().length === 0}
        else={
          <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 4 }}>
            <Each of={state.detectedRules()} key={(rule: string) => rule}>
              {(rule: () => string) => (
                <View
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 4,
                    paddingHorizontal: 8,
                    paddingVertical: 3,
                    borderRadius: 6,
                    borderWidth: 1,
                    borderColor: paletteToken("edge"),
                    backgroundColor: paletteToken("well"),
                  }}
                >
                  <Icon name="fileText" size={12} color={paletteToken("accent_fg")} />
                  <Text
                    style={{
                      fontSize: 12,
                      fontWeight: "500",
                      color: paletteToken("fg"),
                    }}
                  >
                    {rule()}
                  </Text>
                </View>
              )}
            </Each>
          </View>
        }
      >
        <Text style={{ fontSize: 12, fontStyle: "italic", color: paletteToken("fg_muted") }}>
          {t("projects.noRules")}
        </Text>
      </If>
    </View>
  );
}

/** detail.rs Section 4：页脚（配置摘要 + 保存并同步）。 */
function DetailFooter(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  const totalSkills = computed(() =>
    Object.values(state.agentSkills()).reduce((sum, list) => sum + list.length, 0)
  );
  const enabledAgents = computed(() => Object.keys(state.agentSkills()).length);
  return (
    <View
      style={{
        display: "flex",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "space-between",
        padding: 16,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        gap: 12,
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        <View style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
            {tf("projects.footerSummary", { skills: totalSkills(), agents: enabledAgents() })}
          </Text>
          <If when={state.dirty()}>
            <View
              testID="projects-unsaved-badge"
              style={{
                paddingHorizontal: 4,
                paddingVertical: 1,
                borderRadius: 4,
                backgroundColor: paletteToken("warn_bg"),
                borderWidth: 1,
                borderColor: paletteToken("warn_border"),
              }}
            >
              <Text style={{ fontSize: 12, color: paletteToken("warn") }}>
                {t("projects.unsavedBadge")}
              </Text>
            </View>
          </If>
        </View>
        <If when={state.statusMessage() !== null}>
          <Text
            style={{
              fontSize: 12,
              color: state.statusIsError() ? paletteToken("danger") : paletteToken("accent_fg"),
            }}
          >
            {state.statusMessage()}
          </Text>
        </If>
      </View>
      {/* canApply 时保持实色，不换成 loader；busy 只吞连点。结论留在状态行。 */}
      <View
        testID="proj-save-sync"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          height: 32,
          paddingHorizontal: 14,
          borderRadius: 10,
          cursor: state.busy() || !state.canApply() ? "default" : "pointer",
          borderWidth: 1,
          borderColor: state.canApply() ? "transparent" : paletteToken("border"),
          backgroundColor: state.canApply() ? paletteToken("accent") : "transparent",
          opacity: state.canApply() ? 1 : 0.6,
        }}
        hoverStyle={
          state.canApply() && !state.busy()
            ? { backgroundColor: paletteToken("accent_hover") }
            : undefined
        }
        disabled={state.busy() || !state.canApply()}
        onPress={() => {
          if (state.busy()) return;
          state.saveAndSync();
        }}
      >
        <IconSpin
          name="check"
          size={14}
          color={state.canApply() ? paletteToken("on_accent") : paletteToken("fg_muted")}
          spin={state.busy()}
        />
        <Text
          style={{
            fontSize: 12,
            fontWeight: "500",
            color: state.canApply() ? paletteToken("on_accent") : paletteToken("fg_muted"),
          }}
        >
          {t("projects.saveAndSync")}
        </Text>
      </View>
    </View>
  );
}

/** 供宿主页复用：detail 列对未选中项目的替身（mod.rs 空态）。 */
export function SelectProjectHint(): JSX.Element {
  return (
    <View
      testID="projects-select-hint"
      style={{
        display: "flex",
        flex: 1,
        minWidth: 0,
        minHeight: 0,
        height: "100%",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        paddingHorizontal: 24,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 16,
          backgroundColor: paletteToken("accent_soft"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="briefcase" size={28} color={paletteToken("accent_fg")} />
      </View>
      <Text style={{ fontSize: 18, fontWeight: "700", color: paletteToken("fg") }}>
        {t("projects.selectProjectTitle")}
      </Text>
      <Text
        style={{
          fontSize: 14,
          maxWidth: 280,
          textAlign: "center",
          color: paletteToken("fg_muted"),
        }}
      >
        {t("projects.selectProjectDesc")}
      </Text>
    </View>
  );
}
