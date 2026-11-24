// 工具栏下拉族：触发钮 + 弹层 + 行。三个页面（账号、我的技能、市场）共用
// 同一套尺度，尺寸一律取 chrome/toolbar.tsx 的 BAR_* 常量，不在这里另写像素。
import { Text, View, signal, type JSX } from "nsh";
import { paletteToken } from "../services/themeMode";
import { Icon, type IconName } from "./icons";
import {
  BAR_CHEVRON,
  BAR_FONT,
  BAR_GAP,
  BAR_H,
  BAR_ICON,
  BAR_PAD_X,
  BAR_RADIUS,
  BAR_SLOT_RADIUS,
} from "./toolbar";
import { Tip } from "./tooltip";

/** Dropdown panel width shared by every bar menu. */
export const BAR_MENU_W = 196;
/** Dropdown row height inside a bar menu panel. */
export const BAR_MENU_ROW_H = 32;
/** Leading glyph of a dropdown row (check / branch / dot). */
export const BAR_MENU_GLYPH = 13;

/** Outlined menu trigger in the bar scale: glyph + label + chevron.
 * The active (narrowed/filtered) state tints the whole chip accent-soft, the
 * same signal BarSecondary filters use — so a bar has exactly two active
 * treatments: accent-soft for "a filter is on", card+border for "this is the
 * current mode" (BarSegmentSlot). */
export function BarMenuTrigger(props: {
  testID?: string;
  glyph: IconName;
  label: string | (() => string);
  tip?: string | (() => string);
  active?: boolean | (() => boolean);
  onPress: () => void;
}): JSX.Element {
  const hovered = signal(false);
  const label = () => (typeof props.label === "function" ? props.label() : props.label);
  const active = () =>
    typeof props.active === "function" ? Boolean(props.active()) : Boolean(props.active);
  const tipText = () => (typeof props.tip === "function" ? props.tip() : props.tip);
  return (
    <View
      tooltip={
        hovered() && props.tip !== undefined ? <Tip text={tipText() ?? ""} /> : undefined
      }
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
    >
      <View
        testID={props.testID}
        role="button"
        accessibilityLabel={label()}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: BAR_GAP,
          height: BAR_H,
          paddingHorizontal: BAR_PAD_X,
          borderRadius: BAR_RADIUS,
          borderWidth: 1,
          borderColor: active() ? paletteToken("accent_soft_edge") : paletteToken("border"),
          backgroundColor: active() ? paletteToken("accent_soft") : paletteToken("bg"),
          cursor: "pointer",
          flexShrink: 0,
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={props.onPress}
      >
        <Icon
          name={props.glyph}
          size={BAR_ICON}
          color={active() ? paletteToken("accent") : paletteToken("fg_muted")}
        />
        <Text
          style={{
            fontSize: BAR_FONT,
            fontWeight: "500",
            color: active() ? paletteToken("accent") : paletteToken("fg"),
          }}
        >
          {label()}
        </Text>
        <Icon
          name="chevronDown"
          size={BAR_CHEVRON}
          color={active() ? paletteToken("accent") : paletteToken("fg_muted")}
        />
      </View>
    </View>
  );
}

/** Dropdown panel under a BarMenuTrigger. */
export function BarMenuPanel(props: { testID?: string; children: JSX.Element | JSX.Element[] }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        width: BAR_MENU_W,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0 8px 30px rgba(0,0,0,0.28)",
        display: "flex",
        flexDirection: "column",
        padding: 6,
        gap: 2,
      }}
    >
      {props.children}
    </View>
  );
}

/** One row of a BarMenuPanel: check column, label, optional count. */
export function BarMenuRow(props: {
  testID?: string;
  label: string;
  count?: number;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: BAR_MENU_ROW_H,
        paddingHorizontal: 8,
        gap: BAR_GAP,
        borderRadius: BAR_SLOT_RADIUS,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <View style={{ width: BAR_ICON, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {props.active ? <Icon name="check" size={BAR_MENU_GLYPH} color={paletteToken("accent")} /> : null}
      </View>
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: BAR_FONT,
          fontWeight: props.active ? "600" : "500",
          color: props.active ? paletteToken("accent") : paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
      {props.count !== undefined ? (
        <Text
          style={{
            fontSize: BAR_FONT,
            fontVariant: "tabular-nums",
            color: paletteToken("fg_faint"),
          }}
        >
          {String(props.count)}
        </Text>
      ) : null}
    </View>
  );
}

