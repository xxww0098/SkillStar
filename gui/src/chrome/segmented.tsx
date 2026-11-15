import { Text, View, signal, type JSX } from "nsh";
import { mixHsl, paletteToken } from "../services/themeMode";
import { MOTION_MS, motionEnabled, motionTransition, stretchTransition } from "./motion";
import { Icon, type IconName } from "./icons";

// Sliding-capsule segmented control — chrome::segmented parity
// (chrome/segmented.rs). One absolutely positioned capsule glides between
// equal slots: the slide rides a transform transition (channel 2, ~80ms
// settle — motionTransition()), so left/top never animate, and the capsule
// stretches toward the slot it travels to while it goes, then lands at its
// exact slot width. Segments stay transparent and crossfade their ink by the
// same position, so the highlight glides instead of each segment repainting
// its own fill. Reduce Motion drops the slide and the stretch: the capsule
// lands on its end slot at once.
//
// The capsule is a nested pair on purpose — the outer view carries the
// position (translate), the inner the stretch (scale) — so each animates one
// transform function and neither animates width/height/margin.
//
// Two flavors, as in the GPUI side: `SliderSegmented` for icon rows (the
// sidebar mode switcher, the my-skills scope switch) and `TextSlider` for the
// one settings geometry (72×32 slots: translation language, translation mode,
// skill update mode, background style, UI language).

/** How far the capsule stretches toward the slot it travels to (12%). */
const STRETCH = 0.12;

/** The capsule's resting elevation (kit shadow_sm: one soft black layer). */
const CAPSULE_SHADOW = "0px 1px 2px rgba(0,0,0,0.12)";

export interface SegmentOption {
  id: string;
  icon?: IconName;
  label?: string | (() => string);
}

export interface SliderGeometry {
  /** Slot width, excluding the gap. */
  slotW: number;
  /** Slot height, excluding the gap. */
  slotH: number;
  /** Track inner padding per side. */
  pad: number;
  /** Segment icon size. */
  iconSize: number;
}

const GAP = 2;

/** Settings-density text slots (segmented.rs TEXT_SLOT). */
export const TEXT_SLOT: SliderGeometry = { slotW: 72, slotH: 32, pad: 2, iconSize: 14 };

export function TextSlider(props: {
  motionId: string;
  options: SegmentOption[];
  selected: number;
  onClick: (index: number) => void;
}): JSX.Element {
  return <SliderSegmented {...props} geo={TEXT_SLOT} vertical={false} />;
}

export function SliderSegmented(props: {
  motionId: string;
  options: SegmentOption[];
  /** The selected index, or an accessor for it. Pass an accessor when the
   * value comes from a signal: this component body runs once, so a plain
   * number read here would never move the capsule again. */
  selected: number | (() => number);
  geo: SliderGeometry;
  vertical?: boolean;
  onClick: (index: number) => void;
}): JSX.Element {
  const { geo, options } = props;
  const selected = () =>
    typeof props.selected === "function" ? props.selected() : props.selected;
  const vertical = props.vertical ?? false;
  const stride = (vertical ? geo.slotH : geo.slotW) + GAP;
  const count = options.length;
  const trackW = vertical
    ? geo.slotW + geo.pad * 2 + 2
    : count * geo.slotW + (count - 1) * GAP + geo.pad * 2 + 2;
  const trackH = vertical
    ? count * geo.slotH + (count - 1) * GAP + geo.pad * 2 + 2
    : geo.slotH + geo.pad * 2 + 2;
  // Capsule radii come from the heights, so the ends are always half-rounds
  // whatever the caller's slot size is.
  const capsuleRadius = Math.round(geo.slotH / 2);
  const trackRadius = Math.round(trackH / 2);

  // Stretch state: 1 at rest, 1 + STRETCH while the capsule travels. The
  // anchor is the trailing edge, so the growth reaches into the slot it is
  // going to and never leaves the track.
  const stretch = signal(1);
  const anchor = signal(vertical ? "center top" : "left center");
  let settleTimer: ReturnType<typeof setTimeout> | null = null;

  function kick(forward: boolean): void {
    if (!motionEnabled() || count < 2) return;
    if (settleTimer !== null) clearTimeout(settleTimer);
    anchor.set(
      vertical
        ? forward
          ? "center top"
          : "center bottom"
        : forward
          ? "left center"
          : "right center",
    );
    stretch.set(1 + STRETCH);
    settleTimer = setTimeout(() => {
      settleTimer = null;
      stretch.set(1);
    }, MOTION_MS);
  }

  function pick(index: number): void {
    const from = selected();
    if (index !== from) kick(index > from);
    props.onClick(index);
  }

  return (
    <View
      role="tablist"
      desktop={{ windowDrag: false }}
      style={{
        position: "relative",
        display: "flex",
        flexDirection: vertical ? "column" : "row",
        gap: GAP,
        padding: geo.pad,
        borderRadius: trackRadius,
        backgroundColor: paletteToken("bg"),
        borderWidth: 1,
        borderColor: paletteToken("border"),
        width: trackW,
        height: trackH,
      }}
    >
      {/* Out-of-flow capsule: fixed at `pad`, slid by transform only. */}
      <View
        style={{
          position: "absolute",
          width: geo.slotW,
          height: geo.slotH,
          left: geo.pad,
          top: geo.pad,
          zIndex: 0,
          transform: vertical
            ? `translateY(${selected() * stride}px)`
            : `translateX(${selected() * stride}px)`,
          ...motionTransition(),
        }}
      >
        <View
          style={{
            width: "100%",
            height: "100%",
            borderRadius: capsuleRadius,
            backgroundColor: paletteToken("card"),
            borderWidth: 1,
            borderColor: paletteToken("border"),
            boxShadow: CAPSULE_SHADOW,
            transformOrigin: anchor(),
            transform: vertical
              ? `scaleY(${stretch()})`
              : `scaleX(${stretch()})`,
            ...stretchTransition(),
          }}
        />
      </View>
      {options.map((seg, ix) => (
        <SegmentPill
          seg={seg}
          geo={geo}
          active={ix === selected()}
          near={Math.min(1, Math.max(0, 1 - Math.abs(selected() - ix)))}
          onClick={() => pick(ix)}
        />
      ))}
    </View>
  );
}

function SegmentPill(props: {
  seg: SegmentOption;
  geo: SliderGeometry;
  active: boolean;
  near: number;
  onClick: () => void;
}): JSX.Element {
  // Props stay getters on purpose: nsh runs this body once, so only values
  // read inside a JSX expression follow the parent signal.
  const ink = () => mixHsl(paletteToken("fg_muted"), paletteToken("fg"), props.near);
  const iconInk = () =>
    mixHsl(paletteToken("fg_muted"), paletteToken("accent_fg"), props.near);
  return (
    <View
      role="tab"
      testID={props.seg.id}
      style={{
        position: "relative",
        zIndex: 1,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        width: props.geo.slotW,
        height: props.geo.slotH,
        borderRadius: Math.round(props.geo.slotH / 2),
        cursor: "pointer",
      }}
      onPress={props.onClick}
    >
      {props.seg.icon ? (
        <Icon name={props.seg.icon} size={props.geo.iconSize} color={iconInk()} />
      ) : null}
      {props.seg.label ? (
        <Text
          style={{
            fontSize: 12,
            color: ink(),
            fontWeight: props.active ? "600" : "400",
          }}
        >
          {typeof props.seg.label === "function" ? props.seg.label() : props.seg.label}
        </Text>
      ) : null}
    </View>
  );
}
