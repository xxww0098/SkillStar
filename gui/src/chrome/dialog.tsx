import { Overlay, Text, View, effect, signal, untracked, type JSX } from "nsh";
import { isLight, paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { fadeTransition } from "./motion";

// Confirm dialogs — chrome::open_confirm / open_form_dialog parity
// (chrome/dialog.rs). One shared surface: the card sits in the middle of the
// window on both axes (open_centered parity, the same recipe as the import
// modal), enters with a FadeSlide (opacity + a short settle from just above
// rest), and sits on the shared prompt veil — a translucent wash that keeps
// the page visible (theme.rs prompt_veil: black @ 0.28 dark / 0.18 light).
// Kit dialogs read the same color from ThemeColor.overlay.
//
// Custom self-buttoned surfaces (import modal, share sheet, reader) measure
// themselves and center the same way — `openCentered` lands with the page
// waves that need it.

export interface ConfirmRequest {
  title: string;
  /** Muted body copy below the title. */
  body?: string;
  /** Caller-built fields render inside the card (open_form_dialog flavor). */
  fields?: () => JSX.Element;
  okText: string;
  /** Irreversible commits paint the danger fill. */
  danger?: boolean;
  /** Return false to keep the dialog open (validation failed). */
  onOk: () => boolean | void;
  /** Identity for a late imperative close (dialogs.rs close_if_current):
   * an async success closes the card by token once its job lands; a token
   * that already left the stack (user dismissed it) no-ops, and a newer
   * dialog stacked on top never closes with it. */
  token?: string;
}

interface ConfirmEntry extends ConfirmRequest {
  seq: number;
}

const entries = signal<ConfirmEntry[]>([]);
let nextSeq = 1;

/** Open the shared confirm card (chrome::open_confirm). */
export function openConfirm(request: ConfirmRequest): void {
  entries.set([...untracked(entries), { ...request, seq: nextSeq++ }]);
}

function close(seq: number): void {
  const list = untracked(entries);
  const next = list.filter((e) => e.seq !== seq);
  if (next.length !== list.length) entries.set(next);
}

/** Close the card opened with this token, if it is still open. */
export function closeConfirm(token: string): void {
  const list = untracked(entries);
  const next = list.filter((e) => e.token !== token);
  if (next.length !== list.length) entries.set(next);
}

/** Uninstall confirm shared by My Skills and Marketplace (dialog.rs:208). */
export function openUninstallConfirm(title: string, onOk: () => boolean | void): void {
  openConfirm({
    title,
    body: t("uninstallDialog.description"),
    okText: t("uninstallDialog.confirmUninstall"),
    danger: true,
    onOk,
  });
}

/** `卸载 N 个技能？` (dialog.rs:228). */
export function uninstallTitleCount(count: number): string {
  return t(count === 1 ? "uninstallDialog.title_one" : "uninstallDialog.title_other").replace(
    "{{count}}",
    String(count)
  );
}

/** `卸载「name」？` (dialog.rs:240). */
export function uninstallTitleNamed(name: string): string {
  return t("uninstallDialog.titleNamed").replace("{{name}}", name);
}

/** The veil behind every prompt (theme.rs prompt_veil). */
export function promptVeil(): string {
  return isLight() ? "rgba(0,0,0,0.18)" : "rgba(0,0,0,0.28)";
}

// The top confirm entry, as its own signal: an `effect` (the one reactive
// context outside components) mirrors the entries list into it, and the
// host below reads it only through JSX expressions. This matters because
// nsh components run ONCE and do not track what their bodies read — a
// body-level `const top = entries(); if (!top) return <View />;` freezes
// on the first (empty) paint and no confirm card ever appears (the bug
// this host shipped with). NoticeBand works the same way: its entries
// consumption lives in JSX children, never in a body const.
const topEntry = signal<ConfirmEntry | null>(null);
effect(() => {
  const list = entries();
  topEntry.set(list.length > 0 ? list[list.length - 1] : null);
});

/**
 * Mount once at the shell root, above the pages. Dialogs and sheets notify
 * the window layer in GPUI; here the entries signal is that layer — pages
 * and the shell below never re-render on dialog churn.
 */
export function DialogHost(): JSX.Element {
  return (
    <If when={topEntry()}>{(entry) => <ConfirmCard entry={entry()} />}</If>
  );
}

function ConfirmCard(props: { entry: ConfirmEntry }): JSX.Element {
  const runOk = () => {
    const entry = props.entry;
    // A false return keeps the dialog open (dialog.rs on_ok contract).
    if (entry.onOk() === false) return;
    close(entry.seq);
  };
  const commitFill = () => (props.entry.danger ? paletteToken("danger") : paletteToken("accent"));
  const commitHover = () =>
    props.entry.danger ? paletteToken("danger_hover") : paletteToken("accent_hover");
  return (
    <Overlay
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        zIndex: 60,
      }}
      onCancel={() => close(props.entry.seq)}
      onPress={() => close(props.entry.seq)}
    >
      {/* Veil: page stays visible through it. */}
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
      />
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          display: "flex",
          // Column default: alignItems centers the card across the window and
          // justifyContent centers it between the top and bottom edges, so the
          // card lands on the exact center of the screen (open_centered
          // parity). A row here would center only horizontally.
          justifyContent: "center",
          alignItems: "center",
          paddingHorizontal: 20,
        }}
      >
        {/* FadeSlide entrance: opacity + settle from just above rest. */}
        <View
          testID="dialog-confirm-card"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 16,
            width: 420,
            maxWidth: 520,
            padding: 20,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("panel"),
            boxShadow: SHEET_SHADOW,
            transform: "translateY(0)",
            ...fadeTransition(true),
          }}
          onPress={() => {
            // The card swallows the press so the veil does not close it: a
            // press reaches the nearest view that hears presses.
          }}
        >
          <Text style={{ fontSize: 16, color: paletteToken("fg") }}>{props.entry.title}</Text>
          {props.entry.body ? (
            <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg_muted") }}>
              {props.entry.body}
            </Text>
          ) : null}
          {props.entry.fields ? props.entry.fields() : null}
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              justifyContent: "center",
              gap: 8,
              paddingTop: 4,
            }}
          >
            <DialogButton
              testID="dialog-confirm-cancel"
              label={t("common.cancel")}
              onPress={() => close(props.entry.seq)}
            />
            <DialogButton
              testID="dialog-confirm-ok"
              label={props.entry.okText}
              fill={commitFill()}
              hoverFill={commitHover()}
              onPress={runOk}
            />
          </View>
        </View>
      </View>
    </Overlay>
  );
}

function DialogButton(props: {
  testID?: string;
  label: string;
  fill?: string;
  hoverFill?: string;
  onPress: () => void;
}): JSX.Element {
  const filled = () => props.fill !== undefined;
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        height: 28,
        minWidth: 88,
        alignItems: "center",
        justifyContent: "center",
        paddingHorizontal: 16,
        borderRadius: 6,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: filled() ? "transparent" : paletteToken("border"),
        backgroundColor: props.fill ?? "transparent",
      }}
      hoverStyle={{ backgroundColor: props.hoverFill ?? paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          color: filled() ? paletteToken("on_accent") : paletteToken("fg"),
          fontWeight: "500",
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

// The kit Dialog's resting elevation (chrome/sheet.rs sheet_shadow parity):
// two soft black layers. Sheet surfaces repeat the same constant so the
// drawer and the dialogs keep one elevation.
export const SHEET_SHADOW =
  "0px 20px 25px -5px rgba(0,0,0,0.10), 0px 8px 10px -6px rgba(0,0,0,0.10)";
