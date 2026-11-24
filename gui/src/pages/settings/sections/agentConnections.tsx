// 「智能体」分区 UI — 双分组卡布局：顶部搜索与分段过滤栏 + 下方智能体列表卡；
// 行渲染与受管技能面板在 managedSkills.tsx；状态机与纯计算在 agentConnectionsState.ts。

import { Text, View, computed, effect, type JSX } from "nsh";
import os from "os";
import { Input } from "nsh/base";
import { paletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { Icon, type IconName } from "../../../chrome/icons";
import {
  ChoicePills,
  card,
  CollapseChevron,
  SectionShell,
  SettingsButton,
  StatusPill,
} from "../parts";
import { st } from "../settingsStrings";
import { SETTINGS_SECTIONS } from "../sections";
import {
  createAgentConnectionsState,
  displayPaths,
  hasGlobalSkills,
  homePrefix,
  matchedAgents,
  realAgentConnectionsBridge,
  sharedNames,
  sharedNamesByDir,
  statusCounts,
  VISIBLE_AGENTS,
  type AgentConnectionsBridge,
  type AgentConnectionsState,
  type AgentDeployModeChoice,
  type AgentFilter,
  type AgentNotice,
  type AgentProfileDto,
  type DeployMode,
  type DeployModesDto,
  type ManagedFailureDto,
  type ManagedReportDto,
  type ManagedSkipDto,
  type ManagedStateDto,
  type ReconcileModeReportDto,
} from "./agentConnectionsState";
import { AgentRow } from "./managedSkills";

export {
  createAgentConnectionsState,
  displayPaths,
  hasGlobalSkills,
  homePrefix,
  matchedAgents,
  realAgentConnectionsBridge,
  sharedNames,
  sharedNamesByDir,
  statusCounts,
  VISIBLE_AGENTS,
  type AgentConnectionsBridge,
  type AgentConnectionsState,
  type AgentDeployModeChoice,
  type AgentFilter,
  type AgentNotice,
  type AgentProfileDto,
  type DeployMode,
  type DeployModesDto,
  type ManagedFailureDto,
  type ManagedReportDto,
  type ManagedSkipDto,
  type ManagedStateDto,
  type ReconcileModeReportDto,
};

const AGENT_FILTERS: { id: AgentFilter; labelKey: string; countIndex: 0 | 1 | 2 }[] = [
  { id: "all", labelKey: "settings.filterAgentsAll", countIndex: 0 },
  { id: "enabled", labelKey: "settings.filterAgentsEnabled", countIndex: 1 },
  { id: "disabled", labelKey: "settings.filterAgentsDisabled", countIndex: 2 },
];

const pageState = createAgentConnectionsState(realAgentConnectionsBridge);

export function AgentConnectionsSection(): JSX.Element {
  return <AgentConnectionsWorkbench state={pageState} />;
}

export function AgentConnectionsWorkbench(props: { state: AgentConnectionsState }): JSX.Element {
  const state = props.state;
  effect(() => {
    void state.start();
  });
  const matched = computed(() =>
    matchedAgents(state.profiles(), state.agentQuery(), state.agentFilter()),
  );
  return (
    <SectionShell
      section={SETTINGS_SECTIONS.find((section) => section.id === "agents")!}
      grouped
      meta={
        <StatusPill
          testID="agents-active-pill"
          tone="accent"
          mono
          text={tf("settings.activeCount", {
            enabled: String(state.profiles().filter((profile) => profile.enabled).length),
            total: String(state.profiles().length),
          })}
        />
      }
    >
      <If
        when={state.profiles().length === 0}
        else={
          <>
            <If when={state.deployModeAvailable()}>
              <DeployModeBar state={state} />
            </If>
            <AgentFilterBar state={state} />
            <View style={card()}>
              <If when={matched().length === 0} else={<AgentRows state={state} matched={matched} />}>
                <AgentEmptyFilters state={state} />
              </If>
            </View>
          </>
        }
      >
        <View style={card()}>
          <AgentEmptyState icon="bot" message={t("settings.noAgentsRegistered")} />
        </View>
      </If>
    </SectionShell>
  );
}

/** 检索与状态过滤工具栏卡。 */
function AgentFilterBar(props: { state: AgentConnectionsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={card({
        padding: 14,
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        gap: 10,
      })}
    >
      <View
        desktop={{ windowDrag: false }}
        style={{
          flex: 1,
          minWidth: 160,
          height: 36,
          paddingHorizontal: 10,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: state.agentQuery() !== "" ? paletteToken("edge") : paletteToken("border"),
          backgroundColor: paletteToken("input"),
        }}
        focusWithinStyle={{ borderColor: paletteToken("edge") }}
      >
        <Icon
          name="search"
          size={14}
          color={state.agentQuery() !== "" ? paletteToken("accent") : paletteToken("fg_muted")}
        />
        <View style={{ flex: 1, minWidth: 0 }}>
          <Input
            testID="agent-search"
            style={{
              fontSize: 13,
              color: paletteToken("fg"),
              padding: 0,
              backgroundColor: "transparent",
              borderWidth: 0,
            }}
            value={state.agentQuery()}
            placeholder={t("settings.searchAgentsPlaceholder")}
            submitBehavior="blurAndSubmit"
            onChange={(event: { value: string }) => state.editAgentQuery(event.value)}
          />
        </View>
        <If when={state.agentQuery() !== ""}>
          <View
            testID="agent-search-clear"
            role="button"
            accessibilityLabel={t("common.clear")}
            style={{
              width: 20,
              height: 20,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 999,
              backgroundColor: paletteToken("well"),
              flexShrink: 0,
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            activeStyle={{ backgroundColor: paletteToken("border") }}
            onPress={() => state.editAgentQuery("")}
          >
            <Icon name="x" size={11} color={paletteToken("fg_muted")} />
          </View>
        </If>
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 3,
          height: 36,
          padding: 3,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
          flexShrink: 0,
        }}
      >
        {(() => {
          const counts = computed(() => statusCounts(state.profiles(), state.agentQuery()));
          return AGENT_FILTERS.map((filter) => (
            <AgentFilterPill
              testID={`agent-filter-${filter.id}`}
              label={t(filter.labelKey)}
              count={counts()[filter.countIndex]}
              active={state.agentFilter() === filter.id}
              onPress={() => state.setAgentFilter(filter.id)}
            />
          ));
        })()}
      </View>
    </View>
  );
}

function AgentFilterPill(props: {
  testID: string;
  label: string;
  count: number;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        height: "100%",
        paddingHorizontal: 10,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: props.active ? paletteToken("accent_soft_edge") : "transparent",
        backgroundColor: props.active ? paletteToken("card_active") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={props.active ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      activeStyle={{ backgroundColor: paletteToken("card_active") }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: props.active ? "600" : "500",
          color: props.active ? paletteToken("fg") : paletteToken("fg_muted"),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
      <View
        style={{
          minWidth: 18,
          height: 18,
          paddingHorizontal: 5,
          borderRadius: 999,
          backgroundColor: props.active ? paletteToken("accent_soft") : paletteToken("panel"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Text
          style={{
            fontSize: 11,
            fontWeight: props.active ? "600" : "500",
            fontVariant: ["tabular-nums"],
            color: props.active ? paletteToken("accent_fg") : paletteToken("fg_muted"),
          }}
        >
          {String(props.count)}
        </Text>
      </View>
    </View>
  );
}

function AgentEmptyFilters(props: { state: AgentConnectionsState }): JSX.Element {
  const state = props.state;
  return (
    <AgentEmptyState
      icon="search"
      message={t("settings.noAgentsMatchFilter")}
      action={
        <SettingsButton
          testID="agent-clear-filters"
          size="sm"
          variant="accent"
          icon="rotateCcw"
          label={t("settings.clearAgentFilters")}
          onPress={() => state.clearAgentFilters()}
        />
      }
    />
  );
}

function AgentEmptyState(props: {
  icon: IconName;
  message: string;
  action?: JSX.Element;
}): JSX.Element {
  return (
    <View
      style={{
        paddingHorizontal: 24,
        paddingVertical: 40,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
      }}
    >
      <View
        style={{
          width: 44,
          height: 44,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("well"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name={props.icon} size={20} color={paletteToken("fg_muted")} />
      </View>
      <Text style={{ fontSize: 13, color: paletteToken("fg_muted"), textAlign: "center" }}>
        {props.message}
      </Text>
      {props.action ?? null}
    </View>
  );
}

function visibleAgentRows(
  state: AgentConnectionsState,
  matched: AgentProfileDto[],
): {
  rows: AgentProfileDto[];
  hidden: number;
} {
  const filterActive = state.agentQuery().trim() !== "" || state.agentFilter() !== "all";
  const hidden = filterActive ? 0 : Math.max(0, matched.length - VISIBLE_AGENTS);
  const shown =
    filterActive || state.showAllAgents()
      ? matched.length
      : Math.min(VISIBLE_AGENTS, matched.length);
  return { rows: matched.slice(0, shown), hidden };
}

function AgentRows(props: {
  state: AgentConnectionsState;
  matched: () => AgentProfileDto[];
}): JSX.Element {
  const state = props.state;
  const view = computed(() => visibleAgentRows(state, props.matched()));
  const rows = computed(() => view().rows);
  const hidden = computed(() => view().hidden);
  const sharedByDir = computed(() => sharedNamesByDir(state.profiles()));
  const homedir = os.homedir();
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
      <Each of={rows()} key={(profile) => profile.id}>
        {(profile, index) => (
          <AgentRow
            state={state}
            agentId={profile().id}
            profile={profile}
            sharedByDir={sharedByDir}
            homedir={homedir}
            last={index() + 1 === rows().length}
          />
        )}
      </Each>
      <If when={hidden() > 0}>
        <AgentShowMore state={state} hidden={hidden()} />
      </If>
    </View>
  );
}

function AgentShowMore(props: { state: AgentConnectionsState; hidden: number }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="agent-show-more"
      role="button"
      style={{
        width: "100%",
        paddingHorizontal: 18,
        paddingVertical: 11,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      activeStyle={{ backgroundColor: paletteToken("border_soft") }}
      onPress={() => state.toggleShowAllAgents()}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("accent_fg") }}>
        {state.showAllAgents()
          ? t("settings.collapseAgentList")
          : tf("settings.showRemainingAgents", { count: String(props.hidden) })}
      </Text>
      <CollapseChevron open={state.showAllAgents()} color={paletteToken("accent_fg")} />
    </View>
  );
}

function DeployModeBar(props: { state: AgentConnectionsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="default-deploy-mode-bar"
      style={{
        ...card(),
        paddingHorizontal: 18,
        paddingVertical: 14,
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        opacity: state.deployModePending() ? 0.65 : 1,
      }}
    >
      <View style={{ flex: 1, minWidth: 220, display: "flex", flexDirection: "column", gap: 3 }}>
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
          {st("settings.defaultDeployMode")}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {st("settings.defaultDeployModeHint")}
        </Text>
      </View>
      <ChoicePills
        idPrefix="default-deploy-mode"
        options={[
          { id: "symlink", label: st("settings.deployModeSymlink") },
          { id: "copy", label: st("settings.deployModeCopy") },
        ]}
        current={state.defaultDeployMode()}
        onPick={(id) => {
          if (!state.deployModePending()) {
            state.setDefaultDeployMode(id as DeployMode);
          }
        }}
      />
    </View>
  );
}
