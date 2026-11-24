import { Clipboard, List, Scroll, Text, View, computed, effect, signal, type JSX } from "nsh";
import { Input, Switch } from "nsh/base";
import { AgentIcon } from "../../chrome/agentRail";
import { Alert } from "../../chrome/alerts";
import { openConfirm } from "../../chrome/dialog";
import { IconSpin } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { pressTransition, settleTransition } from "../../chrome/motion";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { toast } from "../../shell/events";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { DiscoveredTool, McpAgentTarget, McpTransport } from "./types";

const EMPTY_SET: ReadonlySet<string> = new Set();

/** Tool rows viewport cap inside the Tools & Health card. */
const TOOL_VIEWPORT_H = 160;
const TOOL_ROW_H = 32;
/**
 * Two-column tile width inside `<Sheet>` (`352` outer - `24` Sheet pad - `2` Sheet border
 * - `24` Scroll pad - `24` Card pad - `2` Card border = `276px`; `(276 - 6) / 2 = 135`).
 */
const AGENT_TILE_W = 135;

const ROW_CENTER = { display: "flex", flexDirection: "row", alignItems: "center" } as const;
const COL = { display: "flex", flexDirection: "column" } as const;
const CENTER = { display: "flex", alignItems: "center", justifyContent: "center" } as const;

const cardSectionStyle = () => ({
  ...COL,
  gap: 8,
  padding: 12,
  borderRadius: 12,
  borderWidth: 1,
  borderColor: paletteToken("border"),
  backgroundColor: paletteToken("card"),
});

/**
 * Detail drawer rendered inside `<Sheet>` (`DETAIL_COLUMN_W = 352`).
 * Sticky header + single `<Scroll>` holding three card sections (Connection,
 * Tools & Health, Synced Agents) + sticky edit/delete footer.
 */
export function McpDetailDrawer(props: { state: McpState }): JSX.Element {
  const srv = () => props.state.selectedServer();
  const copied = signal(false);
  let copyResetTimer: ReturnType<typeof setTimeout> | null = null;

  effect(() => {
    void props.state.selectedName();
    copied.set(false);
    if (copyResetTimer !== null) {
      clearTimeout(copyResetTimer);
      copyResetTimer = null;
    }
    return () => {
      if (copyResetTimer !== null) {
        clearTimeout(copyResetTimer);
        copyResetTimer = null;
      }
    };
  });

  const checkRes = computed(() => {
    const s = srv();
    return s ? props.state.probeResults()[s.name] : undefined;
  });
  const isProbing = computed(() => {
    const s = srv();
    return Boolean(s && props.state.isProbing(s.name));
  });
  const endpoint = computed(() => {
    const s = srv();
    if (!s) return "";
    return s.transport === "stdio" ? `${s.command} ${s.args.join(" ")}`.trim() : s.url;
  });
  const keyChips = computed<string[]>(() => {
    const s = srv();
    if (!s) return [];
    return s.transport === "stdio" ? Object.keys(s.env ?? {}) : Object.keys(s.headers ?? {});
  });
  const tools = computed<DiscoveredTool[]>(() => checkRes()?.tools ?? []);
  const enabledAgentSet = computed<ReadonlySet<string>>(() => {
    const s = srv();
    return s && s.agents.length > 0 ? new Set(s.agents) : EMPTY_SET;
  });
  const disabledToolSet = computed<ReadonlySet<string>>(() => {
    const s = srv();
    return s && s.disabledTools.length > 0 ? new Set(s.disabledTools) : EMPTY_SET;
  });
  const unsupportedToolsAgents = computed<string[]>(() => {
    const s = srv();
    if (!s || s.agents.length === 0) return [];
    const byId = props.state.agentById();
    return s.agents
      .map((id) => byId.get(id))
      .filter((a): a is McpAgentTarget => Boolean(a && !a.supportsToolsOff))
      .map((a) => a.name);
  });

  return (
    <View testID="mcp-detail-drawer" style={{ ...COL, flex: 1, minHeight: 0 }}>
      {/* Sticky Header */}
      <View
        style={{
          ...ROW_CENTER,
          gap: 10,
          paddingHorizontal: 14,
          paddingVertical: 12,
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border"),
          flexShrink: 0,
        }}
      >
        <View
          style={{
            ...CENTER,
            width: 28,
            height: 28,
            borderRadius: 8,
            borderWidth: 1,
            borderColor:
              srv()?.transport === "stdio"
                ? paletteToken("border_soft")
                : paletteToken("accent_soft_edge"),
            backgroundColor:
              srv()?.transport === "stdio" ? paletteToken("well") : paletteToken("accent_soft"),
            flexShrink: 0,
          }}
        >
          <Icon
            name={srv()?.transport === "stdio" ? "terminal" : "globe"}
            size={14}
            color={srv()?.transport === "stdio" ? paletteToken("fg_muted") : paletteToken("accent_fg")}
          />
        </View>
        <View style={{ ...COL, flex: 1, minWidth: 0, gap: 2 }}>
          <Text
            style={{
              fontSize: 14,
              fontWeight: "600",
              fontFamily: "monospace",
              color: paletteToken("fg"),
              numberOfLines: 1,
              textOverflow: "ellipsis",
            }}
          >
            {srv()?.name ?? ""}
          </Text>
          <Text
            style={{
              fontSize: 11,
              fontFamily: "monospace",
              fontVariant: ["tabular-nums"],
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
            }}
          >
            {`${(srv()?.transport ?? "stdio").toUpperCase()} · ${
              (srv()?.agents.length ?? 0) > 0
                ? `${srv()?.agents.length} ${ms().agentEnabledSuffix}`
                : ms().rowAgentsNone
            }`}
          </Text>
        </View>
        <Switch
          testID="mcp-detail-pause-toggle"
          checked={srv()?.paused !== true}
          accessibilityLabel={ms().activeLabel}
          trackStyle={{
            width: 34,
            height: 18,
            borderRadius: 9,
            borderWidth: 1,
            backgroundColor: srv()?.paused !== true ? paletteToken("ok") : paletteToken("well"),
            borderColor: srv()?.paused !== true ? paletteToken("ok") : paletteToken("edge"),
          }}
          thumbStyle={{
            width: 12,
            height: 12,
            borderRadius: 6,
            backgroundColor:
              srv()?.paused !== true ? paletteToken("panel") : paletteToken("fg_muted"),
            transform: srv()?.paused !== true ? "translateX(17px)" : "translateX(2px)",
            ...settleTransition(),
          }}
          onChange={(e: { checked: boolean }) => {
            const s = srv();
            if (s) void props.state.togglePaused(s.name, !e.checked);
          }}
        />
        <View
          testID="mcp-detail-close"
          role="button"
          accessibilityLabel={t("common.close")}
          tooltip={<Tip text={t("common.close")} />}
          style={{ ...CENTER, width: 26, height: 26, borderRadius: 7, cursor: "pointer", flexShrink: 0 }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => props.state.selectServer(null)}
        >
          <Icon name="x" size={14} color={paletteToken("fg_muted")} />
        </View>
      </View>

      {/* Single Scroll Body */}
      <Scroll
        style={{ flex: 1, minHeight: 0, width: "100%" }}
        contentStyle={{ ...COL, padding: 12, gap: 12 }}
      >
        {/* 1. Connection Card */}
        <View style={cardSectionStyle()}>
          <View style={{ ...ROW_CENTER, gap: 6 }}>
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {srv()?.transport === "stdio" ? ms().detailCommand : ms().detailUrl}
            </Text>
            <View style={{ flex: 1, minWidth: 0 }} />
            <View
              testID="mcp-detail-copy"
              role="button"
              accessibilityLabel={ms().copyCommand}
              tooltip={<Tip text={ms().copyCommand} />}
              style={{
                ...ROW_CENTER,
                height: 22,
                paddingHorizontal: 7,
                borderRadius: 6,
                borderWidth: 1,
                borderColor: copied() ? paletteToken("ok_border") : paletteToken("border"),
                backgroundColor: copied() ? paletteToken("ok_bg") : paletteToken("well"),
                gap: 4,
                cursor: "pointer",
              }}
              hoverStyle={{
                backgroundColor: copied() ? paletteToken("ok_bg") : paletteToken("panel_hover"),
                borderColor: copied() ? paletteToken("ok_border") : paletteToken("edge"),
              }}
              onPress={() => {
                const text = endpoint();
                if (text.length === 0) return;
                void Clipboard.writeText(text).then(() => {
                  copied.set(true);
                  if (copyResetTimer !== null) clearTimeout(copyResetTimer);
                  copyResetTimer = setTimeout(() => {
                    copyResetTimer = null;
                    copied.set(false);
                  }, 1600);
                  toast(ms().copiedToast, "success");
                });
              }}
            >
              <Icon
                name={copied() ? "check" : "copy"}
                size={11}
                color={copied() ? paletteToken("ok") : paletteToken("fg_muted")}
              />
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "500",
                  color: copied() ? paletteToken("ok") : paletteToken("fg_muted"),
                }}
              >
                {copied() ? ms().detailCopied : ms().detailCopyCode}
              </Text>
            </View>
          </View>

          <View
            style={{
              paddingHorizontal: 10,
              paddingVertical: 8,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <Text
              style={{
                fontSize: 11,
                lineHeight: 16,
                fontFamily: "monospace",
                color: paletteToken("fg"),
                numberOfLines: 3,
                textOverflow: "ellipsis",
              }}
            >
              {endpoint()}
            </Text>
          </View>

          <If when={keyChips().length > 0 || (srv()?.startupTimeoutSec ?? 0) > 0}>
            <View style={{ ...ROW_CENTER, flexWrap: "wrap", gap: 5 }}>
              <If when={keyChips().length > 0}>
                <Text style={{ fontSize: 10, fontWeight: "500", color: paletteToken("fg_muted") }}>
                  {srv()?.transport === "stdio" ? ms().detailEnvCount : ms().detailHeadersCount}
                </Text>
              </If>
              <Each of={keyChips()} key={(k: string) => k}>
                {(keyName: () => string) => <KeyChip label={keyName()} />}
              </Each>
              <If when={(srv()?.startupTimeoutSec ?? 0) > 0}>
                <KeyChip label={`timeout ${srv()?.startupTimeoutSec}s`} />
              </If>
            </View>
          </If>

          <If when={srv()?.paused === true}>
            <Alert tone="warning" message={ms().detailPausedNotice} />
          </If>
        </View>

        {/* 2. Tools & Health Card */}
        <View style={cardSectionStyle()}>
          <View style={{ ...ROW_CENTER, gap: 6 }}>
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {ms().toolsSectionTitle}
            </Text>
            <View
              style={{
                ...CENTER,
                height: 18,
                paddingHorizontal: 6,
                borderRadius: 999,
                borderWidth: 1,
                borderColor:
                  checkRes()?.ok === true
                    ? paletteToken("ok_border")
                    : checkRes()?.ok === false
                      ? paletteToken("danger_border")
                      : paletteToken("border_soft"),
                backgroundColor:
                  checkRes()?.ok === true
                    ? paletteToken("ok_bg")
                    : checkRes()?.ok === false
                      ? paletteToken("danger_bg")
                      : paletteToken("well"),
                flexShrink: 0,
              }}
            >
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "600",
                  fontVariant: ["tabular-nums"],
                  color:
                    checkRes()?.ok === true
                      ? paletteToken("ok")
                      : checkRes()?.ok === false
                        ? paletteToken("danger_fg")
                        : paletteToken("fg_muted"),
                }}
              >
                {checkRes()?.ok === true
                  ? `${checkRes()?.tools.length ?? 0} ${ms().toolsUnit}`
                  : checkRes()?.ok === false
                    ? ms().probeFailedLabel
                    : isProbing()
                      ? ms().probingShort
                      : ms().unprobedLabel}
              </Text>
            </View>
            <View style={{ flex: 1, minWidth: 0 }} />
            <View
              testID="mcp-btn-probe"
              role="button"
              accessibilityLabel={ms().btnProbe}
              style={{
                ...ROW_CENTER,
                height: 26,
                paddingHorizontal: 9,
                borderRadius: 7,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("well"),
                gap: 5,
                cursor: isProbing() ? "default" : "pointer",
                opacity: isProbing() ? 0.65 : 1,
                flexShrink: 0,
              }}
              hoverStyle={
                isProbing()
                  ? undefined
                  : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
              }
              onPress={() => {
                const s = srv();
                if (s && !isProbing()) void props.state.runCheck(s.name);
              }}
            >
              <IconSpin name="refresh" size={11} color={paletteToken("fg_muted")} spin={isProbing()} />
              <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg") }}>
                {isProbing() ? ms().probing : ms().btnProbe}
              </Text>
            </View>
          </View>

          <If when={checkRes() === undefined}>
            <Text style={{ fontSize: 11, lineHeight: 16, color: paletteToken("fg_muted") }}>
              {ms().detailUnprobedHint}
            </Text>
          </If>

          <If
            when={
              unsupportedToolsAgents().length > 0 &&
              ((srv()?.disabledTools.length ?? 0) > 0 || checkRes()?.ok === true)
            }
          >
            <View testID="mcp-detail-tools-unsupported">
              <Alert
                tone="warning"
                message={`${ms().detailToolsUnsupportedHint}${unsupportedToolsAgents().join(", ")}`}
              />
            </View>
          </If>

          <If when={checkRes() !== undefined && checkRes()?.ok === false}>
            <Alert tone="error" message={checkRes()?.error ?? ms().probeFailedLabel} />
          </If>

          <If when={checkRes()?.ok === true}>
            <View style={{ ...COL, gap: 6 }}>
              <Text
                style={{
                  fontSize: 10,
                  fontFamily: "monospace",
                  fontVariant: ["tabular-nums"],
                  color: paletteToken("fg_muted"),
                }}
              >
                {`${checkRes()?.serverName || "MCP"} ${checkRes()?.serverVersion || ""} · ${
                  checkRes()?.durationMs ?? 0
                }ms`}
              </Text>
              <If when={tools().length === 0}>
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                  {ms().detailToolsEmpty}
                </Text>
              </If>
              <If when={tools().length > 0}>
                <View
                  style={{
                    height: Math.min(tools().length * TOOL_ROW_H + 6, TOOL_VIEWPORT_H),
                    borderRadius: 8,
                    borderWidth: 1,
                    borderColor: paletteToken("border_soft"),
                    backgroundColor: paletteToken("well"),
                  }}
                >
                  <List
                    each={tools()}
                    keyExtractor={(tool: DiscoveredTool) => tool.name}
                    estimatedItemSize={TOOL_ROW_H}
                    scrollEventThrottle={32}
                    style={{ width: "100%", height: "100%" }}
                    contentStyle={{ padding: 3 }}
                  >
                    {(tool: () => DiscoveredTool) => (
                      <ToolDisableRow state={props.state} tool={tool} disabledSet={disabledToolSet} />
                    )}
                  </List>
                </View>
              </If>
            </View>
          </If>
        </View>

        {/* 3. Synced Agents Card */}
        <View style={cardSectionStyle()}>
          <View style={{ ...ROW_CENTER, gap: 6 }}>
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {ms().agentBlockTitle}
            </Text>
            <Text
              style={{
                fontSize: 11,
                fontFamily: "monospace",
                fontVariant: ["tabular-nums"],
                color: paletteToken("fg_muted"),
              }}
            >
              {`${srv()?.agents.length ?? 0} ${ms().agentEnabledSuffix}`}
            </Text>
            <View style={{ flex: 1, minWidth: 0 }} />
            <TinyLink
              testID="mcp-agent-all-on"
              label={ms().agentAllOn}
              onPress={() => void props.state.setAllAgents(props.state.filteredAgents(), true)}
            />
            <TinyLink
              testID="mcp-agent-all-off"
              label={ms().agentAllOff}
              onPress={() => void props.state.setAllAgents(props.state.filteredAgents(), false)}
            />
          </View>

          <View
            style={{
              ...ROW_CENTER,
              alignSelf: "flex-start",
              gap: 2,
              padding: 2,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <ScopeChip
              testID="mcp-agent-scope-installed"
              label={ms().agentScopeInstalled}
              active={props.state.installedOnlyAgents()}
              onPress={() => props.state.installedOnlyAgents.set(true)}
            />
            <ScopeChip
              testID="mcp-agent-scope-all"
              label={ms().agentScopeAll}
              active={!props.state.installedOnlyAgents()}
              onPress={() => props.state.installedOnlyAgents.set(false)}
            />
          </View>

          <If when={!props.state.installedOnlyAgents()}>
            <View
              style={{
                ...ROW_CENTER,
                gap: 6,
                height: 28,
                paddingHorizontal: 8,
                borderRadius: 7,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("well"),
              }}
            >
              <Icon name="search" size={12} color={paletteToken("fg_muted")} />
              <View style={{ flex: 1, minWidth: 0 }}>
                <Input
                  testID="mcp-agent-search"
                  value={props.state.agentQuery()}
                  placeholder={ms().agentSearch}
                  onChange={(e: { value: string }) => props.state.agentQuery.set(e.value)}
                  style={{
                    fontSize: 11,
                    color: paletteToken("fg"),
                    padding: 0,
                    backgroundColor: "transparent",
                    borderWidth: 0,
                  }}
                />
              </View>
            </View>
          </If>

          <If
            when={props.state.filteredAgents().length > 0}
            else={
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted"), paddingVertical: 4 }}>
                {ms().agentNoMatch}
              </Text>
            }
          >
            <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
              <Each of={props.state.filteredAgents()} key={(a: McpAgentTarget) => a.id}>
                {(agent: () => McpAgentTarget) => (
                  <AgentTile
                    state={props.state}
                    agent={agent}
                    transport={() => srv()?.transport ?? "stdio"}
                    enabledSet={enabledAgentSet}
                  />
                )}
              </Each>
            </View>
          </If>
        </View>
      </Scroll>

      {/* Sticky Footer */}
      <View
        style={{
          ...ROW_CENTER,
          gap: 8,
          paddingHorizontal: 12,
          paddingVertical: 10,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border"),
          flexShrink: 0,
        }}
      >
        <FooterButton
          testID="mcp-detail-edit"
          label={ms().detailEdit}
          icon="pencil"
          onPress={() => {
            const s = srv();
            if (s) props.state.openEditModal(s);
          }}
        />
        <FooterButton
          testID="mcp-detail-delete"
          label={t("common.delete")}
          icon="trash"
          danger
          onPress={() => {
            const s = srv();
            if (!s) return;
            const targetName = s.name;
            openConfirm({
              title: `${ms().deleteConfirmTitle}: ${targetName}`,
              body: ms().deleteConfirmBody,
              okText: ms().deleteConfirmOk,
              danger: true,
              onOk: () => {
                void props.state.removeServer(targetName);
              },
            });
          }}
        />
      </View>
    </View>
  );
}

function KeyChip(props: { label: string }): JSX.Element {
  return (
    <View
      style={{
        ...CENTER,
        height: 20,
        paddingHorizontal: 6,
        borderRadius: 5,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
      }}
    >
      <Text
        style={{
          fontSize: 10,
          fontFamily: "monospace",
          fontVariant: ["tabular-nums"],
          color: paletteToken("fg_muted"),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function TinyLink(props: { testID: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{ ...CENTER, height: 20, paddingHorizontal: 6, borderRadius: 5, cursor: "pointer" }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {props.label}
      </Text>
    </View>
  );
}

function ScopeChip(props: {
  testID: string;
  label: string;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="tab"
      accessibilityState={{ selected: props.active }}
      style={{
        ...CENTER,
        height: 22,
        paddingHorizontal: 9,
        borderRadius: 6,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: props.active ? paletteToken("border") : "transparent",
        backgroundColor: props.active ? paletteToken("card") : "transparent",
      }}
      hoverStyle={props.active ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: props.active ? "600" : "500",
          color: props.active ? paletteToken("fg") : paletteToken("fg_muted"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function AgentTile(props: {
  state: McpState;
  agent: () => McpAgentTarget;
  transport: () => McpTransport;
  enabledSet: () => ReadonlySet<string>;
}): JSX.Element {
  const on = computed(() => props.enabledSet().has(props.agent().id));
  const hasConflict = computed(() => {
    const tr = props.transport();
    const a = props.agent();
    return (tr === "sse" && (a.noRemote || a.noSse)) || (tr === "http" && a.noRemote);
  });
  const label = () =>
    props.agent().installed
      ? props.agent().name
      : `${props.agent().name} · ${ms().agentAbsentTag}`;
  const tipText = computed<string | undefined>(() => {
    const a = props.agent();
    if (hasConflict()) {
      return a.notes ? `${ms().detailAgentConflict} · ${a.notes}` : ms().detailAgentConflict;
    }
    if (!a.installed) {
      return a.notes
        ? `${a.name} · ${ms().agentAbsentTag} · ${a.notes}`
        : `${a.name} · ${ms().agentAbsentTag}`;
    }
    return a.notes ? `${a.name} · ${a.notes}` : undefined;
  });

  return (
    <View
      testID={`mcp-drawer-agent-${props.agent().id}`}
      role="checkbox"
      accessibilityLabel={label()}
      accessibilityState={{ checked: on() }}
      tooltip={tipText() ? <Tip text={tipText()!} /> : undefined}
      style={{
        ...ROW_CENTER,
        width: AGENT_TILE_W,
        height: 32,
        paddingHorizontal: 8,
        gap: 6,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: on() ? paletteToken("accent_soft_edge") : paletteToken("border_soft"),
        backgroundColor: on() ? paletteToken("accent_soft") : paletteToken("well"),
        cursor: "pointer",
        flexShrink: 0,
      }}
      hoverStyle={{
        backgroundColor: on() ? paletteToken("accent_soft") : paletteToken("panel_hover"),
        borderColor: on() ? paletteToken("accent") : paletteToken("edge"),
      }}
      onPress={() => {
        const s = props.state.selectedServer();
        if (s) void props.state.toggleAgent(s.name, props.agent().id, !on());
      }}
    >
      <AgentIcon id={props.agent().id} size={14} grayscale={!on()} opacity={on() ? 1 : 0.42} />
      <Text
        style={{
          fontSize: 11,
          fontWeight: on() ? "500" : "400",
          color: on() ? paletteToken("fg") : paletteToken("fg_muted"),
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.agent().name}
      </Text>
      <View style={{ flex: 1, minWidth: 0 }} />
      <If when={hasConflict()}>
        <View
          style={{
            width: 6,
            height: 6,
            borderRadius: 3,
            backgroundColor: paletteToken("warn"),
            flexShrink: 0,
          }}
        />
      </If>
      <If when={on()}>
        <Icon name="check" size={11} color={paletteToken("accent_fg")} />
      </If>
    </View>
  );
}

function ToolDisableRow(props: {
  state: McpState;
  tool: () => DiscoveredTool;
  disabledSet: () => ReadonlySet<string>;
}): JSX.Element {
  const enabled = computed(() => !props.disabledSet().has(props.tool().name));
  return (
    <View
      testID={`mcp-tool-row-${props.tool().name}`}
      role="checkbox"
      accessibilityLabel={props.tool().name}
      accessibilityState={{ checked: enabled() }}
      tooltip={
        props.tool().description
          ? <Tip text={`${props.tool().name}: ${props.tool().description}`} />
          : undefined
      }
      style={{ ...ROW_CENTER, gap: 6, height: TOOL_ROW_H, paddingHorizontal: 6, borderRadius: 6, cursor: "pointer" }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={() => {
        const s = props.state.selectedServer();
        if (s) void props.state.toggleDisabledTool(s, props.tool().name);
      }}
    >
      <Icon
        name={enabled() ? "squareCheck" : "square"}
        size={13}
        color={enabled() ? paletteToken("accent") : paletteToken("fg_faint")}
      />
      <Text
        style={{
          fontSize: 11,
          fontFamily: "monospace",
          fontWeight: "600",
          color: enabled() ? paletteToken("fg") : paletteToken("fg_faint"),
          flexShrink: 0,
          numberOfLines: 1,
        }}
      >
        {props.tool().name}
      </Text>
      <Text
        style={{
          fontSize: 11,
          color: paletteToken("fg_muted"),
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.tool().description}
      </Text>
    </View>
  );
}

function FooterButton(props: {
  testID: string;
  label: string;
  icon: "pencil" | "trash";
  danger?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        ...ROW_CENTER,
        justifyContent: "center",
        flex: 1,
        height: 32,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: props.danger ? paletteToken("danger_border") : paletteToken("border"),
        backgroundColor: props.danger ? paletteToken("danger_bg") : paletteToken("card"),
        gap: 6,
        cursor: "pointer",
        ...(props.danger ? pressTransition() : {}),
      }}
      hoverStyle={{
        backgroundColor: props.danger ? paletteToken("danger_hover") : paletteToken("card_hover"),
        borderColor: props.danger ? paletteToken("danger") : paletteToken("edge"),
      }}
      activeStyle={props.danger ? { opacity: 0.88 } : undefined}
      onPress={props.onPress}
    >
      <Icon
        name={props.icon}
        size={13}
        color={props.danger ? paletteToken("danger_fg") : paletteToken("fg")}
      />
      <Text
        style={{
          fontSize: 12,
          fontWeight: props.danger ? "600" : "500",
          color: props.danger ? paletteToken("danger_fg") : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
