import { Scroll, Text, View, effect, signal, type JSX, type ScrollRef } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { requestThemeMode } from "../../shell/events";
import { realSettingsBridge } from "./bridge";
import { activeSectionFromScroll, createSettingsState, type SettingsState } from "./state";
import {
  SETTINGS_BODY_W,
  SETTINGS_NAV_COLLAPSED_W,
  SETTINGS_NAV_LABELS_MIN_PANE,
  SETTINGS_NAV_MIN_PANE,
  SETTINGS_NAV_W,
  SETTINGS_SECTION_PB,
  SETTINGS_SECTIONS,
} from "./sections";
import { GeneralSection, NetworkSection } from "./general";
import { TranslationSection } from "./sections/translation";
import { AgentConnectionsSection } from "./sections/agentConnections";
import { StorageSection } from "./sections/storage";
import { AboutSection } from "./sections/about";

// 设置页：无顶栏（仅保留 20px 透明窗口拖拽条），左侧次级导航顶部对齐并与主侧栏及右侧内容区
// 保持明确对称留白（左 28px + 壳层 8px / 右 28px）。宽窗口下右侧按 activeSection 切换六大分区
// （display 保活），窄窗口（< SETTINGS_NAV_MIN_PANE）自动收起次级导航并退化为单列纵向堆叠。

const paneWidth = signal(0);
let scroller: ScrollRef | null = null;
const pageState = createSettingsState(realSettingsBridge);

export function SettingsPage(): JSX.Element {
  return <SettingsWorkbench state={pageState} />;
}

export function SettingsWorkbench(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  effect(() => state.start());
  effect(() =>
    requestThemeMode.subscribe((style) => {
      state.setBackgroundStyle(style);
    })
  );
  return (
    <View
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}
      onLayout={(event) => paneWidth.set(event.bounds.size.width)}
    >
      {/* 顶部无边框窗口拖拽区（移除原 PageBar 后保留顶部拖拽句柄与上边距）。 */}
      <View
        testID="settings-window-drag"
        style={{ width: "100%", height: 20, flexShrink: 0 }}
        desktop={{ windowDrag: true }}
      />
      <SettingsRow state={state} />
    </View>
  );
}

function SettingsRow(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "stretch",
        flex: 1,
        minHeight: 0,
        width: "100%",
        paddingLeft: paneWidth() >= SETTINGS_NAV_MIN_PANE ? 28 : 20,
        paddingRight: 20,
        paddingTop: 4,
        gap: paneWidth() >= SETTINGS_NAV_MIN_PANE ? 28 : 0,
      }}
    >
      {paneWidth() >= SETTINGS_NAV_MIN_PANE ? (
        <SettingsNav state={state} labels={paneWidth() >= SETTINGS_NAV_LABELS_MIN_PANE} />
      ) : null}
      <View style={{ position: "relative", flex: 1, minWidth: 0, height: "100%", minHeight: 0 }}>
        <SettingsColumn state={state} />
      </View>
    </View>
  );
}

/** 次级导航卡片（顶部与分区头齐平，左侧活跃胶囊条 + 图标底座，hover 零帧换色）。 */
function SettingsNav(props: { state: SettingsState; labels: boolean }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        alignSelf: "flex-start",
        marginTop: 4,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        gap: 4,
        paddingHorizontal: props.labels ? 8 : 6,
        paddingVertical: 8,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        width: props.labels ? SETTINGS_NAV_W : SETTINGS_NAV_COLLAPSED_W,
      }}
    >
      {SETTINGS_SECTIONS.map((section) => (
        <SettingsNavItem
          domId={section.domId}
          labelKey={section.labelKey}
          icon={section.icon}
          labels={props.labels}
          active={state.activeSection() === section.id}
          onPress={() => focusSection(state, section.id)}
        />
      ))}
    </View>
  );
}

function SettingsNavItem(props: {
  domId: string;
  labelKey: string;
  icon: Parameters<typeof Icon>[0]["name"];
  labels: boolean;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.domId}
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: props.labels ? "flex-start" : "center",
        gap: 8,
        height: 38,
        width: "100%",
        borderRadius: 10,
        borderWidth: 1,
        borderColor: props.active ? paletteToken("accent_soft_edge") : "transparent",
        backgroundColor: props.active ? paletteToken("card_active") : "transparent",
        cursor: "pointer",
        flexShrink: 0,
        ...(props.labels ? { paddingLeft: 6, paddingRight: 10 } : {}),
      }}
      hoverStyle={
        props.active
          ? undefined
          : {
              backgroundColor: paletteToken("panel_hover"),
            }
      }
      activeStyle={{
        backgroundColor: paletteToken("panel_active"),
      }}
      onPress={props.onPress}
    >
      {props.labels ? (
        <View
          style={{
            width: 3,
            height: 16,
            borderRadius: 2,
            flexShrink: 0,
            backgroundColor: props.active ? paletteToken("accent") : "transparent",
          }}
        />
      ) : null}
      <View
        style={{
          width: props.labels ? 24 : 28,
          height: props.labels ? 24 : 28,
          borderRadius: 7,
          flexShrink: 0,
          backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon
          name={props.icon}
          size={15}
          color={props.active ? paletteToken("accent_fg") : paletteToken("fg_muted")}
        />
      </View>
      {props.labels ? (
        <Text
          style={{
            minWidth: 0,
            flex: 1,
            fontSize: 13,
            fontWeight: props.active ? "600" : "500",
            color: props.active ? paletteToken("fg") : paletteToken("fg_muted"),
            numberOfLines: 1,
          }}
        >
          {t(props.labelKey)}
        </Text>
      ) : null}
    </View>
  );
}

function focusSection(state: SettingsState, id: string): void {
  state.activateSection(id);
  scroller?.scrollTo({ y: 0, animated: false });
}

function SettingsColumn(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  return (
    <Scroll
      style={{ width: "100%", height: "100%" }}
      contentStyle={{
        display: "flex",
        flexDirection: "column",
        paddingTop: 4,
        paddingBottom: 32,
        paddingRight: 8,
      }}
      ref={(node) => {
        scroller = node;
      }}
      onScroll={(event) => {
        if (paneWidth() < SETTINGS_NAV_MIN_PANE) {
          const index = activeSectionFromScroll(
            state.sectionTops(),
            event.offset.y,
            event.contentSize.height,
            event.size.height,
          );
          state.activateSection(SETTINGS_SECTIONS[index].id);
        }
      }}
    >
      {SETTINGS_SECTIONS.map((section, index) => (
        <View
          style={{
            width: "100%",
            maxWidth: SETTINGS_BODY_W,
            flexShrink: 0,
            paddingBottom: paneWidth() < SETTINGS_NAV_MIN_PANE ? SETTINGS_SECTION_PB : 0,
            display:
              paneWidth() < SETTINGS_NAV_MIN_PANE || state.activeSection() === section.id
                ? "flex"
                : "none",
            flexDirection: "column",
          }}
          onLayout={(event) => state.reportSectionTop(index, event.bounds.origin.y)}
        >
          <SectionBody state={state} id={section.id} />
        </View>
      ))}
    </Scroll>
  );
}

function SectionBody(props: { state: SettingsState; id: string }): JSX.Element {
  const state = props.state;
  switch (props.id) {
    case "agents":
      return <AgentConnectionsSection />;
    case "general":
      return <GeneralSection state={state} />;
    case "network":
      return <NetworkSection state={state} />;
    case "translation":
      return <TranslationSection />;
    case "storage":
      return <StorageSection />;
    case "about":
      return <AboutSection />;
    default:
      return <AgentConnectionsSection />;
  }
}
