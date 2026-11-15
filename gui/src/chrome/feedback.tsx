import { Easing, View, createAnimatedValue, effect, type JSX, type ViewStyle } from "nsh";
import { hexAlpha, paletteToken } from "../services/themeMode";
import { motionEnabled } from "./motion";
import { Icon, type IconName } from "./icons";

// Loading feedback — chrome::Pulse / icon_spin / icon_sweep parity
// (chrome/mod.rs:85-238).
//
// - Skeleton bars pulse on a stepped cadence (PULSE_FPS 20 in GPUI; the
//   native `pulse` keyframes here sample below refresh on purpose) and stay
//   static under Reduce Motion.
// - Spinners turn 0.8s per revolution (SPIN_MS).
// - The translate button's laser sweeps a glyph left to right once per
//   second (SWEEP_PASS_MS) and the glyph dims under it (IconSweep).
// - Update ellipsis steps ~400ms — `Ellipsis` cycles "…" prefixes on a
//   timer, and stops under Reduce Motion.
//
// nsh 纪律：组件体只执行一次，所以「按 props 换形」一律走 JSX 的 <If>
// 表达式，不在函数顶部对 props 做一次性判断——那样 prop 之后再变，组件
// 不会重画（首帧是静止字形就永远是静止字形）。

/** Spinner cadence: 0.8s per revolution. */
export const SPIN_MS = 800;

/** One sweep pass across the glyph (GPUI's Animation::new(1s)). */
export const SWEEP_PASS_MS = 1000;

/** Beam width: a fading tail plus SWEEP_CORE_W of bright leading edge
 * (chrome/mod.rs SWEEP_BEAM_W / SWEEP_CORE_W). */
const SWEEP_BEAM_W = 10;
const SWEEP_CORE_W = 2;

/** Ellipsis step cadence (~400ms per step). */
export const ELLIPSIS_MS = 400;

/** A spinning glyph — icon_spin parity. `spin` false renders it at rest. */
export function IconSpin(props: {
  name: IconName;
  size?: number;
  color: string;
  spin: boolean;
  /** Test id on the turning frame (the spinning half only). */
  testID?: string;
}): JSX.Element {
  const size = props.size ?? 16;
  return (
    <If
      when={props.spin}
      else={<Icon name={props.name} size={size} color={props.color} />}
    >
      <View
        testID={props.testID}
        style={{
          width: size,
          height: size,
          ...spinStyle(),
        }}
      >
        <Icon name={props.name} size={size} color={props.color} />
      </View>
    </If>
  );
}

/** A swept glyph — icon_sweep parity (chrome/mod.rs:113-162). While `sweep`
 * is set a laser beam crosses the glyph left to right once per second and the
 * glyph dims to 0.35, so the beam reads as the moving part; the pass stops (and
 * settles out of sight) as soon as the flag drops, and Reduce Motion skips the
 * pass, leaving the dimmed glyph. The beam is an animated `translateX` on a
 * clipped one-glyph frame, so its frames never lay the page out again. */
export function IconSweep(props: {
  name: IconName;
  size?: number;
  color: string;
  sweep: boolean;
  /** Test id on the laser beam (the sweeping half only). */
  testID?: string;
}): JSX.Element {
  const size = props.size ?? 16;
  const beam = createAnimatedValue(-SWEEP_BEAM_W);
  effect(() => {
    // props.sweep 在 effect 里读取：pending 从无到有、从有到无都重新起停。
    if (!props.sweep || !motionEnabled()) return;
    let stopped = false;
    const passes = async () => {
      beam.set(-SWEEP_BEAM_W);
      while (!stopped) {
        const { finished } = await beam.keyframes([-SWEEP_BEAM_W, size], {
          duration: SWEEP_PASS_MS,
          easing: Easing.linear,
        });
        if (!finished) break;
      }
    };
    void passes();
    return () => {
      stopped = true;
      // 停扫并停回画面外：系统降低动态效果或 pending 收走时，光束不会冻
      // 在字形中间（GPUI 的 icon_sweep 同样「收起在看不见的地方」）。
      beam.set(-SWEEP_BEAM_W);
    };
  });
  const laser = paletteToken("accent");
  return (
    <If
      when={props.sweep}
      else={<Icon name={props.name} size={size} color={props.color} />}
    >
      <View
        style={{
          position: "relative",
          width: size,
          height: size,
          flexShrink: 0,
          overflow: "hidden",
        }}
      >
        <View style={{ width: "100%", height: "100%", opacity: 0.35 }}>
          <Icon name={props.name} size={size} color={props.color} />
        </View>
        <View
          testID={props.testID}
          style={{
            position: "absolute",
            top: 0,
            bottom: 0,
            left: 0,
            width: SWEEP_BEAM_W,
            transform: [{ translateX: beam }],
          }}
        >
          <View
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              left: 0,
              width: SWEEP_BEAM_W - SWEEP_CORE_W,
              backgroundImage: `linear-gradient(90deg, ${hexAlpha(laser, 0)}, ${hexAlpha(
                laser,
                0.65,
              )})`,
            }}
          />
          <View
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              right: 0,
              width: SWEEP_CORE_W,
              backgroundColor: hexAlpha(laser, 0.95),
            }}
          />
        </View>
      </View>
    </If>
  );
}

function spinStyle(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    animationName: "spin",
    animationDuration: SPIN_MS,
    animationIterationCount: "infinite",
    animationTimingFunction: "linear",
  };
}

/** Skeleton bar. `secondary` halves the fill's presence. Pulse stops under RM. */
export function Pulse(props: {
  id: string;
  secondary?: boolean;
  style?: ViewStyle;
}): JSX.Element {
  const fill = paletteToken("well");
  return (
    <View
      testID={props.id}
      style={{
        width: "100%",
        height: 16,
        backgroundColor: fill,
        opacity: props.secondary ? 0.5 : 1,
        ...pulseStyle(),
        ...props.style,
      }}
    />
  );
}

function pulseStyle(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    animationName: "pulse",
    animationDuration: 2000,
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
  };
}
