import { Text, View, computed, type JSX } from "nsh";
import { Switch } from "nsh/base";
import { AgentIcon } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { settleTransition } from "../../chrome/motion";
import { Tip } from "../../chrome/tooltip";
import { PAGE_PAD } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { McpAgentTarget, McpServerRecord } from "./types";

/** Agent chips the row spends on coverage before it falls back to "+N". */
const COVERAGE_SLOTS = 4;

/**
 * One library row (58px detached 12px-radius card). Identity, status,
 * agent coverage, power — left to right, one line of secondary text.
 */
export function McpServerRow(props: {
  state: McpState;
  server: () => McpServerRecord;
  isFirst: () => boolean;
  isLast: () => boolean;
}): JSX.Element {
  const name = () => props.server().name;
  const selected = computed(() => props.state.selectedName() === name());
  const checked = computed(() => props.state.selectedSet().has(name()));
  const probeRes = computed(() => props.state.probeResults()[name()]);
  const isProbing = computed(() => props.state.isProbing(name()));

  const endpoint = computed(() => {
    const cur = props.server();
    return cur.transport === "stdio"
      ? `${cur.command} ${cur.args.join(" ")}`.trim()
      : cur.url;
  });

  const enabledAgents = computed<McpAgentTarget[]>(() => {
    const byId = props.state.agentById();
    const out: McpAgentTarget[] = [];
    for (const id of props.server().agents) {
      const target = byId.get(id);
      if (target) out.push(target);
    }
    return out;
  });

  const onStatusPress = () => {
    if (!probeRes() && !isProbing()) {
      void props.state.runCheck(name());
    } else {
      props.state.selectServer(name());
    }
  };

  const statusLabel = () => {
    if (isProbing()) return ms().probingShort;
    const res = probeRes();
    if (!res) return "";
    if (!res.ok) return ms().probeFailedLabel;
    return `${res.tools.length} ${ms().toolsUnit}`;
  };

  return (
    <View
      testID={`mcp-card-${name()}`}
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        height: 58,
        marginHorizontal: PAGE_PAD,
        marginBottom: props.isLast() ? 0 : 8,
        paddingHorizontal: 14,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: selected() ? paletteToken("accent") : paletteToken("border"),
        backgroundColor: selected() ? paletteToken("card_active") : paletteToken("card"),
        overflow: "hidden",
        cursor: "pointer",
      }}
      hoverStyle={{
        backgroundColor: selected()
          ? paletteToken("card_active_hover")
          : paletteToken("card_hover"),
        borderColor: selected() ? paletteToken("accent") : paletteToken("edge"),
      }}
      onPress={() => {
        if (props.state.selectionMode()) {
          props.state.toggleSelectServer(name());
        } else {
          props.state.selectServer(name());
        }
      }}
    >
      <If when={props.state.selectionMode()}>
        <View
          testID={`mcp-row-check-${name()}`}
          role="checkbox"
          accessibilityState={{ checked: checked() }}
          accessibilityLabel={name()}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 16,
            height: 16,
            flexShrink: 0,
            cursor: "pointer",
          }}
        >
          <Icon
            name={checked() ? "squareCheck" : "square"}
            size={15}
            color={checked() ? paletteToken("accent") : paletteToken("fg_muted")}
          />
        </View>
      </If>

      <View
        style={{
          width: 32,
          height: 32,
          borderRadius: 9,
          borderWidth: 1,
          borderColor:
            props.server().transport === "stdio"
              ? paletteToken("border_soft")
              : paletteToken("accent_soft_edge"),
          backgroundColor:
            props.server().transport === "stdio" ? paletteToken("well") : paletteToken("accent_soft"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          opacity: props.server().paused ? 0.55 : 1,
        }}
      >
        <Icon
          name={props.server().transport === "stdio" ? "terminal" : "globe"}
          size={14}
          color={
            props.server().transport === "stdio" ? paletteToken("fg_muted") : paletteToken("accent_fg")
          }
        />
      </View>

      <View style={{ display: "flex", flexDirection: "column", flex: 1, minWidth: 0, gap: 2 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minWidth: 0 }}>
          <Text
            style={{
              fontSize: 13,
              fontWeight: "600",
              fontFamily: "monospace",
              color: paletteToken("fg"),
              numberOfLines: 1,
              textOverflow: "ellipsis",
              flexShrink: 1,
              minWidth: 0,
            }}
          >
            {name()}
          </Text>
          <StatusChip
            name={name()}
            label={statusLabel()}
            probing={isProbing()}
            probed={probeRes() !== undefined}
            ok={probeRes()?.ok === true}
            onPress={onStatusPress}
          />
          <If when={props.server().paused === true}>
            <View
              style={{
                paddingHorizontal: 6,
                height: 18,
                borderRadius: 5,
                borderWidth: 1,
                borderColor: paletteToken("warn_border"),
                backgroundColor: paletteToken("warn_bg"),
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
              }}
            >
              <Text style={{ fontSize: 10, fontWeight: "500", color: paletteToken("warn") }}>
                {ms().pausedLabel}
              </Text>
            </View>
          </If>
        </View>
        <Text
          style={{
            fontSize: 11,
            fontFamily: "monospace",
            color: paletteToken("fg_muted"),
            numberOfLines: 1,
            textOverflow: "ellipsis",
          }}
        >
          {endpoint()}
        </Text>
      </View>

      <AgentCoverage
        name={name()}
        agents={enabledAgents}
        total={() => props.server().agents.length}
      />

      <View style={{ flexShrink: 0 }}>
        <Switch
          testID={`mcp-pause-${name()}`}
          checked={!props.server().paused}
          accessibilityLabel={name()}
          trackStyle={{
            width: 34,
            height: 18,
            borderRadius: 9,
            borderWidth: 1,
            backgroundColor: !props.server().paused ? paletteToken("ok") : paletteToken("well"),
            borderColor: !props.server().paused ? paletteToken("ok") : paletteToken("edge"),
          }}
          thumbStyle={{
            width: 12,
            height: 12,
            borderRadius: 6,
            backgroundColor: !props.server().paused ? paletteToken("panel") : paletteToken("fg_muted"),
            transform: !props.server().paused ? "translateX(17px)" : "translateX(2px)",
            ...settleTransition(),
          }}
          onChange={(e: { checked: boolean }) =>
            void props.state.togglePaused(name(), !e.checked)
          }
        />
      </View>
    </View>
  );
}

/**
 * Probe state pill inline behind the name: hollow ring until checked, then
 * status-toned pill with tool count or failure label. Clicking starts a probe.
 */
function StatusChip(props: {
  name: string;
  label: string;
  probing: boolean;
  probed: boolean;
  ok: boolean;
  onPress: () => void;
}): JSX.Element {
  const dotColor = () => {
    if (props.probing) return paletteToken("warn");
    if (!props.probed) return paletteToken("fg_faint");
    return props.ok ? paletteToken("ok") : paletteToken("danger");
  };
  const showPill = () => props.probing || props.probed;
  const pillBg = () => {
    if (props.probing) return paletteToken("warn_bg");
    return props.ok ? paletteToken("ok_bg") : paletteToken("danger_bg");
  };
  const pillBorder = () => {
    if (props.probing) return paletteToken("warn_border");
    return props.ok ? paletteToken("ok_border") : paletteToken("danger_border");
  };
  const textColor = () => {
    if (props.probing) return paletteToken("warn");
    return props.ok ? paletteToken("ok") : paletteToken("danger_fg");
  };

  return (
    <View
      testID={`mcp-status-${props.name}`}
      role="button"
      accessibilityLabel={props.label.length > 0 ? props.label : ms().unprobedLabel}
      tooltip={<Tip text={ms().btnProbe} />}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        height: 18,
        paddingHorizontal: showPill() ? 6 : 4,
        borderRadius: 5,
        borderWidth: showPill() ? 1 : 0,
        borderColor: showPill() ? pillBorder() : "transparent",
        backgroundColor: showPill() ? pillBg() : "transparent",
        cursor: "pointer",
        flexShrink: 0,
      }}
      hoverStyle={{ opacity: 0.85 }}
      onPress={props.onPress}
    >
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 3,
          borderWidth: showPill() ? 0 : 1,
          borderColor: dotColor(),
          backgroundColor: showPill() ? dotColor() : "transparent",
          flexShrink: 0,
        }}
      />
      <If when={props.label.length > 0}>
        <Text
          style={{
            fontSize: 10,
            fontWeight: "500",
            fontVariant: ["tabular-nums"],
            color: textColor(),
            numberOfLines: 1,
          }}
        >
          {props.label}
        </Text>
      </If>
    </View>
  );
}

/** Which agents already receive this server: up to 4 22×22 squircles, "+N", or a dash. */
function AgentCoverage(props: {
  name: string;
  agents: () => McpAgentTarget[];
  /** Enabled targets, including ids this build does not know (they only count). */
  total: () => number;
}): JSX.Element {
  const overflow = () => props.total() - Math.min(props.agents().length, COVERAGE_SLOTS);
  return (
    <If
      when={props.agents().length > 0}
      else={
        <View
          tooltip={<Tip text={ms().rowAgentsNone} />}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            paddingHorizontal: 6,
            flexShrink: 0,
          }}
        >
          <Text
            style={{
              fontSize: 11,
              color: paletteToken("fg_faint"),
            }}
          >
            —
          </Text>
        </View>
      }
    >
      <View
        testID={`mcp-row-agents-${props.name}`}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          flexShrink: 0,
        }}
      >
        <Each of={props.agents().slice(0, COVERAGE_SLOTS)} key={(a: McpAgentTarget) => a.id}>
          {(agent: () => McpAgentTarget) => (
            <View
              testID={`mcp-card-${props.name}-agent-${agent().id}`}
              tooltip={<Tip text={agent().name} />}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                borderWidth: 1,
                borderColor: paletteToken("border_soft"),
                backgroundColor: paletteToken("well"),
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
              }}
            >
              <AgentIcon
                id={agent().id}
                size={15}
                testID={`mcp-row-agent-icon-${props.name}-${agent().id}`}
              />
            </View>
          )}
        </Each>
        <If when={overflow() > 0}>
          <Text
            style={{
              fontSize: 10,
              fontWeight: "600",
              fontFamily: "monospace",
              fontVariant: ["tabular-nums"],
              color: paletteToken("fg_muted"),
            }}
          >
            {`+${overflow()}`}
          </Text>
        </If>
      </View>
    </If>
  );
}
