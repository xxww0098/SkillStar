import { Text, View, cleanup, computed, signal, untracked, type JSX } from "nsh";
import { mixHsl, paletteToken } from "../services/themeMode";
import { MOTION_MS, motionEnabled, motionTransition, stretchTransition } from "./motion";
import { Icon, type IconName } from "./icons";

// Sliding-capsule segmented control — chrome::segmented parity
// (chrome/segmented.rs). One absolutely positioned capsule glides between
// equal slots: the slide rides a transform transition (on-screen move,
// --ease-in-out, --duration-sm — motionTransition()), so left/top never
// animate, and the capsule stretches toward the slot it travels to while it
// goes, then lands at its exact slot width. Segments stay transparent and
// crossfade their ink by the same position, so the highlight glides instead
// of each segment repainting its own fill. Reduce Motion drops the slide and
// the stretch: the capsule lands on its end slot at once.
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
  /** Fixed slot radius. Omitted keeps the half-round capsule (slotH / 2). */
  slotRadius?: number;
  /** Fixed track radius. Omitted keeps the half-round ends (trackH / 2). */
  trackRadius?: number;
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
  geo: SliderGeometry | (() => SliderGeometry);
  vertical?: boolean | (() => boolean);
  onClick: (index: number) => void;
}): JSX.Element {
  const geo = () => (typeof props.geo === "function" ? props.geo() : props.geo);
  const vertical = () =>
    typeof props.vertical === "function" ? props.vertical() : Boolean(props.vertical);
  const selected = () =>
    typeof props.selected === "function" ? props.selected() : props.selected;
  const count = () => props.options.length;
  const stride = () => (vertical() ? geo().slotH : geo().slotW) + GAP;
  const trackW = () =>
    vertical()
      ? geo().slotW + geo().pad * 2 + 2
      : count() * geo().slotW + Math.max(0, count() - 1) * GAP + geo().pad * 2 + 2;
  const trackH = () =>
    vertical()
      ? count() * geo().slotH + Math.max(0, count() - 1) * GAP + geo().pad * 2 + 2
      : geo().slotH + geo().pad * 2 + 2;
  // Capsule radii come from the heights, so the ends are always half-rounds
  // whatever the caller's slot size is; a caller can pin them instead (the
  // page toolbar's scope switch rides the bar scale: 10 track / 7 slot).
  const capsuleRadius = () => geo().slotRadius ?? Math.round(geo().slotH / 2);
  const trackRadius = () => geo().trackRadius ?? Math.round(trackH() / 2);

  let lastVertical = untracked(vertical);
  let lastSelected = untracked(selected);
  const capsuleMotion = computed(() => {
    const v = vertical();
    const s = selected();
    const vChanged = v !== lastVertical;
    const sChanged = s !== lastSelected;
    lastVertical = v;
    lastSelected = s;
    return !vChanged || sChanged;
  });

  // Stretch state: 1 at rest, 1 + STRETCH while the capsule travels. The
  // anchor is the trailing edge, so the growth reaches into the slot it is
  // going to and never leaves the track.
  const stretch = signal(1);
  const anchorSide = signal<"start" | "end">("start");
  const anchor = () =>
    vertical()
      ? anchorSide() === "start"
        ? "center top"
        : "center bottom"
      : anchorSide() === "start"
        ? "left center"
        : "right center";
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  cleanup(() => {
    if (settleTimer !== null) {
      clearTimeout(settleTimer);
      settleTimer = null;
    }
  });

  function kick(forward: boolean): void {
    if (!motionEnabled() || count() < 2) return;
    if (settleTimer !== null) clearTimeout(settleTimer);
    anchorSide.set(forward ? "start" : "end");
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
        flexDirection: vertical() ? "column" : "row",
        gap: GAP,
        padding: geo().pad,
        borderRadius: trackRadius(),
        backgroundColor: paletteToken("bg"),
        borderWidth: 1,
        borderColor: paletteToken("border"),
        width: trackW(),
        height: trackH(),
      }}
    >
      {/* Out-of-flow capsule: fixed at `pad`, slid by transform only. */}
      <View
        style={{
          position: "absolute",
          width: geo().slotW,
          height: geo().slotH,
          left: geo().pad,
          top: geo().pad,
          zIndex: 0,
          transform: vertical()
            ? `translateY(${selected() * stride()}px)`
            : `translateX(${selected() * stride()}px)`,
          ...(capsuleMotion() ? motionTransition() : {}),
        }}
      >
        <View
          style={{
            width: "100%",
            height: "100%",
            borderRadius: capsuleRadius(),
            backgroundColor: paletteToken("card"),
            borderWidth: 1,
            borderColor: paletteToken("border"),
            boxShadow: CAPSULE_SHADOW,
            transformOrigin: anchor(),
            transform: vertical()
              ? `scaleY(${stretch()})`
              : `scaleX(${stretch()})`,
            ...(capsuleMotion() ? stretchTransition() : {}),
          }}
        />
      </View>
      <Each of={props.options} key={(seg) => seg.id}>
        {(seg, index) => (
          <SegmentPill
            seg={seg}
            index={index}
            geo={geo}
            selected={selected}
            onClick={pick}
          />
        )}
      </Each>
    </View>
  );
}

function SegmentPill(props: {
  seg: () => SegmentOption;
  index: () => number;
  geo: () => SliderGeometry;
  selected: () => number;
  onClick: (index: number) => void;
}): JSX.Element {
  // Props stay getters on purpose: nsh runs this body once, so only values
  // read inside a JSX expression follow the parent signal.
  const active = () => props.index() === props.selected();
  const near = () => Math.min(1, Math.max(0, 1 - Math.abs(props.selected() - props.index())));
  const ink = () => mixHsl(paletteToken("fg_muted"), paletteToken("fg"), near());
  const iconInk = () =>
    mixHsl(paletteToken("fg_muted"), paletteToken("accent_fg"), near());
  const label = () => {
    const raw = props.seg().label;
    return typeof raw === "function" ? raw() : raw;
  };
  return (
    <View
      role="tab"
      testID={props.seg().id}
      style={{
        position: "relative",
        zIndex: 1,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        width: props.geo().slotW,
        height: props.geo().slotH,
        borderRadius: props.geo().slotRadius ?? Math.round(props.geo().slotH / 2),
        cursor: "pointer",
      }}
      onPress={() => props.onClick(props.index())}
    >
      <If when={props.seg().icon}>
        {(icon) => <Icon name={icon()} size={props.geo().iconSize} color={iconInk()} />}
      </If>
      <If when={props.seg().label !== undefined}>
        <Text
          style={{
            display: label() ? "flex" : "none",
            fontSize: 12,
            color: ink(),
            fontWeight: active() ? "600" : "400",
          }}
        >
          {label() ?? ""}
        </Text>
      </If>
    </View>
  );
}
