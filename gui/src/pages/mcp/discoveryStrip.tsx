import { List, Text, View, computed, type JSX } from "nsh";
import { AgentIcon } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { PAGE_PAD } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { FoundServer } from "./types";

/** Agent names the collapsed strip names before it falls back to "and N more". */
const NAMED_AGENTS = 2;
/** Height budget of the expanded list: 3 × 34px rows + 8px padding, capped at 116px. */
const FOUND_VIEWPORT_H = 116;

/**
 * Unmanaged entries discovered in agent configs. One line by default so the
 * library stays on screen; the list opens on demand inside a fixed viewport
 * instead of pushing the page down.
 */
export function McpDiscoveryStrip(props: { state: McpState }): JSX.Element {
  const items = () => props.state.filteredFound();
  const agentsNamed = computed<string[]>(() => {
    const seen = new Set<string>();
    const names: string[] = [];
    for (const f of items()) {
      if (!seen.has(f.agentName)) {
        seen.add(f.agentName);
        names.push(f.agentName);
      }
    }
    return names;
  });

  const headerSuffix = () => {
    if (items().length === 0) return ms().foundEmptyHint;
    const names = agentsNamed();
    if (names.length === 0) return ms().discoveryLosslessHint;
    const listed = `${ms().foundAgentsPrefix} ${names.slice(0, NAMED_AGENTS).join(" · ")}${
      names.length > NAMED_AGENTS ? `${ms().foundMoreAgents}${names.length - NAMED_AGENTS}` : ""
    }`;
    return `${listed} · ${ms().discoveryLosslessHint}`;
  };

  return (
    <View
      testID="mcp-found-banner"
      style={{
        marginHorizontal: PAGE_PAD,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        overflow: "hidden",
      }}
    >
      <View
        tooltip={<Tip text={ms().unmanagedSubtitle} />}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          height: 38,
          paddingLeft: 12,
          paddingRight: 8,
        }}
      >
        <Icon name="scan" size={14} color={paletteToken("accent")} />
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg"), flexShrink: 0 }}>
          {ms().foundLabel}
        </Text>
        <View
          style={{
            height: 18,
            minWidth: 18,
            paddingHorizontal: 5,
            borderRadius: 5,
            backgroundColor: paletteToken("accent_soft"),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <Text
            style={{
              fontSize: 10,
              fontWeight: "600",
              fontFamily: "monospace",
              fontVariant: ["tabular-nums"],
              color: paletteToken("accent_fg"),
            }}
          >
            {`${items().length}`}
          </Text>
        </View>
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 11,
            color: paletteToken("fg_muted"),
            numberOfLines: 1,
            textOverflow: "ellipsis",
          }}
        >
          {headerSuffix()}
        </Text>
        <View
          testID="mcp-found-toggle"
          role="button"
          accessibilityState={{ expanded: props.state.foundOpen() }}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 4,
            height: 24,
            paddingHorizontal: 8,
            borderRadius: 7,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("bg"),
            cursor: "pointer",
            flexShrink: 0,
          }}
          hoverStyle={{
            backgroundColor: paletteToken("panel_hover"),
            borderColor: paletteToken("edge"),
          }}
          onPress={() => props.state.setFoundOpen(!props.state.foundOpen())}
        >
          <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg") }}>
            {props.state.foundOpen() ? ms().foundCollapse : ms().foundExpand}
          </Text>
          <Icon
            name={props.state.foundOpen() ? "chevronUp" : "chevronDown"}
            size={11}
            color={paletteToken("fg_muted")}
          />
        </View>
      </View>

      <If when={props.state.foundOpen()}>
        <If
          when={items().length > 0}
          else={
            <View
              style={{
                borderTopWidth: 1,
                borderTopColor: paletteToken("border_soft"),
                paddingHorizontal: 12,
                paddingVertical: 8,
              }}
            >
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                {ms().foundEmptyHint}
              </Text>
            </View>
          }
        >
          <View
            style={{
              height: Math.min(items().length * 34 + 8, FOUND_VIEWPORT_H),
              maxHeight: FOUND_VIEWPORT_H,
              borderTopWidth: 1,
              borderTopColor: paletteToken("border_soft"),
            }}
          >
            <List
              each={items()}
              keyExtractor={(f: FoundServer) => `${f.agentId}::${f.server.name}`}
              estimatedItemSize={34}
              scrollEventThrottle={32}
              style={{ width: "100%", height: "100%" }}
              contentStyle={{ padding: 4 }}
            >
              {(f: () => FoundServer) => <FoundRow state={props.state} found={f} />}
            </List>
          </View>
        </If>
      </If>
    </View>
  );
}

function FoundRow(props: { state: McpState; found: () => FoundServer }): JSX.Element {
  return (
    <View
      testID={`mcp-found-${props.found().agentId}-${props.found().server.name}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        height: 34,
        paddingLeft: 8,
        paddingRight: 4,
        borderRadius: 7,
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
    >
      <AgentIcon id={props.found().agentId} size={14} />
      <Text
        style={{
          fontSize: 12,
          fontWeight: "600",
          fontFamily: "monospace",
          color: paletteToken("fg"),
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.found().server.name}
      </Text>
      <Text
        style={{
          fontSize: 11,
          color: paletteToken("fg_muted"),
          flexShrink: 0,
          numberOfLines: 1,
        }}
      >
        {`${props.found().agentName} · ${props.found().server.transport.toUpperCase()}`}
      </Text>
      <View style={{ flex: 1, minWidth: 0 }} />
      <SmallAction
        testID={`mcp-adopt-${props.found().agentId}-${props.found().server.name}`}
        label={ms().adoptBtn}
        primary
        onPress={() =>
          void props.state.importFound(props.found().agentId, props.found().server.name)
        }
      />
      <SmallAction
        testID={`mcp-drop-found-${props.found().agentId}-${props.found().server.name}`}
        label={ms().dropFoundBtn}
        onPress={() =>
          void props.state.removeFound(props.found().agentId, props.found().server.name)
        }
      />
    </View>
  );
}

function SmallAction(props: {
  testID: string;
  label: string;
  primary?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        height: 24,
        paddingHorizontal: 9,
        borderRadius: 7,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: props.primary ? paletteToken("accent") : paletteToken("border"),
        backgroundColor: props.primary ? paletteToken("accent") : "transparent",
      }}
      hoverStyle={{
        backgroundColor: props.primary
          ? paletteToken("accent_hover")
          : paletteToken("panel_hover"),
        borderColor: props.primary ? paletteToken("accent_hover") : paletteToken("edge"),
      }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: props.primary ? "500" : "400",
          color: props.primary ? paletteToken("on_accent") : paletteToken("fg_muted"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
