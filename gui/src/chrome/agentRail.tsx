import { Image, Scroll, View, computed, effect, signal, type JSX, type ScrollRef } from "nsh";
import { Settings } from "skillstar-gui/native";
import { isLight, paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { Icon } from "./icons";
import { Tip } from "./tooltip";

// Shared agent brand SVG icons, badges, and footer carousel used across
// my_skills, skill_cards, projects, and settings. Keeps `nsh` fine-grained
// reactivity rules: every dynamic prop and signal read happens inside a JSX
// expression or callback, never cached at the top of a component body.

export interface AgentRailProfile {
  id: string;
  displayName: string;
  icon?: string;
}

export interface AgentRailSlot {
  profile: AgentRailProfile;
  linked: boolean;
  pending: boolean;
  label: string;
}

export const RAIL_SLOT_SIZE = 28;
export const RAIL_SLOT_GAP = 6;
export const RAIL_MAX_VISIBLE = 7;
export const RAIL_VIEWPORT_W =
  RAIL_MAX_VISIBLE * RAIL_SLOT_SIZE + (RAIL_MAX_VISIBLE - 1) * RAIL_SLOT_GAP;
const RAIL_ARROW_W = 16;
const RAIL_STEP_PX = 2 * (RAIL_SLOT_SIZE + RAIL_SLOT_GAP);

const iconUriCache = new Map<string, string>();

/** Resolve the `data:image/svg+xml;base64,…` URI (or custom `data:image/…`
 * override) for an agent id + theme brightness, memoized per process. */
export function resolveAgentIconUri(id: string, light: boolean, customIcon?: string): string {
  const custom = customIcon?.trim();
  if (custom && custom.startsWith("data:image/")) {
    return custom;
  }
  const key = `${id.trim().toLowerCase()}|${light ? "1" : "0"}`;
  const cached = iconUriCache.get(key);
  if (cached !== undefined) return cached;
  const resolved = Settings.agentIconDataUri(id, light);
  iconUriCache.set(key, resolved);
  return resolved;
}

/** Maximum horizontal scroll offset (points) for a 7-slot rail viewport. */
export function railMaxScrollX(count: number): number {
  return Math.max(0, count * (RAIL_SLOT_SIZE + RAIL_SLOT_GAP) - RAIL_SLOT_GAP - RAIL_VIEWPORT_W);
}

/** Crisp brand SVG glyph; reads `isLight()` reactively so mono-ink agents
 * switch between `#e8eef8` (dark) and `#18181b` (paper) automatically. */
export function AgentIcon(props: {
  id: string;
  icon?: string;
  size?: number;
  grayscale?: boolean;
  opacity?: number;
  testID?: string;
}): JSX.Element {
  return (
    <Image
      testID={props.testID}
      src={resolveAgentIconUri(props.id, isLight(), props.icon)}
      fadeDuration={0}
      style={{
        width: props.size ?? 16,
        height: props.size ?? 16,
        flexShrink: 0,
        objectFit: "contain",
        filter: props.grayscale ? "grayscale(100%)" : undefined,
        opacity: props.opacity ?? 1,
      }}
    />
  );
}

/** Squircle brand SVG badge shared by drawers, link menus, and project rows. */
export function AgentBadge(props: {
  id: string;
  name: string;
  icon?: string;
  active?: boolean;
  size?: number;
}): JSX.Element {
  return (
    <View
      accessibilityLabel={props.name}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: props.size ?? 22,
        height: props.size ?? 22,
        flexShrink: 0,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: props.active ? paletteToken("accent_soft_edge") : paletteToken("border_soft"),
        backgroundColor: props.active ? paletteToken("accent_soft") : paletteToken("well"),
      }}
    >
      <AgentIcon
        id={props.id}
        icon={props.icon}
        size={Math.max(12, (props.size ?? 22) - 6)}
        grayscale={props.active === false}
        opacity={props.active === false ? 0.6 : 1}
      />
    </View>
  );
}

/** 38px inset footer band common to skill, market, and deck cards: sits directly
 * on the card surface beneath a soft inset hairline so 28×28 carousel slots
 * sit with vertical breathing room inside 160px cards. */
export function AgentFooterBar(props: {
  children?: JSX.Element | (JSX.Element | null)[] | null;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        alignSelf: "stretch",
        gap: 8,
        minWidth: 0,
        height: 38,
        flexShrink: 0,
        marginHorizontal: 14,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      {props.children}
    </View>
  );
}

/** Horizontal agent SVG switch carousel: renders a plain flex row when <= 7
 * agents fit without overflow (zero <Scroll> overhead inside virtualized
 * grids), switching to a 232px horizontal carousel with chevron steppers
 * when 8+ agents are enabled. */
export function AgentRail(props: {
  scopeKey: string;
  railPrefix?: string;
  slotPrefix?: string;
  slots: AgentRailSlot[];
  pending: boolean;
  onSlotClick: (slot: AgentRailSlot) => void;
}): JSX.Element {
  const offsetX = signal(0);
  let scrollRef: ScrollRef | null = null;
  let lastScopeKey = props.scopeKey;
  const compact = computed(() => props.slots.length <= RAIL_MAX_VISIBLE);
  const maxScrollX = computed(() => railMaxScrollX(props.slots.length));

  effect(() => {
    const key = props.scopeKey;
    if (compact()) {
      scrollRef = null;
      offsetX.set(0);
      lastScopeKey = key;
      return;
    }
    if (key !== lastScopeKey) {
      lastScopeKey = key;
      offsetX.set(0);
      scrollRef?.scrollTo({ x: 0, animated: false });
    }
  });

  const clampedOffset = (): number => Math.min(offsetX(), maxScrollX());
  const canPrev = (): boolean => clampedOffset() > 1;
  const canNext = (): boolean => clampedOffset() < maxScrollX() - 1;
  const stepBy = (dir: -1 | 1): void => {
    const max = maxScrollX();
    const next = Math.max(0, Math.min(max, clampedOffset() + dir * RAIL_STEP_PX));
    offsetX.set(next);
    scrollRef?.scrollTo({ x: next, animated: true });
  };

  return (
    <View
      testID={`${props.railPrefix ?? "deck-rail"}-${props.scopeKey}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 2,
        flex: 1,
        minWidth: 0,
        height: "100%",
      }}
    >
      <If
        when={!compact()}
        else={
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: RAIL_SLOT_GAP,
              height: RAIL_SLOT_SIZE,
            }}
          >
            <RailSlots
              scopeKey={props.scopeKey}
              slotPrefix={props.slotPrefix ?? "deck-slot"}
              slots={props.slots}
              pending={props.pending}
              onSlotClick={props.onSlotClick}
            />
          </View>
        }
      >
        <>
          <CarouselArrow
            testID={`${props.railPrefix ?? "deck-rail"}-${props.scopeKey}-prev`}
            icon="chevronLeft"
            label={t("common.previous")}
            enabled={canPrev()}
            onPress={() => stepBy(-1)}
          />
          <Scroll
            ref={(el) => {
              scrollRef = el;
            }}
            testID={`${props.railPrefix ?? "deck-rail"}-${props.scopeKey}-viewport`}
            horizontal
            scrollbar={false}
            bounces={false}
            scrollEventThrottle={32}
            onScroll={(e) => {
              offsetX.set(e.offset.x);
            }}
            style={{
              width: RAIL_VIEWPORT_W,
              height: RAIL_SLOT_SIZE,
              flexShrink: 0,
            }}
            contentStyle={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: RAIL_SLOT_GAP,
              height: RAIL_SLOT_SIZE,
            }}
          >
            <RailSlots
              scopeKey={props.scopeKey}
              slotPrefix={props.slotPrefix ?? "deck-slot"}
              slots={props.slots}
              pending={props.pending}
              onSlotClick={props.onSlotClick}
            />
          </Scroll>
          <CarouselArrow
            testID={`${props.railPrefix ?? "deck-rail"}-${props.scopeKey}-next`}
            icon="chevronRight"
            label={t("common.next")}
            enabled={canNext()}
            onPress={() => stepBy(1)}
          />
        </>
      </If>
    </View>
  );
}

function RailSlots(props: {
  scopeKey: string;
  slotPrefix: string;
  slots: AgentRailSlot[];
  pending: boolean;
  onSlotClick: (slot: AgentRailSlot) => void;
}): JSX.Element {
  return (
    <Each of={props.slots} key="index">
      {(slot) => (
        <RailSlot
          scopeKey={props.scopeKey}
          slotPrefix={props.slotPrefix}
          slot={slot}
          pending={props.pending}
          onClick={props.onSlotClick}
        />
      )}
    </Each>
  );
}

function CarouselArrow(props: {
  testID: string;
  icon: "chevronLeft" | "chevronRight";
  label: string;
  enabled: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: RAIL_ARROW_W,
        height: 24,
        flexShrink: 0,
        borderRadius: 5,
        opacity: props.enabled ? 1 : 0.35,
        cursor: props.enabled ? "pointer" : "default",
      }}
      hoverStyle={props.enabled ? { backgroundColor: paletteToken("panel_hover") } : undefined}
      onPress={() => {
        if (props.enabled) props.onPress();
      }}
    >
      <Icon
        name={props.icon}
        size={12}
        color={props.enabled ? paletteToken("fg_muted") : paletteToken("fg_faint")}
      />
    </View>
  );
}

function RailSlot(props: {
  scopeKey: string;
  slotPrefix: string;
  slot: () => AgentRailSlot;
  pending: boolean;
  onClick: (slot: AgentRailSlot) => void;
}): JSX.Element {
  const hovered = signal(false);
  return (
    <View
      testID={`${props.slotPrefix}-${props.scopeKey}-${props.slot().profile.id}`}
      accessibilityLabel={props.slot().label}
      tooltip={hovered() ? <Tip text={props.slot().label} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: RAIL_SLOT_SIZE,
        height: RAIL_SLOT_SIZE,
        flexShrink: 0,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: props.slot().linked ? paletteToken("accent_soft_edge") : "transparent",
        backgroundColor: props.slot().linked ? paletteToken("accent_soft") : "transparent",
        // Pending swallows a second press but does not dim the slot.
        opacity: props.slot().linked ? 1 : 0.48,
        cursor: props.slot().pending || props.pending ? "default" : "pointer",
      }}
      hoverStyle={
        props.slot().pending || props.pending
          ? undefined
          : props.slot().linked
            ? { borderColor: paletteToken("accent") }
            : {
                backgroundColor: paletteToken("panel_hover"),
                borderColor: paletteToken("border_soft"),
                opacity: 0.92,
              }
      }
      onPress={() => {
        const slot = props.slot();
        if (!slot.pending && !props.pending) props.onClick(slot);
      }}
    >
      <AgentIcon
        id={props.slot().profile.id}
        icon={props.slot().profile.icon}
        size={16}
        grayscale={!props.slot().linked}
      />
    </View>
  );
}
