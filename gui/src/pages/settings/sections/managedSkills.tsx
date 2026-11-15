// 「智能体」行与展开「受管技能」面板：主行呈现品牌图标底座、名称、暂停状态胶囊、
// 等宽路径与共享提示、可折叠链接胶囊及主开关；展开面板内提供受管技能开关与解绑操作。

import { Text, View, type JSX } from "nsh";
import os from "os";
import { paletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { AgentIcon } from "../../../chrome/agentRail";
import { Icon } from "../../../chrome/icons";
import { ChoicePills, CollapseChevron, MONO_FONT, SettingsSwitch, StatusPill } from "../parts";
import { st } from "../settingsStrings";
import {
  displayPaths,
  hasGlobalSkills,
  managedTargetKeyOf,
  noticeFromReport,
  sharedNames,
  SKIP_UNMANAGED_REAL_DIRECTORY,
  type AgentConnectionsState,
  type AgentDeployModeChoice,
  type AgentProfileDto,
  type ManagedStateDto,
} from "./agentConnectionsState";

export { managedTargetKeyOf, noticeFromReport, SKIP_UNMANAGED_REAL_DIRECTORY };

export type PauseStatus = "loading" | "empty" | "active" | "paused" | "partial";
export type PauseAction = "pause" | "restore";

export interface PauseSnapshot {
  status: PauseStatus;
  active: string[];
  suspended: string[];
  action: PauseAction | null;
  checked: boolean;
}

function uniqueNames(names: string[]): string[] {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const raw of names) {
    const name = raw.trim();
    if (name === "" || seen.has(name)) continue;
    seen.add(name);
    unique.push(name);
  }
  return unique;
}

export function pauseSnapshot(state: ManagedStateDto | undefined): PauseSnapshot {
  if (state === undefined) {
    return { status: "loading", active: [], suspended: [], action: null, checked: false };
  }
  const active = uniqueNames(state.active_skill_names);
  const suspended = uniqueNames(state.suspended_skill_names);
  const hasActive = active.length > 0;
  const hasSuspended = suspended.length > 0;
  const status: PauseStatus = hasSuspended
    ? hasActive
      ? "partial"
      : "paused"
    : hasActive
      ? "active"
      : "empty";
  return {
    status,
    active,
    suspended,
    action: hasSuspended ? "restore" : hasActive ? "pause" : null,
    checked: status === "active",
  };
}

export function managedStatusText(pending: boolean, snapshot: PauseSnapshot): string {
  if (pending) return t("settings.managedSkillsUpdating");
  switch (snapshot.status) {
    case "loading":
      return t("settings.managedSkillsChecking");
    case "empty":
      return t("settings.noManagedSkills");
    case "paused":
      return tf("settings.managedSkillsPausedCount", { count: String(snapshot.suspended.length) });
    case "partial":
      return tf("settings.managedSkillsPartialCount", {
        paused: String(snapshot.suspended.length),
        active: String(snapshot.active.length),
      });
    case "active":
      return tf("settings.managedSkillsActiveCount", { count: String(snapshot.active.length) });
  }
}

function slug(text: string): string {
  return text.replace(/[^a-zA-Z0-9-]/g, "-");
}

function snapshotForAgent(state: AgentConnectionsState, agentId: string): PauseSnapshot {
  const profile = state.profiles().find((item) => item.id === agentId);
  if (profile === undefined || !hasGlobalSkills(profile)) {
    return pauseSnapshot(undefined);
  }
  return pauseSnapshot(state.managedStates()[managedTargetKeyOf(profile.global_skills_dir)]);
}

function managedStatusColor(pending: boolean, status: PauseStatus): string {
  if (pending) return paletteToken("accent_fg");
  if (status === "paused" || status === "partial") return paletteToken("warn");
  if (status === "active") return paletteToken("ok");
  return paletteToken("fg_muted");
}

/** 单条智能体行（主开关 + 可折叠链接详情面板）。 */
export function AgentRow(props: {
  state: AgentConnectionsState;
  agentId: string;
  last: boolean;
}): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        ...(props.last
          ? {}
          : { borderBottomWidth: 1, borderBottomColor: paletteToken("border_soft") }),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          paddingHorizontal: 18,
          paddingVertical: 13,
          width: "100%",
        }}
      >
        <AgentGlyph state={state} agentId={props.agentId} />
        <AgentIdentity state={state} agentId={props.agentId} />
        <AgentLinkedChip state={state} agentId={props.agentId} />
        <SettingsSwitch
          id={`agent-toggle-${slug(props.agentId)}`}
          checked={
            state
              .profiles()
              .find((profile) => profile.id === props.agentId)
              ?.enabled === true
          }
          onToggle={() => state.toggleAgentEnabled(props.agentId)}
        />
      </View>
      {state.expandedAgent() === props.agentId ? (
        <LinkedPanel state={state} agentId={props.agentId} />
      ) : null}
    </View>
  );
}

function AgentGlyph(props: { state: AgentConnectionsState; agentId: string }): JSX.Element {
  const state = props.state;
  const profile = (): AgentProfileDto | undefined =>
    state.profiles().find((item) => item.id === props.agentId);
  const enabled = (): boolean => profile()?.enabled === true;
  return (
    <View
      style={{
        width: 36,
        height: 36,
        flexShrink: 0,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: enabled() ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: enabled() ? paletteToken("accent_soft") : paletteToken("well"),
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <AgentIcon
        id={props.agentId}
        icon={profile()?.icon}
        size={18}
        grayscale={!enabled()}
        opacity={enabled() ? 1 : 0.6}
      />
    </View>
  );
}

function AgentIdentity(props: { state: AgentConnectionsState; agentId: string }): JSX.Element {
  const state = props.state;
  const profile = (): AgentProfileDto | undefined =>
    state.profiles().find((item) => item.id === props.agentId);
  const snapshot = (): PauseSnapshot => snapshotForAgent(state, props.agentId);
  return (
    <View
      style={{
        flex: 1,
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        gap: 3,
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          minWidth: 0,
        }}
      >
        <Text
          style={{
            fontSize: 14,
            fontWeight: "600",
            color: profile()?.enabled ? paletteToken("fg") : paletteToken("fg_muted"),
            numberOfLines: 1,
            flexShrink: 1,
            minWidth: 0,
          }}
        >
          {profile()?.display_name ?? props.agentId}
        </Text>
        {profile()?.enabled === true &&
        (snapshot().status === "paused" || snapshot().status === "partial") ? (
          <StatusPill
            tone="warn"
            text={tf("settings.managedSkillsPausedCount", {
              count: String(snapshot().suspended.length),
            })}
          />
        ) : null}
      </View>
      {(profile() ? displayPaths(profile()!, os.homedir()) : []).length > 0 ? (
        <Text
          style={{
            fontSize: 12,
            fontFamily: MONO_FONT,
            color: paletteToken("fg_muted"),
            numberOfLines: 1,
          }}
        >
          {displayPaths(profile()!, os.homedir()).join(" · ")}
        </Text>
      ) : null}
      {profile() && sharedNames(state.profiles(), profile()!).length > 0 ? (
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), numberOfLines: 1 }}>
          {tf("settings.sharedSkillsTarget", {
            names: sharedNames(state.profiles(), profile()!).join(", "),
          })}
        </Text>
      ) : null}
    </View>
  );
}

function AgentLinkedChip(props: {
  state: AgentConnectionsState;
  agentId: string;
}): JSX.Element | null {
  const state = props.state;
  const info = (): { linked: number; visible: boolean; expanded: boolean } => {
    const profile = state.profiles().find((item) => item.id === props.agentId);
    const linked = state.linkedSkills()[props.agentId]?.length ?? profile?.synced_count ?? 0;
    const suspended = snapshotForAgent(state, props.agentId).suspended.length;
    const expanded = state.expandedAgent() === props.agentId;
    return { linked, visible: linked > 0 || suspended > 0 || expanded, expanded };
  };
  return info().visible ? (
    <View
      testID={`agent-links-${slug(props.agentId)}`}
      role="button"
      style={{
        flexShrink: 0,
        height: 28,
        paddingHorizontal: 10,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: info().expanded ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: info().expanded ? paletteToken("accent_soft") : paletteToken("well"),
        cursor: "pointer",
      }}
      hoverStyle={
        info().expanded
          ? undefined
          : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
      }
      activeStyle={{ backgroundColor: paletteToken("card_active") }}
      onPress={() => state.toggleAgentExpand(props.agentId)}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          fontVariant: ["tabular-nums"],
          color: info().expanded ? paletteToken("accent_fg") : paletteToken("tag"),
          numberOfLines: 1,
        }}
      >
        {`${info().linked} ${t("settings.linked")}`}
      </Text>
      <CollapseChevron
        open={info().expanded}
        color={info().expanded ? paletteToken("accent_fg") : paletteToken("fg_muted")}
      />
    </View>
  ) : null;
}

/** 展开面板：受管技能暂停/恢复开关 + 一键取消全部链接 + 已链接技能列表。 */
export function LinkedPanel(props: { state: AgentConnectionsState; agentId: string }): JSX.Element {
  const state = props.state;
  const skills = (): string[] => state.linkedSkills()[props.agentId] ?? [];
  const profile = (): AgentProfileDto | undefined =>
    state.profiles().find((item) => item.id === props.agentId);
  const clearable = (): boolean => profile() !== undefined && hasGlobalSkills(profile()!);
  const pending = (): boolean =>
    clearable()
      ? state.managedPending()[managedTargetKeyOf(profile()!.global_skills_dir)] === true
      : false;
  return (
    <View
      style={{
        paddingHorizontal: 18,
        paddingVertical: 14,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      {clearable() ? (
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <ManagedSkillsRow state={state} agentId={props.agentId} />
          {skills().length > 0 ? (
            <View
              testID={`unlink-all-${slug(props.agentId)}`}
              role="button"
              style={{
                height: 28,
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 5,
                paddingHorizontal: 10,
                borderRadius: 8,
                borderWidth: 1,
                borderColor: pending() ? paletteToken("border") : paletteToken("danger_border"),
                backgroundColor: paletteToken("card"),
                cursor: pending() ? "default" : "pointer",
                ...(pending() ? { opacity: 0.5 } : {}),
              }}
              hoverStyle={pending() ? undefined : { backgroundColor: paletteToken("danger_bg") }}
              activeStyle={
                pending() ? undefined : { backgroundColor: paletteToken("danger_hover") }
              }
              onPress={() => {
                if (!pending()) state.unlinkAllLinkedSkills(props.agentId);
              }}
            >
              <Icon
                name="link2Off"
                size={12}
                color={pending() ? paletteToken("fg_muted") : paletteToken("danger")}
              />
              <Text
                style={{
                  fontSize: 12,
                  fontWeight: "500",
                  color: pending() ? paletteToken("fg_muted") : paletteToken("danger_fg"),
                }}
              >
                {t("settings.unlinkAllFromAgent")}
              </Text>
            </View>
          ) : null}
        </View>
      ) : null}
      {state.deployModeAvailable() ? (
        <AgentDeployModeRow state={state} agentId={props.agentId} />
      ) : null}
      {skills().length === 0 ? (
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t("settings.noSkillsLinked")}
        </Text>
      ) : (
        <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
          {skills().map((skill) => (
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 6,
                paddingLeft: 9,
                paddingRight: 5,
                paddingVertical: 4,
                borderRadius: 8,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
              }}
            >
              <Text
                style={{
                  fontSize: 12,
                  fontFamily: MONO_FONT,
                  color: paletteToken("fg"),
                  numberOfLines: 1,
                }}
              >
                {skill}
              </Text>
              <View
                testID={`unlink-${slug(props.agentId)}-${slug(skill)}`}
                role="button"
                accessibilityLabel={t("common.clear")}
                style={{
                  width: 18,
                  height: 18,
                  borderRadius: 5,
                  borderWidth: 1,
                  borderColor: "transparent",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  cursor: pending() ? "default" : "pointer",
                  ...(pending() ? { opacity: 0.5 } : {}),
                }}
                hoverStyle={
                  pending()
                    ? undefined
                    : {
                        backgroundColor: paletteToken("danger_bg"),
                        borderColor: paletteToken("danger_border"),
                      }
                }
                activeStyle={
                  pending() ? undefined : { backgroundColor: paletteToken("danger_hover") }
                }
                onPress={() => {
                  if (!pending()) state.unlinkLinkedSkill(props.agentId, skill);
                }}
              >
                <Icon name="x" size={10} color={paletteToken("fg_muted")} />
              </View>
            </View>
          ))}
        </View>
      )}
    </View>
  );
}

/** 受管技能开关行（展开面板内显示，避免列表主行出现双绿色开关混淆）。 */
export function ManagedSkillsRow(props: {
  state: AgentConnectionsState;
  agentId: string;
}): JSX.Element {
  const state = props.state;
  const snapshotOf = (): PauseSnapshot => snapshotForAgent(state, props.agentId);
  const pendingOf = (): boolean => {
    const profile = state.profiles().find((item) => item.id === props.agentId);
    if (profile === undefined || !hasGlobalSkills(profile)) return false;
    return state.managedPending()[managedTargetKeyOf(profile.global_skills_dir)] === true;
  };
  const disabled = (): boolean => {
    const snapshot = snapshotOf();
    return pendingOf() || snapshot.status === "loading" || snapshot.status === "empty";
  };
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        gap: 8,
        minWidth: 0,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg"), flexShrink: 0 }}>
        {t("settings.managedSkills")}
      </Text>
      <Text
        style={{
          minWidth: 0,
          numberOfLines: 1,
          fontSize: 12,
          fontWeight: "500",
          color: managedStatusColor(pendingOf(), snapshotOf().status),
        }}
      >
        {managedStatusText(pendingOf(), snapshotOf())}
      </Text>
      <SettingsSwitch
        testID={`managed-skills-${slug(props.agentId)}`}
        checked={snapshotOf().checked}
        disabled={disabled()}
        onToggle={() => state.toggleManagedSkills(props.agentId)}
      />
    </View>
  );
}

function AgentDeployModeRow(props: {
  state: AgentConnectionsState;
  agentId: string;
}): JSX.Element {
  const state = props.state;
  const slugId = (): string => props.agentId.toLowerCase().replace(/[^a-z0-9]/g, "-");
  const currentChoice = (): AgentDeployModeChoice =>
    state.agentDeployOverrides()[props.agentId] ?? "inherit";
  const disallowJunction = (): boolean =>
    state.disallowJunctionAgents().includes(props.agentId);
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flexWrap: "wrap",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          opacity: state.deployModePending() ? 0.65 : 1,
        }}
      >
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
          {st("settings.agentDeployMode")}
        </Text>
        <ChoicePills
          idPrefix={`agent-deploy-${slugId()}`}
          options={[
            { id: "inherit", label: st("settings.deployModeInherit") },
            { id: "symlink", label: st("settings.deployModeSymlink") },
            { id: "copy", label: st("settings.deployModeCopy") },
          ]}
          current={currentChoice()}
          onPick={(id) => {
            if (!state.deployModePending()) {
              state.setAgentDeployMode(props.agentId, id as AgentDeployModeChoice);
            }
          }}
        />
      </View>
      {disallowJunction() ? (
        <Text
          testID={`disallow-junction-${slugId()}`}
          style={{ fontSize: 11, color: paletteToken("fg_muted") }}
        >
          {st("settings.disallowJunctionHint")}
        </Text>
      ) : null}
    </View>
  );
}
