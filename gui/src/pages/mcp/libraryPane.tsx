import { List, Text, View, signal, type JSX } from "nsh";
import { openConfirm } from "../../chrome/dialog";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { IconSpin, Pulse } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { PAGE_PAD } from "../../layout";
import { isLight, paletteToken } from "../../services/themeMode";
import { McpDiscoveryStrip } from "./discoveryStrip";
import { ms } from "./mcpStrings";
import { McpServerRow } from "./serverRow";
import type { McpState } from "./state";
import type { McpServerRecord, SyncWarning } from "./types";

const SKELETON_ROWS = [0, 1, 2, 3, 4] as const;

/**
 * The managed pane: a fixed frame (notices, library bar, batch bar) over the
 * one scroll region that holds server rows. The page body never scrolls, so
 * the toolbar and the counts stay put while the library moves.
 */
export function McpLibraryPane(props: { state: McpState }): JSX.Element {
  const showSkeleton = bindDelayedFlag(
    () => props.state.loading() && (!props.state.started() || props.state.servers().length === 0),
  );

  return (
    <View style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}>
      <If when={props.state.warnings().length > 0}>
        <WarningStrip state={props.state} />
      </If>

      <If when={props.state.found().length > 0}>
        <View style={{ paddingTop: 10 }}>
          <McpDiscoveryStrip state={props.state} />
        </View>
      </If>

      <LibraryBar state={props.state} />

      <If when={props.state.selectionMode()}>
        <BatchBar state={props.state} />
      </If>

      <View style={{ flex: 1, minHeight: 0 }}>
        <If when={showSkeleton()}>
          <LibrarySkeleton />
        </If>

        <If when={!showSkeleton() && !props.state.loading() && props.state.servers().length === 0}>
          <EmptyManagedState state={props.state} />
        </If>

        <If
          when={
            !showSkeleton() &&
            props.state.servers().length > 0 &&
            props.state.filteredServers().length === 0
          }
        >
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              flex: 1,
              gap: 6,
            }}
          >
            <Icon name="search" size={20} color={paletteToken("fg_faint")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {ms().noMatchTitle}
            </Text>
          </View>
        </If>

        <If when={!showSkeleton() && props.state.filteredServers().length > 0}>
          <List
            testID="mcp-library-list"
            each={props.state.filteredServers()}
            keyExtractor={(item: McpServerRecord) => item.name}
            estimatedItemSize={66}
            scrollEventThrottle={32}
            style={{ width: "100%", height: "100%" }}
            contentStyle={{ paddingTop: 4, paddingBottom: 16 }}
          >
            {(item: () => McpServerRecord, index: () => number) => (
              <McpServerRow
                state={props.state}
                server={item}
                isFirst={() => index() === 0}
                isLast={() => index() === props.state.filteredServers().length - 1}
              />
            )}
          </List>
        </If>
      </View>
    </View>
  );
}

function LibrarySkeleton(): JSX.Element {
  return (
    <View
      testID="mcp-library-skeleton"
      accessibilityState={{ busy: true }}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        paddingHorizontal: PAGE_PAD,
        paddingTop: 4,
        paddingBottom: 16,
        width: "100%",
      }}
    >
      {SKELETON_ROWS.map((index) => (
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 12,
            width: "100%",
            height: 58,
            paddingHorizontal: 14,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            flexShrink: 0,
          }}
        >
          <Pulse
            id={`mcp-skel-${index}-icon`}
            style={{ width: 32, height: 32, borderRadius: 9, flexShrink: 0 }}
          />
          <View style={{ display: "flex", flexDirection: "column", flex: 1, minWidth: 0, gap: 6 }}>
            <Pulse
              id={`mcp-skel-${index}-name`}
              style={{ width: "34%", height: 13, borderRadius: 6 }}
            />
            <Pulse
              id={`mcp-skel-${index}-cmd`}
              secondary
              style={{ width: "62%", height: 11, borderRadius: 5 }}
            />
          </View>
          <Pulse
            id={`mcp-skel-${index}-switch`}
            secondary
            style={{ width: 34, height: 18, borderRadius: 9, flexShrink: 0 }}
          />
        </View>
      ))}
    </View>
  );
}

function WarningStrip(props: { state: McpState }): JSX.Element {
  const expanded = signal(false);
  const surfaceBg = () => (isLight() ? paletteToken("warn_bg") : "rgba(251,191,36,0.08)");
  const surfaceBorder = () =>
    isLight() ? paletteToken("warn_border") : "rgba(251,191,36,0.30)";

  return (
    <View
      testID="mcp-warnings-banner"
      style={{
        marginHorizontal: PAGE_PAD,
        marginTop: 10,
      }}
    >
      <If
        when={props.state.warnings().length > 1}
        else={
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              paddingHorizontal: 12,
              paddingVertical: 8,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: surfaceBorder(),
              backgroundColor: surfaceBg(),
            }}
          >
            <Icon name="alertTriangle" size={14} color={paletteToken("warn")} />
            <Text
              style={{
                flex: 1,
                minWidth: 0,
                fontSize: 12,
                lineHeight: 18,
                color: paletteToken("warn"),
              }}
            >
              {props.state.warnings()[0]
                ? `${ms().warningsTitle} · [${props.state.warnings()[0].agentId}] ${props.state.warnings()[0].message}`
                : ms().warningsTitle}
            </Text>
          </View>
        }
      >
        <View
          style={{
            borderRadius: 10,
            borderWidth: 1,
            borderColor: surfaceBorder(),
            backgroundColor: surfaceBg(),
            overflow: "hidden",
          }}
        >
          <View
            testID="mcp-warnings-toggle"
            role="button"
            accessibilityState={{ expanded: expanded() }}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              height: 34,
              paddingHorizontal: 12,
              cursor: "pointer",
            }}
            hoverStyle={{ opacity: 0.85 }}
            onPress={() => expanded.set(!expanded())}
          >
            <Icon name="alertTriangle" size={14} color={paletteToken("warn")} />
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("warn") }}>
              {ms().warningsTitle}
            </Text>
            <Text
              style={{
                fontSize: 11,
                fontFamily: "monospace",
                fontVariant: ["tabular-nums"],
                color: paletteToken("warn"),
              }}
            >
              {`${props.state.warnings().length}`}
            </Text>
            <View style={{ flex: 1, minWidth: 0 }} />
            <Icon
              name={expanded() ? "chevronUp" : "chevronDown"}
              size={12}
              color={paletteToken("warn")}
            />
          </View>
          <If when={expanded()}>
            <View
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 4,
                paddingHorizontal: 12,
                paddingBottom: 8,
                paddingTop: 2,
                borderTopWidth: 1,
                borderTopColor: surfaceBorder(),
              }}
            >
              <Each of={props.state.warnings()} key="index">
                {(w: () => SyncWarning) => (
                  <Text
                    style={{
                      fontSize: 11,
                      lineHeight: 16,
                      color: paletteToken("fg_muted"),
                      numberOfLines: 2,
                      textOverflow: "ellipsis",
                    }}
                  >
                    {`[${w().agentId}] ${w().message}`}
                  </Text>
                )}
              </Each>
            </View>
          </If>
        </View>
      </If>
    </View>
  );
}

/** Pinned bar between the notices and the rows: counts, order, bulk entries. */
function LibraryBar(props: { state: McpState }): JSX.Element {
  const countText = () => {
    const shown = props.state.filteredServers().length;
    const total = props.state.servers().length;
    return shown === total ? `${total}` : `${shown} / ${total}`;
  };
  const pausedCount = () => props.state.pausedCount();

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 36,
        paddingHorizontal: PAGE_PAD,
        flexShrink: 0,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
        {ms().libraryTitle}
      </Text>
      <Text
        style={{
          fontSize: 11,
          fontFamily: "monospace",
          fontVariant: ["tabular-nums"],
          color: paletteToken("fg_faint"),
        }}
      >
        {countText()}
      </Text>
      <If when={pausedCount() > 0}>
        <View
          style={{
            height: 18,
            paddingHorizontal: 6,
            borderRadius: 5,
            borderWidth: 1,
            borderColor: paletteToken("warn_border"),
            backgroundColor: paletteToken("warn_bg"),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Text
            style={{
              fontSize: 10,
              fontWeight: "500",
              fontVariant: ["tabular-nums"],
              color: paletteToken("warn"),
            }}
          >
            {`${ms().pausedLabel} ${pausedCount()}`}
          </Text>
        </View>
      </If>

      <View style={{ flex: 1, minWidth: 0 }} />

      <BarGhost
        testID="mcp-btn-probe-all"
        glyph="activity"
        spin={props.state.probingCount() > 0}
        label={ms().libraryProbeAll}
        onPress={() => void props.state.probeAllServers()}
      />
      <BarGhost
        testID="mcp-sort-toggle"
        glyph="arrowUpDown"
        label={props.state.sortOrder() === "asc" ? ms().sortAsc : ms().sortDesc}
        onPress={() => props.state.sortOrder.set(props.state.sortOrder() === "asc" ? "desc" : "asc")}
      />
      <BarGhost
        testID="mcp-select-toggle"
        glyph={props.state.selectionMode() ? "check" : "square"}
        label={props.state.selectionMode() ? ms().cancelSelectBtn : ms().selectBtn}
        active={props.state.selectionMode()}
        onPress={() => props.state.toggleSelectMode()}
      />
    </View>
  );
}

function BarGhost(props: {
  testID: string;
  glyph: IconName;
  label: string;
  active?: boolean;
  spin?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityState={{ selected: props.active === true }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 5,
        height: 28,
        paddingHorizontal: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: props.active ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: props.active ? paletteToken("accent_soft") : paletteToken("card"),
        cursor: "pointer",
        flexShrink: 0,
      }}
      hoverStyle={{
        backgroundColor: props.active
          ? paletteToken("accent_soft")
          : paletteToken("panel_hover"),
        borderColor: props.active ? paletteToken("accent_soft_edge") : paletteToken("edge"),
      }}
      onPress={props.onPress}
    >
      <IconSpin
        name={props.glyph}
        size={13}
        color={
          props.active || props.spin ? paletteToken("accent_fg") : paletteToken("fg_muted")
        }
        spin={Boolean(props.spin)}
      />
      <Text
        style={{
          fontSize: 11,
          fontWeight: "500",
          color: props.active ? paletteToken("accent_fg") : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function BatchBar(props: { state: McpState }): JSX.Element {
  const count = () => props.state.selectedNames().length;
  return (
    <View
      testID="mcp-batch-bar"
      style={{
        marginHorizontal: PAGE_PAD,
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        gap: 6,
        minHeight: 40,
        paddingHorizontal: 10,
        paddingVertical: 6,
        marginBottom: 4,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("accent_soft_edge"),
        backgroundColor: paletteToken("accent_soft"),
      }}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          fontVariant: ["tabular-nums"],
          color: paletteToken("accent_fg"),
        }}
      >
        {`${ms().batchSelectedCount} ${count()}`}
      </Text>
      <BatchLink
        testID="mcp-batch-select-all"
        label={ms().batchSelectAll}
        onPress={() => props.state.selectAllFiltered()}
      />
      <BatchLink
        testID="mcp-batch-clear"
        label={ms().batchClear}
        onPress={() => props.state.clearSelection()}
      />
      <View style={{ flex: 1, minWidth: 0 }} />
      <BatchAction
        testID="mcp-batch-enable"
        label={ms().batchEnable}
        disabled={count() === 0}
        onPress={() => void props.state.batchSetPaused(false)}
      />
      <BatchAction
        testID="mcp-batch-pause"
        label={ms().batchPause}
        disabled={count() === 0}
        onPress={() => void props.state.batchSetPaused(true)}
      />
      <BatchAction
        testID="mcp-batch-delete"
        label={ms().batchDelete}
        danger
        disabled={count() === 0}
        onPress={() =>
          openConfirm({
            title: ms().batchDeleteConfirmTitle,
            body: ms().batchDeleteConfirmBody,
            okText: ms().batchDeleteConfirmOk,
            danger: true,
            onOk: () => {
              void props.state.batchRemoveSelected();
            },
          })
        }
      />
    </View>
  );
}

function BatchLink(props: { testID: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        paddingHorizontal: 8,
        height: 28,
        borderRadius: 8,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {props.label}
      </Text>
    </View>
  );
}

function BatchAction(props: {
  testID: string;
  label: string;
  danger?: boolean;
  disabled: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityState={{ disabled: props.disabled }}
      style={{
        height: 28,
        paddingHorizontal: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: props.danger ? paletteToken("danger_border") : paletteToken("border"),
        backgroundColor: props.danger ? paletteToken("danger_bg") : paletteToken("card"),
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        opacity: props.disabled ? 0.45 : 1,
        cursor: props.disabled ? "default" : "pointer",
      }}
      hoverStyle={
        props.disabled
          ? undefined
          : {
              backgroundColor: props.danger
                ? paletteToken("danger_hover")
                : paletteToken("panel_hover"),
              borderColor: props.danger ? paletteToken("danger_border") : paletteToken("edge"),
            }
      }
      onPress={() => {
        if (!props.disabled) props.onPress();
      }}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: "500",
          color: props.danger ? paletteToken("danger_fg") : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function EmptyManagedState(props: { state: McpState }): JSX.Element {
  return (
    <View
      testID="mcp-empty-state"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        flex: 1,
        gap: 10,
        paddingHorizontal: 24,
      }}
    >
      <View
        style={{
          width: 52,
          height: 52,
          borderRadius: 999,
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="server" size={22} color={paletteToken("fg_muted")} />
      </View>
      <Text style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}>
        {ms().emptyManagedTitle}
      </Text>
      <Text
        style={{
          fontSize: 12,
          lineHeight: 18,
          color: paletteToken("fg_muted"),
          textAlign: "center",
          maxWidth: 420,
        }}
      >
        {ms().emptyManagedBody}
      </Text>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          marginTop: 4,
        }}
      >
        <View
          testID="mcp-empty-new"
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            paddingHorizontal: 12,
            height: 32,
            borderRadius: 8,
            backgroundColor: paletteToken("accent"),
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
          onPress={() => props.state.openNewModal()}
        >
          <Icon name="plus" size={14} color={paletteToken("on_accent")} />
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
            {ms().btnNew}
          </Text>
        </View>
        <View
          testID="mcp-empty-paste"
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            paddingHorizontal: 12,
            height: 32,
            borderRadius: 8,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            cursor: "pointer",
          }}
          hoverStyle={{
            backgroundColor: paletteToken("panel_hover"),
            borderColor: paletteToken("edge"),
          }}
          onPress={() => props.state.openPasteModal()}
        >
          <Icon name="clipboard" size={14} color={paletteToken("fg_muted")} />
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
            {ms().btnPaste}
          </Text>
        </View>
      </View>
    </View>
  );
}
