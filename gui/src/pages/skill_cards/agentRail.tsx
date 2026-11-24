import { Scroll, Text, View, computed, effect, signal, type JSX, type ScrollRef } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import {
  AgentBadge,
  AgentFooterBar,
  AgentIcon,
  RAIL_MAX_VISIBLE,
  RAIL_SLOT_GAP,
  RAIL_SLOT_SIZE,
  RAIL_VIEWPORT_W,
  railMaxScrollX,
  type AgentRailProfile,
} from "../../chrome/agentRail";
import type { AgentLinkState } from "./deckQuery";

export { AgentBadge, AgentFooterBar, AgentIcon };

export interface AgentRailSlot {
  profile: AgentRailProfile;
  linked: boolean;
  state?: AgentLinkState;
  pending: boolean;
  label: string;
}

const RAIL_ARROW_W = 16;
const RAIL_STEP_PX = 2 * (RAIL_SLOT_SIZE + RAIL_SLOT_GAP);

function slotState(slot: AgentRailSlot): AgentLinkState {
  if (slot.state === "mixed" || slot.state === "linked" || slot.state === "none") {
    return slot.state;
  }
  return slot.linked ? "linked" : "none";
}

/**
 * Deck-aware agent carousel supporting `none | mixed | linked` per slot
 * without altering the shared `src/chrome/agentRail.tsx`.
 */
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
  const state = () => slotState(props.slot());
  const isLinked = () => state() === "linked";
  const isMixed = () => state() === "mixed";
  return (
    <View
      testID={`${props.slotPrefix}-${props.scopeKey}-${props.slot().profile.id}`}
      accessibilityLabel={props.slot().label}
      tooltip={hovered() ? <Tip text={props.slot().label} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: RAIL_SLOT_SIZE,
        height: RAIL_SLOT_SIZE,
        flexShrink: 0,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: isLinked()
          ? paletteToken("accent_soft_edge")
          : isMixed()
            ? paletteToken("warn_border")
            : "transparent",
        backgroundColor: isLinked()
          ? paletteToken("accent_soft")
          : isMixed()
            ? paletteToken("warn_bg")
            : "transparent",
        opacity: isLinked() || isMixed() ? 1 : 0.48,
        cursor: props.slot().pending || props.pending ? "default" : "pointer",
      }}
      hoverStyle={
        props.slot().pending || props.pending
          ? undefined
          : isLinked()
            ? { borderColor: paletteToken("accent") }
            : isMixed()
              ? { borderColor: paletteToken("warn") }
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
        grayscale={state() === "none"}
      />
      <If when={isMixed()}>
        <View
          testID={`${props.slotPrefix}-${props.scopeKey}-${props.slot().profile.id}-mixed`}
          style={{
            position: "absolute",
            top: 2,
            right: 2,
            width: 6,
            height: 6,
            borderRadius: 999,
            backgroundColor: paletteToken("warn"),
          }}
        />
      </If>
    </View>
  );
}

/** The layers count badge the deck card rides in its header actions
 * (group_card.rs:104-122): an info badge — Layers glyph plus the bare
 * member count, never a unit word. */
export function DeckCountBadge(props: { count: number }): JSX.Element {
  return (
    <View
      testID="deck-count-badge"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 3,
        paddingHorizontal: 6,
        paddingVertical: 2,
        borderRadius: 999,
        backgroundColor: paletteToken("info_bg"),
        borderWidth: 1,
        borderColor: paletteToken("info_border"),
      }}
    >
      <Icon name="layers" size={11} color={paletteToken("info")} />
      <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("info") }}>
        {String(props.count)}
      </Text>
    </View>
  );
}
