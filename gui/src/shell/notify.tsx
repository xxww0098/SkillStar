import {
  Canvas,
  Easing,
  Text,
  View,
  createAnimatedValue,
  effect,
  keymap,
  signal,
  type JSX,
} from "nsh";
import { SPIN_MS } from "../chrome/feedback";
import { Icon, type IconName } from "../chrome/icons";
import {
  DURATION_LG,
  DURATION_MD,
  DURATION_REDUCED,
  DURATION_SM,
  DURATION_SM_OUT,
  EASE_IN,
  EASE_OUT,
  EASE_OUT_STRONG,
  motionEnabled,
} from "../chrome/motion";
import { TOOLBAR_H } from "../layout";
import { hexAlpha, isLight, paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { subscribeNotify } from "./events";
import {
  NOTICE_MAX_W,
  createNoticeBoard,
  noticeRemainingMs,
  type NoticeEntry,
} from "./noticeBoard";

// One toast, centered under the page toolbar. The surface is its own card
// (in-page callouts stay on chrome/alerts.tsx). nsh has no `status` role;
// the line is `summary` and Android announces it politely. It does not take
// focus. Escape closes it only while a button inside it is focused.
//
// Enter: 10px up, 96%, blur 8 → rest, 400ms ease-out; opacity finishes
// sooner. Leave: 6px up, blur 6, 200ms ease-in. Reduce Motion fades 150ms.
// Sheen plays for status, a settled progress, and the restored line.

const board = createNoticeBoard();
const noticeClock = signal(0);
const phase = signal<Phase>("from");
const SHEEN_MS = 900;
const ENTER_BLUR = "blur(8px)";
const LEAVE_BLUR = "blur(6px)";
const MONO = "JetBrains Mono, Menlo, Consolas, monospace";

type Phase = "from" | "rest" | "out";

effect(() =>
  subscribeNotify((request) => {
    const now = Date.now();
    noticeClock.set(now);
    board.push(request, now);
  }),
);

function closeToast(seq: number): void {
  if (!board.close(seq, true)) return;
  const wait = motionEnabled() ? DURATION_SM : DURATION_REDUCED;
  setTimeout(() => {
    const current = board.current();
    if (current?.seq === seq && current.closing) board.remove(seq);
  }, wait);
}

/** Remove the toast posted with this replace key (no exit). */
export function dismissReplaceKey(key: string): void {
  board.dismissReplaceKey(key);
}

function scrimShadow(): string {
  const scrim = paletteToken("scrim");
  return `0px 16px 36px ${hexAlpha(scrim, 0.08)}, 0px 3px 10px ${hexAlpha(scrim, 0.05)}`;
}

function toastFill(): string {
  return `linear-gradient(to bottom, ${paletteToken("card")}, ${paletteToken("panel")})`;
}

export function NoticeBand(): JSX.Element {
  effect(() => {
    const entry = board.current();
    if (!entry) {
      phase.set("from");
      return;
    }
    if (entry.closing || entry.held || entry.deadline == null) return;
    const remain = Math.max(0, entry.deadline - Date.now());
    const id = setTimeout(() => {
      const seq = board.due(Date.now());
      if (seq != null) closeToast(seq);
    }, remain);
    return () => clearTimeout(id);
  });
  effect(() => {
    const entry = board.current();
    if (
      !entry ||
      entry.closing ||
      entry.held ||
      !entry.undo ||
      entry.pinned ||
      entry.restored ||
      entry.kind !== "action"
    ) {
      return;
    }
    noticeClock.set(Date.now());
    const id = setInterval(() => {
      noticeClock.set(Date.now());
    }, 100);
    return () => clearInterval(id);
  });
  return (
    <If when={board.entries().length > 0}>
      <View
        style={{
          position: "absolute",
          top: TOOLBAR_H + 12,
          left: 0,
          right: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          zIndex: 50,
          pointerEvents: "box-none",
        }}
      >
        <ToastCard />
      </View>
    </If>
  );
}

function ToastCard(): JSX.Element {
  const hovering = signal(false);
  const focusInside = signal(false);

  effect(() => {
    const entry = board.current();
    if (!entry) return;
    if (entry.closing) {
      phase.set("out");
      return;
    }
    if (phase() === "out" || phase() === "from") {
      if (phase() === "out") phase.set("from");
      const frame = requestAnimationFrame(() => phase.set("rest"));
      return () => cancelAnimationFrame(frame);
    }
  });

  effect(() => {
    const entry = board.current();
    if (!entry || entry.closing) return;
    if ((hovering() || focusInside()) && !entry.held) board.hold(entry.seq, Date.now());
  });

  effect(() => {
    if (!focusInside()) return;
    return keymap.bind("escape", () => {
      const seq = board.current()?.seq;
      if (seq != null) closeToast(seq);
    });
  });

  const focusProps = {
    onFocus: () => {
      focusInside.set(true);
      const seq = board.current()?.seq;
      if (seq != null) board.hold(seq, Date.now());
    },
    onBlur: () => {
      focusInside.set(false);
      setTimeout(() => {
        if (focusInside() || hovering()) return;
        const seq = board.current()?.seq;
        if (seq != null) board.release(seq, Date.now());
      }, 0);
    },
  };

  return (
    <View
      testID={`notice-row-${board.current()?.seq ?? 0}`}
      desktop={{ windowDrag: false }}
      onMouseEnter={() => {
        hovering.set(true);
        const seq = board.current()?.seq;
        if (seq != null) board.hold(seq, Date.now());
      }}
      onMouseLeave={() => {
        hovering.set(false);
        if (focusInside()) return;
        const seq = board.current()?.seq;
        if (seq != null) board.release(seq, Date.now());
      }}
      style={{
        transform: moverTransform(),
        filter: moverFilter(),
        transitionProperty: motionEnabled() ? "transform, filter" : "none",
        transitionDuration: leaveNow() ? DURATION_SM : DURATION_LG,
        transitionTimingFunction: leaveNow() ? EASE_IN : EASE_OUT,
      }}
    >
      <View
        style={{
          opacity: shownNow() ? 1 : 0,
          transitionProperty: "opacity",
          transitionDuration: motionEnabled()
            ? leaveNow()
              ? DURATION_SM
              : DURATION_MD
            : DURATION_REDUCED,
          transitionTimingFunction: leaveNow() ? EASE_IN : EASE_OUT,
        }}
      >
        <View
          style={{
            position: "relative",
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            minHeight: 40,
            maxWidth: NOTICE_MAX_W,
            paddingLeft: 8,
            paddingRight: 6,
            paddingVertical: 6,
            borderRadius: 16,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundImage: toastFill(),
            boxShadow: scrimShadow(),
            overflow: "hidden",
          }}
        >
          <If when={!!board.current()?.sheen && motionEnabled()}>
            <Sheen />
          </If>
          <ToastIcon />
          <ToastCopy />
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 4,
              flexShrink: 0,
            }}
          >
            <If when={board.current()?.action !== undefined && !board.current()?.restored}>
              <View
                testID="notice-action"
                role="button"
                accessibilityLabel={board.current()?.action?.label ?? ""}
                {...focusProps}
                onPress={() => {
                  const entry = board.current();
                  if (!entry?.action) return;
                  entry.action.run();
                  if (entry.undo) board.showRestored(entry.seq, Date.now());
                }}
                style={{
                  display: "flex",
                  height: 28,
                  alignItems: "center",
                  justifyContent: "center",
                  paddingHorizontal: 10,
                  borderRadius: 10,
                  cursor: "pointer",
                  backgroundColor: "transparent",
                }}
                hoverStyle={{ backgroundColor: paletteToken("accent_soft") }}
              >
                <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("accent_fg") }}>
                  {board.current()?.action?.label ?? ""}
                </Text>
              </View>
            </If>
            <View
              testID="notice-close"
              role="button"
              accessibilityLabel={t("common.close")}
              {...focusProps}
              onPress={() => {
                const seq = board.current()?.seq;
                if (seq != null) closeToast(seq);
              }}
              style={{
                display: "flex",
                width: 28,
                height: 28,
                alignItems: "center",
                justifyContent: "center",
                borderRadius: 10,
                cursor: "pointer",
                backgroundColor: "transparent",
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            >
              <Icon name="x" size={14} color={paletteToken("fg_muted")} />
            </View>
          </View>
        </View>
      </View>
    </View>
  );
}

function leaveNow(): boolean {
  return board.current()?.closing === true || phase() === "out";
}

function shownNow(): boolean {
  return phase() === "rest" && board.current()?.closing !== true;
}

function moverTransform(): string {
  if (!motionEnabled()) return "translateY(0px) scale(1)";
  if (leaveNow()) return "translateY(-6px) scale(1)";
  if (!shownNow()) return "translateY(-10px) scale(0.96)";
  return "translateY(0px) scale(1)";
}

function moverFilter(): string {
  if (!motionEnabled()) return "blur(0px)";
  if (leaveNow()) return LEAVE_BLUR;
  if (!shownNow()) return ENTER_BLUR;
  return "blur(0px)";
}

function Sheen(): JSX.Element {
  const x = createAnimatedValue(-160);
  effect(() => {
    const entry = board.current();
    if (!entry?.sheen || !motionEnabled()) return;
    const generation = entry.generation;
    void generation;
    x.set(-160);
    void x.timing(680, {
      duration: SHEEN_MS,
      easing: Easing.bezier(0, 0, 0.58, 1),
    });
    return () => {
      void x.stop();
    };
  });
  return (
    <View
      style={{
        position: "absolute",
        top: 0,
        bottom: 0,
        width: 96,
        pointerEvents: "none",
        transform: [{ translateX: x }],
        backgroundImage: `linear-gradient(90deg, ${hexAlpha(paletteToken("accent_fg"), 0)}, ${hexAlpha(
          paletteToken("accent_fg"),
          isLight() ? 0.2 : 0.1,
        )}, ${hexAlpha(paletteToken("accent_fg"), 0)})`,
      }}
    />
  );
}

function iconIdentity(entry: NoticeEntry | undefined): string {
  if (!entry) return "";
  if (entry.restored) return "restored";
  if (entry.kind === "progress" && entry.pending !== false) return "progress";
  if (entry.undo && !entry.pinned && !entry.restored) return "undo";
  return entry.tone;
}

function ToastIcon(): JSX.Element {
  const from = signal(false);
  let last = "";
  effect(() => {
    const entry = board.current();
    if (!entry) return;
    const key = iconIdentity(entry);
    if (last !== "" && last !== key && motionEnabled()) {
      from.set(true);
      requestAnimationFrame(() => from.set(false));
    }
    last = key;
  });
  return (
    <View
      style={{
        width: 24,
        height: 24,
        borderRadius: 8,
        overflow: "hidden",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        backgroundColor: wellBg(),
        transform: from() && motionEnabled() ? "rotate(-40deg) scale(0.5)" : "rotate(0deg) scale(1)",
        filter: from() && motionEnabled() ? "blur(4px)" : "blur(0px)",
        transitionProperty: motionEnabled() ? "transform, filter" : "none",
        transitionDuration: DURATION_LG,
        transitionTimingFunction: EASE_OUT_STRONG,
      }}
    >
      <If
        when={isUndo()}
        else={
          <If when={isProgress()} else={<ToneGlyph />}>
            <ProgressRing />
          </If>
        }
      >
        <UndoRing />
      </If>
    </View>
  );
}

function isUndo(): boolean {
  const entry = board.current();
  return !!entry && !!entry.undo && !entry.pinned && !entry.restored && entry.kind === "action";
}

function isProgress(): boolean {
  const entry = board.current();
  return !!entry && entry.kind === "progress" && entry.pending !== false && !entry.restored;
}

function wellBg(): string {
  const entry = board.current();
  if (!entry || entry.restored || entry.tone === "success" || isUndo() || isProgress()) {
    return paletteToken("accent_soft");
  }
  if (entry.tone === "error") return paletteToken("danger_bg");
  if (entry.tone === "warning") return paletteToken("warn_bg");
  return paletteToken("info_bg");
}

function glyphName(): IconName {
  const entry = board.current();
  if (!entry || entry.restored || entry.tone === "success") return "check";
  if (entry.tone === "warning") return "alertTriangle";
  if (entry.tone === "error") return "x";
  return "info";
}

function glyphColor(): string {
  const entry = board.current();
  if (!entry || entry.restored || entry.tone === "success") return paletteToken("accent_fg");
  if (entry.tone === "error") return paletteToken("danger_fg");
  if (entry.tone === "warning") return paletteToken("warn");
  return paletteToken("info");
}

function ToneGlyph(): JSX.Element {
  return <Icon name={glyphName()} size={14} color={glyphColor()} />;
}

function undoFraction(): number {
  const entry = board.current();
  if (!entry?.lifeMs) return 0;
  return Math.max(0, Math.min(1, noticeRemainingMs(entry, noticeClock()) / entry.lifeMs));
}

function undoSeconds(): number {
  const entry = board.current();
  if (!entry) return 0;
  return Math.ceil(noticeRemainingMs(entry, noticeClock()) / 1000);
}

function UndoRing(): JSX.Element {
  return (
    <View
      style={{
        position: "relative",
        width: 24,
        height: 24,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Ring progress={undoFraction()} />
      <Text
        style={{
          position: "absolute",
          fontSize: 10,
          lineHeight: 12,
          fontFamily: MONO,
          fontVariant: ["tabular-nums"],
          color: paletteToken("accent_fg"),
        }}
      >
        {undoSeconds()}
      </Text>
    </View>
  );
}

function ProgressRing(): JSX.Element {
  return (
    <View
      style={
        motionEnabled()
          ? {
              position: "relative",
              width: 24,
              height: 24,
              animationName: "spin",
              animationDuration: SPIN_MS,
              animationIterationCount: "infinite",
              animationTimingFunction: "linear",
            }
          : { position: "relative", width: 24, height: 24 }
      }
    >
      <Ring progress={0.28} />
    </View>
  );
}

function Ring(props: { progress: number }): JSX.Element {
  return (
    <Canvas
      style={{ position: "absolute", top: 0, left: 0, width: 24, height: 24 }}
      draw={(cx) => {
        const progress = Math.max(0, Math.min(1, props.progress));
        cx.beginPath();
        cx.arc(12, 12, 9, 0, Math.PI * 2);
        cx.strokeStyle = hexAlpha(paletteToken("accent_fg"), 0.28);
        cx.lineWidth = 2;
        cx.lineCap = "round";
        cx.stroke();
        if (progress <= 0) return;
        cx.beginPath();
        const start = -Math.PI / 2;
        cx.arc(12, 12, 9, start, start + Math.PI * 2 * progress);
        cx.strokeStyle = paletteToken("accent_fg");
        cx.stroke();
      }}
    />
  );
}

function lineText(): string {
  const entry = board.current();
  if (!entry) return "";
  return entry.restored ? t("notify.restored") : entry.message;
}

function ToastCopy(): JSX.Element {
  const line = signal("");
  const prev = signal<string | null>(null);
  const prevOut = signal(false);
  const lineIn = signal(true);
  let token = 0;
  effect(() => {
    const entry = board.current();
    if (!entry || entry.closing) return;
    const text = entry.restored ? t("notify.restored") : entry.message;
    const current = line();
    if (current !== "" && current !== text) {
      prev.set(current);
      prevOut.set(false);
      lineIn.set(false);
      const mine = ++token;
      requestAnimationFrame(() => prevOut.set(true));
      setTimeout(() => {
        if (mine !== token) return;
        lineIn.set(true);
      }, motionEnabled() ? 100 : 0);
      setTimeout(() => {
        if (mine === token) prev.set(null);
      }, motionEnabled() ? DURATION_SM_OUT : DURATION_REDUCED);
    }
    if (current !== text) line.set(text);
  });
  return (
    <View style={{ position: "relative", flexShrink: 1, minWidth: 0, maxWidth: copyMax() }}>
      <If when={prev() !== null}>
        <Text
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: 0,
            fontSize: 14,
            lineHeight: 20,
            color: paletteToken("fg"),
            opacity: prevOut() ? 0 : 1,
            transform: prevOut() && motionEnabled() ? "translateY(-5px)" : "translateY(0px)",
            filter: prevOut() && motionEnabled() ? "blur(4px)" : "blur(0px)",
            transitionProperty: motionEnabled() ? "opacity, transform, filter" : "opacity",
            transitionDuration: motionEnabled() ? DURATION_SM_OUT : DURATION_REDUCED,
            transitionTimingFunction: EASE_IN,
          }}
        >
          {prev() ?? ""}
        </Text>
      </If>
      <Text
        role="summary"
        android={{ accessibilityLiveRegion: "polite" }}
        style={{
          fontSize: 14,
          lineHeight: 20,
          color: paletteToken("fg"),
          opacity: lineIn() ? 1 : 0,
          transform: !lineIn() && motionEnabled() ? "translateY(5px)" : "translateY(0px)",
          filter: !lineIn() && motionEnabled() ? "blur(4px)" : "blur(0px)",
          transitionProperty: motionEnabled() ? "opacity, transform, filter" : "opacity",
          transitionDuration: motionEnabled() ? DURATION_MD : DURATION_REDUCED,
          transitionTimingFunction: EASE_OUT_STRONG,
        }}
      >
        {lineText()}
      </Text>
    </View>
  );
}

function copyMax(): number {
  const entry = board.current();
  const action = entry?.action && !entry.restored ? 88 : 0;
  return NOTICE_MAX_W - 8 - 24 - 8 - 28 - 6 - 8 - action;
}
