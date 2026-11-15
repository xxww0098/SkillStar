import { Text, View, type JSX, type ViewStyle } from "nsh";
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
            {props.titleExtras?.map((extra) => (
              <View>{extra}</View>
            ))}
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
              {props.filters.map((filter) => (
                <View>{filter}</View>
              ))}
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
          {props.actions?.map((action) => (
            <View>{action}</View>
          ))}
        </View>
      </View>
    </View>
  );
}

/** Unified toolbar search field with active border highlight and clear button. */
export function BarSearchInput(props: {
  value: string;
  placeholder: string;
  onChange: (next: string) => void;
  onClear?: () => void;
  inputTestID: string;
  boxTestID?: string;
  clearTestID?: string;
  width?: number;
}): JSX.Element {
  return (
    <View
      testID={props.boxTestID}
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        width: props.width ?? 208,
        height: 32,
        paddingHorizontal: 10,
        borderRadius: 10,
        borderWidth: 1,
        borderColor:
          props.value.length > 0 ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        flexShrink: 0,
      }}
    >
      <Icon
        name="search"
        size={14}
        color={props.value.length > 0 ? paletteToken("accent") : paletteToken("fg_muted")}
      />
      <View style={{ flex: 1, minWidth: 0 }}>
        <Input
          testID={props.inputTestID}
          value={props.value}
          placeholder={props.placeholder}
          onChange={(e: { value: string }) => props.onChange(e.value)}
          style={{
            fontSize: 12,
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
          tooltip={<Tip text={t("common.clear")} />}
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
            if (props.onClear) props.onClear();
            else props.onChange("");
          }}
        >
          <Icon name="x" size={12} color={paletteToken("fg_muted")} />
        </View>
      </If>
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
    <View
      testID={props.testID}
      role="tablist"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 32,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: 2,
        gap: 2,
        flexShrink: 0,
      }}
    >
      <ViewToggleSlot
        testID={props.gridTestID}
        glyph="layoutGrid"
        label={t("toolbar.viewGrid")}
        selected={!props.listMode}
        onPress={() => props.onSelect(false)}
      />
      <ViewToggleSlot
        testID={props.listTestID}
        glyph="list"
        label={t("toolbar.viewList")}
        selected={props.listMode}
        onPress={() => props.onSelect(true)}
      />
    </View>
  );
}

function ViewToggleSlot(props: {
  testID: string;
  glyph: "layoutGrid" | "list";
  label: string;
  selected: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="tab"
      accessibilityLabel={props.label}
      tooltip={<Tip text={props.label} />}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 28,
        height: 26,
        borderRadius: 7,
        cursor: "pointer",
        backgroundColor: props.selected ? paletteToken("card") : "transparent",
        borderWidth: 1,
        borderColor: props.selected ? paletteToken("border") : "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Icon
        name={props.glyph}
        size={14}
        color={props.selected ? paletteToken("accent") : paletteToken("fg_muted")}
      />
    </View>
  );
}

/** `h-8` count chip — matches unified toolbar height & radius. */
export function BarCount(props: { glyph: IconName; label: string }): JSX.Element {
  return (
    <View style={chipStyle()}>
      <Icon name={props.glyph} size={14} color={paletteToken("accent")} />
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** Plain text chip in the toolbar track. */
export function BarTextChip(props: { label: string }): JSX.Element {
  return (
    <View style={chipStyle()}>
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

function chipStyle(): ViewStyle {
  return {
    height: 32,
    paddingHorizontal: 10,
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    flexShrink: 0,
    borderRadius: 10,
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
  return (
    <View tooltip={props.tip ? <Tip text={props.tip} /> : undefined}>
      <View
        testID={props.testID}
        role="button"
        disabled={props.disabled}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          height: 32,
          paddingHorizontal: 12,
          borderRadius: 10,
          opacity: props.disabled ? 0.5 : 1,
          cursor: props.disabled ? "default" : "pointer",
          backgroundColor: paletteToken("accent"),
          flexShrink: 0,
        }}
        hoverStyle={props.disabled ? undefined : { backgroundColor: paletteToken("accent_hover") }}
        onPress={() => {
          if (!props.disabled) props.onPress();
        }}
      >
        {props.glyph ? <Icon name={props.glyph} size={14} color={paletteToken("on_accent")} /> : null}
        <Text style={{ fontSize: 12, color: paletteToken("on_accent"), fontWeight: "500" }}>
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
  return (
    <View tooltip={props.tip ? <Tip text={props.tip} /> : undefined}>
      <View
        testID={props.testID}
        role="button"
        disabled={props.disabled}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          height: 32,
          paddingHorizontal: 10,
          borderRadius: 10,
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
        onPress={() => {
          if (!props.disabled) props.onPress();
        }}
      >
        {props.glyph ? (
          <Icon name={props.glyph} size={14} color={paletteToken("fg_muted")} />
        ) : null}
        <Text style={{ fontSize: 12, color: paletteToken("fg"), fontWeight: "500" }}>
          {props.label}
        </Text>
      </View>
    </View>
  );
}

/** Outlined 32×32 icon command; swaps glyph to spinning indicator while `loading`. */
export function BarIconButton(props: {
  testID?: string;
  glyph: IconName;
  loading?: boolean;
  tip?: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View tooltip={props.tip ? <Tip text={props.tip} /> : undefined}>
      <View
        testID={props.testID}
        role="button"
        accessibilityLabel={props.tip}
        disabled={props.loading}
        desktop={{ windowDrag: false }}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 32,
          height: 32,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("bg"),
          cursor: props.loading ? "default" : "pointer",
          flexShrink: 0,
        }}
        hoverStyle={
          props.loading
            ? undefined
            : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
        }
        onPress={() => {
          if (!props.loading) props.onPress();
        }}
      >
        <IconSpin
          name={props.glyph}
          size={14}
          color={props.loading ? paletteToken("accent") : paletteToken("fg_muted")}
          spin={Boolean(props.loading)}
        />
      </View>
    </View>
  );
}
