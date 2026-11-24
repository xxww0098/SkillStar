import { Clipboard, Scroll, Text, View, effect, signal, type JSX } from "nsh";
import { AgentIcon } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { PAGE_PAD } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { toast } from "../../shell/events";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { ProjectSnippet } from "./types";

const RAIL_W = 224;

export function ProjectSnippetsView(props: { state: McpState }): JSX.Element {
  const copiedAgent = signal<string | null>(null);
  let copyResetTimer: ReturnType<typeof setTimeout> | null = null;
  const clearCopyTimer = () => {
    if (copyResetTimer !== null) {
      clearTimeout(copyResetTimer);
      copyResetTimer = null;
    }
  };
  effect(() => {
    void props.state.selectedProjectAgent();
    copiedAgent.set(null);
    clearCopyTimer();
    return clearCopyTimer;
  });

  const activeSnippet = (): ProjectSnippet | undefined =>
    props.state.projectSnippets().find((s) => s.agentId === props.state.selectedProjectAgent()) ??
    props.state.projectSnippets()[0];

  return (
    <View
      testID="mcp-project-view"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "stretch",
        flex: 1,
        minHeight: 0,
        width: "100%",
        overflow: "hidden",
      }}
    >
      <If
        when={props.state.projectSnippets().length > 0}
        else={<ProjectEmptyState />}
      >
        <>
          <ProjectAgentRail state={props.state} />
          <ProjectCodePane
            snippet={activeSnippet}
            copied={() => {
              const cur = activeSnippet();
              return cur !== undefined && copiedAgent() === cur.agentId;
            }}
            onCopy={() => {
              const snip = activeSnippet();
              if (!snip) return;
              clearCopyTimer();
              copiedAgent.set(snip.agentId);
              copyResetTimer = setTimeout(() => {
                copyResetTimer = null;
                if (copiedAgent() === snip.agentId) copiedAgent.set(null);
              }, 1600);
              void Clipboard.writeText(snip.content).then(() =>
                toast(ms().projectCopied, "success"),
              );
            }}
          />
        </>
      </If>
    </View>
  );
}

function ProjectAgentRail(props: { state: McpState }): JSX.Element {
  return (
    <View
      style={{
        width: RAIL_W,
        minWidth: RAIL_W,
        flexShrink: 0,
        height: "100%",
        minHeight: 0,
        display: "flex",
        flexDirection: "column",
        borderRightWidth: 1,
        borderRightColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
      }}
    >
      <View
        style={{
          paddingHorizontal: 14,
          paddingTop: 14,
          paddingBottom: 6,
          flexShrink: 0,
        }}
      >
        <Text
          style={{
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {ms().projectSelectAgent}
        </Text>
      </View>

      <Scroll
        style={{ flex: 1, minHeight: 0 }}
        contentStyle={{
          display: "flex",
          flexDirection: "column",
          padding: 8,
          gap: 4,
        }}
      >
        <Each of={props.state.projectSnippets()} key={(s: ProjectSnippet) => s.agentId}>
          {(snip: () => ProjectSnippet) => (
            <ProjectAgentRow
              snippet={snip}
              selected={() => props.state.selectedProjectAgent() === snip().agentId}
              onSelect={() => props.state.selectedProjectAgent.set(snip().agentId)}
            />
          )}
        </Each>
      </Scroll>
    </View>
  );
}

function ProjectAgentRow(props: {
  snippet: () => ProjectSnippet;
  selected: () => boolean;
  onSelect: () => void;
}): JSX.Element {
  return (
    <View
      testID={`mcp-project-agent-${props.snippet().agentId}`}
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        paddingHorizontal: 10,
        paddingVertical: 8,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: props.selected() ? paletteToken("accent_soft_edge") : "transparent",
        backgroundColor: props.selected() ? paletteToken("accent_soft") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={
        props.selected()
          ? undefined
          : {
              backgroundColor: paletteToken("panel_hover"),
              borderColor: paletteToken("border_soft"),
            }
      }
      onPress={props.onSelect}
    >
      <AgentIcon id={props.snippet().agentId} size={18} />
      <View style={{ display: "flex", flexDirection: "column", flex: 1, minWidth: 0, gap: 1 }}>
        <Text
          style={{
            fontSize: 12,
            fontWeight: props.selected() ? "600" : "500",
            color: props.selected() ? paletteToken("accent_fg") : paletteToken("fg"),
            numberOfLines: 1,
            textOverflow: "ellipsis",
          }}
        >
          {props.snippet().agentName}
        </Text>
        <Text
          style={{
            fontSize: 11,
            fontFamily: "monospace",
            color: paletteToken("fg_muted"),
            numberOfLines: 1,
            textOverflow: "ellipsis",
          }}
        >
          {props.snippet().relPath}
        </Text>
      </View>
    </View>
  );
}

function ProjectCodePane(props: {
  snippet: () => ProjectSnippet | undefined;
  copied: () => boolean;
  onCopy: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        flex: 1,
        minWidth: 0,
        minHeight: 0,
        display: "flex",
        flexDirection: "column",
        padding: PAGE_PAD,
        gap: 12,
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 2, flexShrink: 0 }}>
        <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
          {ms().projectTitle}
        </Text>
        <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
          {ms().projectSubtitle}
        </Text>
      </View>

      <View
        style={{
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
          borderRadius: 14,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
          overflow: "hidden",
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 10,
            paddingHorizontal: 14,
            paddingVertical: 10,
            borderBottomWidth: 1,
            borderBottomColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            flexShrink: 0,
          }}
        >
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, flex: 1, minWidth: 0 }}>
            <Icon name="fileText" size={14} color={paletteToken("accent")} />
            <Text
              style={{
                fontSize: 12,
                fontWeight: "600",
                fontFamily: "monospace",
                color: paletteToken("fg"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
                flexShrink: 1,
                minWidth: 0,
              }}
            >
              {props.snippet()?.relPath ?? ""}
            </Text>
            <If when={(props.snippet()?.format ?? "").length > 0}>
              <View
                style={{
                  paddingHorizontal: 7,
                  paddingVertical: 2,
                  borderRadius: 6,
                  borderWidth: 1,
                  borderColor: paletteToken("border_soft"),
                  backgroundColor: paletteToken("well"),
                  flexShrink: 0,
                }}
              >
                <Text
                  style={{
                    fontSize: 10,
                    fontWeight: "600",
                    fontFamily: "monospace",
                    color: paletteToken("fg_muted"),
                  }}
                >
                  {(props.snippet()?.format ?? "").toUpperCase()}
                </Text>
              </View>
            </If>
          </View>

          <View
            testID="mcp-project-copy"
            role="button"
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 5,
              height: 28,
              paddingHorizontal: 10,
              borderRadius: 8,
              backgroundColor: props.copied() ? paletteToken("ok") : paletteToken("accent"),
              cursor: "pointer",
              flexShrink: 0,
            }}
            hoverStyle={{
              backgroundColor: props.copied() ? paletteToken("ok") : paletteToken("accent_hover"),
            }}
            onPress={props.onCopy}
          >
            <Icon
              name={props.copied() ? "check" : "copy"}
              size={12}
              color={paletteToken("on_accent")}
            />
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
              {props.copied() ? ms().detailCopied : ms().projectCopyBtn}
            </Text>
          </View>
        </View>

        <Scroll
          style={{
            flex: 1,
            minHeight: 0,
            backgroundColor: paletteToken("well"),
          }}
          contentStyle={{ padding: 14 }}
        >
          <Text
            style={{
              fontSize: 12,
              lineHeight: 18,
              fontFamily: "monospace",
              color: paletteToken("fg"),
            }}
          >
            {props.snippet()?.content ?? ""}
          </Text>
        </Scroll>
      </View>
    </View>
  );
}

function ProjectEmptyState(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flex: 1,
        width: "100%",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="fileText" size={22} color={paletteToken("fg_muted")} />
      </View>
      <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
        {ms().projectTitle}
      </Text>
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg_muted"),
          textAlign: "center",
          maxWidth: 380,
        }}
      >
        {ms().projectEmpty}
      </Text>
    </View>
  );
}
