import { Text, View, signal, type InputRef, type JSX, type ViewStyle } from "nsh";
import { Input } from "nsh/base";
import { PAGE_PAD, TOOLBAR_H } from "../layout";
import { pageBandPadRight } from "../shell/caption";
import { paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { IconSpin } from "./feedback";
import { Icon, type IconName } from "./icons";
import { Tip } from "./tooltip";

// Unified floating rounded page toolbar (56px outer track matching TOOLBAR_H):
// outer gutter aligns flush with PAGE_PAD (20px) so the floating 14px-radius
// card matches the 1040px card-grid track below it, just like AccountsToolbar.
// All dynamic props are read inside JSX expressions for nsh reactivity.

/** The band and card every page toolbar draws, shared with the accounts
 * toolbar so the six pages keep one geometry (TOOLBAR_H band, PAGE_PAD
 * gutters, a 46px card 10px below the panel edge). */
export const PAGE_BAR_BAND_TOP = 10;
export const PAGE_BAR_CARD_H = 46;
export const PAGE_BAR_RADIUS = 14;
export const PAGE_BAR_PAD_X = 12;

/** One control scale for every page toolbar (all six pages, both themes).
 * A control is 32 tall with a 10 radius whether it is a search box, a menu
 * trigger, a chip or an icon button; a segmented track is the same 32 box
 * around 26-tall slots. Labels are 12px, glyphs 14, menu chevrons 11 — so
 * the accounts, skills, groups, marketplace, publisher and projects bars
 * line up glyph for glyph. Pages never hand-roll their own sizes: they
 * compose BarSearchInput / BarMenuTrigger / BarSegmented / BarCount /
 * BarPrimary / BarSecondary / BarIconButton. */
export const BAR_H = 32;
export const BAR_RADIUS = 10;
/** Horizontal padding of a control that carries a label. */
export const BAR_PAD_X = 10;
/** Glyph ↔ label gap inside a control. */
export const BAR_GAP = 6;
/** Leading glyph of a control. */
export const BAR_ICON = 14;
/** Trailing chevron that marks a menu trigger. */
export const BAR_CHEVRON = 11;
/** Clear (×) glyph of the search box. */
export const BAR_CLEAR_ICON = 12;
/** Every bar label paints at this size. */
export const BAR_FONT = 12;
/** Segmented track: 2px padding around 26-tall, 7-radius slots, 2px apart. */
export const BAR_SLOT_PAD = 2;
export const BAR_SLOT_GAP = 2;
export const BAR_SLOT_H = 26;
export const BAR_SLOT_RADIUS = 7;
/** Glyph-only segmented slot width (the view switch). */
export const BAR_SLOT_W = 28;
/** The one search-field width in every page bar. 190 keeps the six bars
 * identical without starving the command zone of the busiest one (my skills:
 * title + scope + agent strip + origin + sort + three actions + view switch). */
export const BAR_SEARCH_W = 190;

export interface PageBarProps {
  title: string;
  /** Controls right next to the title (back button, scope switch). */
  titleExtras?: JSX.Element[];
  search?: JSX.Element;
  filters?: JSX.Element[];
  actions?: JSX.Element[];
  /** Unique per page; keys the window-drag layers. */
  dragId?: string;
}

export function PageBar(props: PageBarProps): JSX.Element {
  return (
    <View
      testID={`${props.dragId ?? "page-toolbar-drag"}-drag`}
      desktop={{ windowDrag: true }}
      style={{
        position: "relative",
        display: "flex",
        height: TOOLBAR_H,
        width: "100%",
        flexShrink: 0,
        paddingLeft: PAGE_PAD,
        // Windows draws its caption buttons over the panel's top right: the
        // card ends beside them (macOS keeps PAGE_PAD).
        paddingRight: pageBandPadRight(),
        paddingTop: PAGE_BAR_BAND_TOP,
        paddingBottom: 0,
      }}
    >
      <View
        style={{
          position: "relative",
          display: "flex",
          flex: 1,
          minWidth: 0,
          height: PAGE_BAR_CARD_H,
          flexDirection: "row",
          alignItems: "center",
          paddingHorizontal: PAGE_BAR_PAD_X,
          gap: 10,
          borderRadius: PAGE_BAR_RADIUS,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
        }}
      >
        {/* Zone 1: title cluster + optional divider. */}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            flexShrink: 0,
            height: 32,
          }}
        >
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
            <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
              {props.title}
            </Text>
            {props.titleExtras}
          </View>
          <If when={Boolean(props.search) || (props.filters?.length ?? 0) > 0}>
            <View
              style={{
                width: 1,
                height: 18,
                marginLeft: 12,
                marginRight: 2,
                backgroundColor: paletteToken("border"),
              }}
            />
          </If>
        </View>

        {/* Zone 2: commands — search, filters, drag slack. */}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            flex: 1,
            minWidth: 0,
            height: "100%",
            alignItems: "center",
            gap: 8,
            overflow: "hidden",
          }}
        >
          {props.search ? <View style={{ display: "flex", flexShrink: 0 }}>{props.search}</View> : null}
          {props.filters && props.filters.length > 0 ? (
            <View
              accessibilityLabel={t("toolbar.filtersGroup")}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 8,
                minWidth: 0,
                width: "auto",
              }}
            >
              {props.filters}
            </View>
          ) : null}
          <View
            testID={`${props.dragId ?? "page-toolbar-drag"}-slack`}
            style={{ flex: 1, minWidth: 8, height: "100%" }}
          />
        </View>

        {/* Zone 3: actions. */}
        <View
          accessibilityLabel={t("toolbar.actionsGroup")}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            flexShrink: 0,
          }}
        >
          {props.actions}
        </View>
      </View>
    </View>
  );
}

/** Unified toolbar search field with active border highlight and clear button.
 * One width (BAR_SEARCH_W) in every page bar — no per-page overrides.
 * `inputRef` hands the text input to a page that focuses it (the market's
 * ⌘F); everything else about the field stays shared. */
export function BarSearchInput(props: {
  value: string;
  placeholder: string;
  onChange: (next: string) => void;
  onClear?: () => void;
  inputTestID: string;
  boxTestID?: string;
  clearTestID?: string;
  inputRef?: (node: InputRef | null) => void;
}): JSX.Element {
  const clearHovered = signal(false);
  return (
    <View
      testID={props.boxTestID}
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: BAR_GAP,
        width: BAR_SEARCH_W,
        height: BAR_H,
        paddingHorizontal: BAR_PAD_X,
        borderRadius: BAR_RADIUS,
        borderWidth: 1,
        borderColor:
          props.value.length > 0 ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        flexShrink: 0,
      }}
    >
      <Icon
        name="search"
        size={BAR_ICON}
        color={props.value.length > 0 ? paletteToken("accent") : paletteToken("fg_muted")}
      />
      <View style={{ flex: 1, minWidth: 0 }}>
        <Input
          testID={props.inputTestID}
          ref={(node: InputRef) => props.inputRef?.(node)}
          value={props.value}
          placeholder={props.placeholder}
          onChange={(e: { value: string }) => props.onChange(e.value)}
          style={{
            fontSize: BAR_FONT,
            color: paletteToken("fg"),
            padding: 0,
            backgroundColor: "transparent",
            borderWidth: 0,
          }}
        />
      </View>
      <If when={props.value.length > 0}>
        <View
          testID={props.clearTestID}
          role="button"
          accessibilityLabel={t("common.clear")}
          tooltip={clearHovered() ? <Tip text={t("common.clear")} /> : undefined}
          onMouseEnter={() => clearHovered.set(true)}
          onMouseLeave={() => clearHovered.set(false)}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 18,
            height: 18,
            borderRadius: 9,
            cursor: "pointer",
            flexShrink: 0,
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => {
            clearHovered.set(false);
            if (props.onClear) props.onClear();
            else props.onChange("");
          }}
        >
          <Icon name="x" size={BAR_CLEAR_ICON} color={paletteToken("fg_muted")} />
        </View>
      </If>
    </View>
  );
}

/** One slot of a bar segmented track: 26 high and 7 radius inside the 32
 * track; BAR_SLOT_W wide for a glyph, label width + BAR_PAD_X for a text slot.
 * `tone` picks the selected treatment — "mode" (this is the current mode:
 * card + border, as on the view switch) or "filter" (a filter is on:
 * accent-soft, as on the agent strip's 全部 tab). */
export function BarSegmentSlot(props: {
  testID: string;
  glyph?: IconName;
  label?: string;
  /** Accessible name / tooltip of a glyph-only slot. */
  accessibilityLabel?: string;
  selected: boolean;
  tone?: "mode" | "filter";
  onPress: () => void;
}): JSX.Element {
  const hovered = signal(false);
  const filter = props.tone === "filter";
  const glyphOnly = props.label === undefined;
  const tip = () => props.accessibilityLabel ?? props.label;
  return (
    <View
      testID={props.testID}
      role="tab"
      accessibilityLabel={tip()}
      tooltip={hovered() && tip() ? <Tip text={tip()!} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: glyphOnly ? 0 : BAR_GAP,
        width: glyphOnly ? BAR_SLOT_W : undefined,
        height: BAR_SLOT_H,
        paddingHorizontal: glyphOnly ? 0 : BAR_PAD_X,
        borderRadius: BAR_SLOT_RADIUS,
        cursor: "pointer",
        flexShrink: 0,
        borderWidth: filter ? 0 : 1,
        borderColor: props.selected ? paletteToken("border") : "transparent",
        backgroundColor: props.selected
          ? filter
            ? paletteToken("accent_soft")
            : paletteToken("card")
          : "transparent",
      }}
      hoverStyle={props.selected ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      focusVisibleStyle={{
        outlineWidth: 2,
        outlineColor: paletteToken("accent"),
        outlineOffset: 1,
      }}
      onPress={props.onPress}
    >
      {props.glyph ? (
        <Icon
          name={props.glyph}
          size={BAR_ICON}
          color={props.selected ? paletteToken("accent") : paletteToken("fg_muted")}
        />
      ) : null}
      {props.label ? (
        <Text
          style={{
            fontSize: BAR_FONT,
            fontWeight: props.selected ? "600" : "500",
            color: props.selected
              ? filter
                ? paletteToken("accent_fg")
                : paletteToken("accent")
              : paletteToken("fg_muted"),
          }}
        >
          {props.label}
        </Text>
      ) : null}
    </View>
  );
}

/** Segmented track in the bar scale; every slot reads the same 26px height. */
export function BarSegmented(props: {
  testID?: string;
  slots: JSX.Element[];
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="tablist"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: BAR_H,
        borderRadius: BAR_RADIUS,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: BAR_SLOT_PAD,
        gap: BAR_SLOT_GAP,
        flexShrink: 0,
      }}
    >
      {props.slots}
    </View>
  );
}

/** Shared grid / list mode switcher matching the toolbar geometry. */
export function BarViewToggle(props: {
  listMode: boolean;
  onSelect: (listMode: boolean) => void;
  gridTestID: string;
  listTestID: string;
  testID?: string;
}): JSX.Element {
  return (
    <BarSegmented
      testID={props.testID}
      slots={[
        <BarSegmentSlot
          testID={props.gridTestID}
          glyph="layoutGrid"
          accessibilityLabel={t("toolbar.viewGrid")}
          selected={!props.listMode}
          onPress={() => props.onSelect(false)}
        />,
        <BarSegmentSlot
          testID={props.listTestID}
          glyph="list"
          accessibilityLabel={t("toolbar.viewList")}
          selected={props.listMode}
          onPress={() => props.onSelect(true)}
        />,
      ]}
    />
  );
}

/** `h-8` count chip — matches unified toolbar height & radius. */
export function BarCount(props: { glyph: IconName; label: string }): JSX.Element {
  return (
    <View style={chipStyle()}>
      <Icon name={props.glyph} size={BAR_ICON} color={paletteToken("accent")} />
      <Text style={{ fontSize: BAR_FONT, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** Plain text chip in the toolbar track. */
export function BarTextChip(props: { label: string }): JSX.Element {
  return (
    <View style={chipStyle()}>
      <Text style={{ fontSize: BAR_FONT, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

function chipStyle(): ViewStyle {
  return {
    height: BAR_H,
    paddingHorizontal: BAR_PAD_X,
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: BAR_GAP,
    flexShrink: 0,
    borderRadius: BAR_RADIUS,
    borderWidth: 1,
    borderColor: paletteToken("border"),
    backgroundColor: paletteToken("bg"),
  };
}

/** Accent-filled toolbar primary command. */
export function BarPrimary(props: {
  testID?: string;
  glyph?: IconName;
  label: string;
  disabled?: boolean;
  onPress: () => void;
  tip?: string;
}): JSX.Element {
  const hovered = signal(false);
  return (
    <View
      tooltip={hovered() && props.tip ? <Tip text={props.tip} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
    >
      <View
        testID={props.testID}
        role="button"
        disabled={props.disabled}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: BAR_GAP,
          height: BAR_H,
          paddingHorizontal: BAR_PAD_X,
          borderRadius: BAR_RADIUS,
          opacity: props.disabled ? 0.5 : 1,
          cursor: props.disabled ? "default" : "pointer",
          backgroundColor: paletteToken("accent"),
          flexShrink: 0,
        }}
        hoverStyle={props.disabled ? undefined : { backgroundColor: paletteToken("accent_hover") }}
        focusVisibleStyle={{
          outlineWidth: 2,
          outlineColor: paletteToken("accent"),
          outlineOffset: 2,
        }}
        onPress={() => {
          if (!props.disabled) props.onPress();
        }}
      >
        {props.glyph ? <Icon name={props.glyph} size={BAR_ICON} color={paletteToken("on_accent")} /> : null}
        <Text style={{ fontSize: BAR_FONT, color: paletteToken("on_accent"), fontWeight: "500" }}>
          {props.label}
        </Text>
      </View>
    </View>
  );
}

/** Outlined secondary toolbar command with optional glyph + label. */
export function BarSecondary(props: {
  testID?: string;
  glyph?: IconName;
  label: string;
  disabled?: boolean;
  onPress: () => void;
  tip?: string;
}): JSX.Element {
  const hovered = signal(false);
  return (
    <View
      tooltip={hovered() && props.tip ? <Tip text={props.tip} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
    >
      <View
        testID={props.testID}
        role="button"
        disabled={props.disabled}
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
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("bg"),
          opacity: props.disabled ? 0.5 : 1,
          cursor: props.disabled ? "default" : "pointer",
          flexShrink: 0,
        }}
        hoverStyle={
          props.disabled
            ? undefined
            : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
        }
        focusVisibleStyle={{
          outlineWidth: 2,
          outlineColor: paletteToken("accent"),
          outlineOffset: 2,
        }}
        onPress={() => {
          if (!props.disabled) props.onPress();
        }}
      >
        {props.glyph ? (
          <Icon name={props.glyph} size={BAR_ICON} color={paletteToken("fg_muted")} />
        ) : null}
        <Text style={{ fontSize: BAR_FONT, color: paletteToken("fg"), fontWeight: "500" }}>
          {props.label}
        </Text>
      </View>
    </View>
  );
}

/** Square icon command in the bar scale (BAR_H × BAR_H): outlined by
 * default, `active` for a toggled-on state (accent-soft, the filter signal),
 * `tone: "primary"` for the one accent-filled action of a bar. Swaps the
 * glyph to a spinning indicator while `loading`.
 *
 * The primary paints its accent block one ring narrower (`BAR_H − 2`) inside
 * the same BAR_H hit box: a saturated fill reads larger than an outlined
 * sibling whose 1px border eats that ring, so the inset gives the two the same
 * visible colour area. Both stay BAR_H × BAR_H targets. */
export function BarIconButton(props: {
  testID?: string;
  glyph: IconName;
  /** Accessible name; defaults to the tooltip. */
  label?: string;
  loading?: boolean;
  disabled?: boolean;
  active?: boolean | (() => boolean);
  tone?: "default" | "primary";
  tip?: string | (() => string);
  onPress: () => void;
}): JSX.Element {
  const hovered = signal(false);
  const active = () =>
    typeof props.active === "function" ? Boolean(props.active()) : Boolean(props.active);
  const primary = props.tone === "primary";
  const inert = () => Boolean(props.disabled) || Boolean(props.loading);
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
        accessibilityLabel={props.label ?? (typeof props.tip === "function" ? props.tip() : props.tip)}
        disabled={inert()}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: BAR_H,
          height: BAR_H,
          opacity: props.disabled && !props.loading ? 0.45 : 1,
          cursor: inert() ? "default" : "pointer",
          flexShrink: 0,
        }}
        focusVisibleStyle={{
          borderRadius: BAR_RADIUS,
          outlineWidth: 2,
          outlineColor: paletteToken("accent"),
          outlineOffset: 1,
        }}
        onPress={() => {
          if (!inert()) props.onPress();
        }}
      >
        <View
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: primary ? BAR_H - 2 : BAR_H,
            height: primary ? BAR_H - 2 : BAR_H,
            borderRadius: primary ? BAR_RADIUS - 1 : BAR_RADIUS,
            borderWidth: primary ? 0 : 1,
            borderColor: primary
              ? "transparent"
              : active()
                ? paletteToken("accent_soft_edge")
                : paletteToken("border"),
            backgroundColor: primary
              ? paletteToken("accent")
              : active()
                ? paletteToken("accent_soft")
                : paletteToken("bg"),
          }}
          hoverStyle={
            inert()
              ? undefined
              : primary
                ? { backgroundColor: paletteToken("accent_hover") }
                : active()
                  ? { backgroundColor: paletteToken("accent_soft") }
                  : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
          }
        >
          <IconSpin
            name={props.glyph}
            size={BAR_ICON}
            color={
              primary
                ? paletteToken("on_accent")
                : props.loading || active()
                  ? paletteToken("accent")
                  : paletteToken("fg_muted")
            }
            spin={Boolean(props.loading)}
          />
        </View>
      </View>
    </View>
  );
}
