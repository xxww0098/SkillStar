import { Text, View, effect, signal, type JSX } from "nsh";
import { paletteToken } from "../services/themeMode";
import { subscribeNotify } from "./events";
import { Alert } from "../chrome/alerts";
import { motionEnabled } from "../chrome/motion";
import {
  NOTICE_ANIM_MS,
  NOTICE_TTL_MS,
  NOTICE_MAX_W,
  createNoticeBoard,
  type NoticeEntry,
} from "./noticeBoard";

// The alert band — notify.rs parity (render + wiring around the board core).
//
// Pages publish through events.ts (`toast`, `publishToast`); the scheduling
// rules (TTL, pinning, replace keys, the cap) live in noticeBoard.ts and are
// unit-tested there (the GPUI side tests the same rules, notify.rs:396-485).
// This layer renders the stack floating at the top center of the main
// panel: no layout space (absolute), rows intercept presses only — never
// the wheel — copy wraps at 560px, and enter/exit ride the opacity/transform
// channel (Reduce Motion snaps both and skips the animation delay).

const board = createNoticeBoard();

/** Rows whose enter motion settled (FadeSlide rest position). */
const settled = signal<Set<number>>(new Set());

effect(() =>
  subscribeNotify((request) => {
    board.push(request, (seq) => {
      setTimeout(() => close(seq), NOTICE_TTL_MS);
    });
    if (motionEnabled()) {
      // One displaced frame, then rest: the enter transition needs a
      // "from" state to animate from.
      requestAnimationFrame(() =>
        settled.set(new Set([...settled(), ...board.entries().map((e) => e.seq)]))
      );
    } else {
      settled.set(new Set([...settled(), ...board.entries().map((e) => e.seq)]));
    }
  })
);

function close(seq: number): void {
  const animate = motionEnabled();
  if (!board.close(seq, animate)) return;
  if (!animate) {
    const next = new Set(settled());
    next.delete(seq);
    settled.set(next);
    return;
  }
  setTimeout(() => {
    board.remove(seq);
    const next = new Set(settled());
    next.delete(seq);
    settled.set(next);
  }, NOTICE_ANIM_MS);
}

/** Remove every notice posted with this replace key (notify.rs dismiss). */
export function dismissReplaceKey(key: string): void {
  board.dismissReplaceKey(key);
}

/**
 * The band. Mount once as the main panel's last child so it paints above
 * the page; empty renders nothing so no overlay exists while idle.
 */
export function NoticeBand(): JSX.Element {
  // nsh components run once — the empty check rides `<If>` and the rows ride
  // a child expression, both reactive spots (a component-body read of the
  // board would freeze the band at its first render and no notice would
  // ever show).
  return (
    <If when={board.entries().length > 0}>
      <View
        style={{
          position: "absolute",
          top: 8,
          left: 0,
          right: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 4,
          zIndex: 50,
          pointerEvents: "box-none",
        }}
      >
        {board.entries().map((entry) => (
          <NoticeRow entry={entry} />
        ))}
      </View>
    </If>
  );
}

function NoticeRow(props: { entry: NoticeEntry }): JSX.Element {
  const entry = props.entry;
  // Same reactive rule as the band: `settled` is read inside the style
  // expressions (a body read would freeze the row at its enter frame).
  return (
    <View
      testID={`notice-row-${entry.seq}`}
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        // Blocks presses over its own alert only; wheel passes through —
        // no scroll interception here by design (block_mouse_except_scroll).
        transform: `translateY(${
          entry.closing ? -8 : settled().has(entry.seq) ? 0 : -8
        }px)`,
        opacity: entry.closing ? 0 : settled().has(entry.seq) ? 1 : 0,
        ...(settled().has(entry.seq) || entry.closing
          ? {
              transitionProperty: "opacity, transform",
              transitionDuration: NOTICE_ANIM_MS,
              transitionTimingFunction: "ease-out",
            }
          : {}),
      }}
    >
      <Alert
        tone={entry.tone}
        message={entry.message}
        maxWidth={NOTICE_MAX_W}
        onClose={() => close(entry.seq)}
      />
      {entry.action ? (
        <NoticeAction label={entry.action.label} run={entry.action.run} />
      ) : null}
    </View>
  );
}

function NoticeAction(props: { label: string; run: () => void }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        height: 28,
        alignItems: "center",
        justifyContent: "center",
        paddingHorizontal: 12,
        borderRadius: 6,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={props.run}
    >
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{props.label}</Text>
    </View>
  );
}
