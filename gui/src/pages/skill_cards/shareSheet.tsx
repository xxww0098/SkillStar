import { Scroll, Text, View, signal, type JSX } from "nsh";
import { Clipboard } from "nsh";
import { dirname } from "path";
import { SkillCardsShare } from "skillstar-gui/native";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { SHEET_SHADOW } from "../../chrome/dialog";
import { fadeTransition, mixHex, motionEnabled } from "../../chrome/motion";
import { isLight, paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { toast } from "../../shell/events";

// DeckShareSheet — skill_cards/share_sheet.rs parity. The share button on a
// deck opens this centered dialog with two ways out:
//
//   1. a 7-day `agd-` share code built from the deck's remote sources
//      (deck `skillSources` first, the hub's git remotes as fallback — the
//      lookup runs behind the bridge, off the UI thread), copied to the
//      clipboard with a success notice;
//   2. a `.agd` bundle export (remote members ship their source link, local
//      members their content) that lands in the download directory, shows
//      its path and can reveal the folder in the file manager.
//
// Local-only members cannot ride a code: they are counted in the warning
// line beside the copy button and are the bundle's job. The GPUI sheet
// parks in a kit dialog measured to the window's center (open_centered);
// here a flex-centered card on the shared prompt veil does the same, with
// the FadeSlide entrance the kit gives it (opacity + a short settle from
// just above rest; Reduce Motion lands it directly).
//
// nsh's components run once — every state read below sits in a JSX
// attribute or child expression (the reactive spots), never in a
// component body; the mount/dismount of the sheet rides `<If>`.

/** The deck fields the sheet needs (ss_skills SkillGroup's serde shape —
 * structurally the page's DeckRecord, so callers pass theirs straight).
 * `skillSources` rides SkillGroup's `#[serde(default)]`: a deck written
 * without it (or a caller's narrower record) reads as "no deck-level
 * sources", and the hub fallback decides alone. */
export interface ShareDeck {
  name: string;
  description: string;
  icon: string;
  skills: string[];
  skillSources?: Record<string, string>;
}

/** What `SkillCardsShare.buildDeckShareCode` resolves with. */
interface ShareCodeOutcome {
  code: string;
  remoteCount: number;
  localCount: number;
}

interface OpenSheet {
  deck: ShareDeck;
  /** Overrides the bundle's directory only — the headless tests'
   * injection point so a test never writes a real $HOME; the product
   * path (unset) stays the download directory, like the GPUI sheet. */
  exportOutDir?: string;
}

// ---- Sheet state (module signals; the host below renders them) ----------

const open = signal<OpenSheet | null>(null);
const code = signal<string | null>(null);
const codeRemote = signal(0);
const codeLocal = signal(0);
const exporting = signal(false);
const exportPath = signal<string | null>(null);
/** The enter animation's "from" state: one displaced frame, then rest. */
const entered = signal(false);

// The generation epoch: a slow lookup for a closed sheet must not land its
// code into the next sheet (the GPUI entity dropped with its dialog; here
// the epoch is the drop).
let epoch = 0;

/** Open the deck share sheet (crate::open_share_sheet parity). */
export function openShareSheet(deck: ShareDeck, options?: { exportOutDir?: string }): void {
  epoch += 1;
  // From-state before the sheet mounts, so the entrance has a frame to leave.
  entered.set(false);
  open.set({ deck, exportOutDir: options?.exportOutDir });
  code.set(null);
  codeRemote.set(0);
  codeLocal.set(0);
  exporting.set(false);
  exportPath.set(null);
  generateCode(deck, epoch);
}

/** Dismiss the sheet (a press on the veil). */
export function closeShareSheet(): void {
  epoch += 1;
  open.set(null);
}

async function generateCode(deck: ShareDeck, ticket: number): Promise<void> {
  try {
    const outcome = (await SkillCardsShare.buildDeckShareCode(
      deck.name,
      deck.description,
      deck.icon,
      deck.skills,
      deck.skillSources ?? {}
    )) as ShareCodeOutcome;
    if (ticket !== epoch) return;
    code.set(outcome.code);
    codeRemote.set(outcome.remoteCount);
    codeLocal.set(outcome.localCount);
  } catch (error) {
    // The hub lookup fails soft on the GPUI side (unwrap_or_default) and
    // the encoder cannot fail; this is the never-path, surfaced anyway.
    if (ticket !== epoch) return;
    toast(String(error), "error");
  }
}

async function copyCode(): Promise<void> {
  const text = code();
  if (!text) return; // still generating — the GPUI copy bails the same way
  await Clipboard.writeText(text);
  toast(t("shareResultCard.copied"), "success");
}

async function runExport(): Promise<void> {
  if (exporting()) return; // loading swallows the press
  const sheet = open();
  if (!sheet) return;
  exporting.set(true);
  try {
    const path = await SkillCardsShare.exportDeckBundle(
      sheet.deck.skills,
      sheet.deck.name,
      sheet.exportOutDir
    );
    exportPath.set(path);
  } catch (error) {
    toast(String(error), "error");
  } finally {
    exporting.set(false);
  }
}

/** The open-folder button: os_open::open_folder parity — hand the parent
 * directory to the platform file manager; failures stay silent, as the
 * GPUI side's `let _ =` spawn. */
async function revealFolder(path: string): Promise<void> {
  const folder = dirname(path);
  if (!folder || folder === path) return;
  try {
    await SkillCardsShare.revealInFileManager(folder);
  } catch {
    // silent, as above
  }
}

// ---- Rendering ------------------------------------------------------------

/**
 * The sheet layer. Mount once per host surface (the deck page, and the
 * headless test harness); idle renders nothing so the page never carries an
 * overlay.
 */
export function ShareSheetHost(): JSX.Element {
  return <If when={open()}>{() => <DeckShareSheet />}</If>;
}

function DeckShareSheet(): JSX.Element {
  if (!entered()) {
    // One displaced frame, then the FadeSlide rest (the notice rows enter
    // the same way): the transition needs a "from" to animate from.
    // Reduce Motion still fades opacity, so it takes the same frame.
    requestAnimationFrame(() => entered.set(true));
  }
  const settled = () => entered();
  return (
    // The veil layer: centered card on the shared prompt wash; a press on
    // the wash dismisses (the kit dialog's outside-press).
    <View
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 60,
      }}
      testID="share-veil"
      onPress={() => closeShareSheet()}
    >
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: isLight() ? "rgba(0,0,0,0.18)" : "rgba(0,0,0,0.28)",
        }}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          width: 460,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          // open_share_sheet's surface: card in light, panel in dark. The
          // card sizes to its content (the kit's measured height): both
          // sections are bounded — the code well is a fixed 96px viewport
          // and the path line truncates — so no clamp is needed, and one
          // would clip the bundle section away on short viewports.
          backgroundColor: paletteToken(isLight() ? "card" : "panel"),
          boxShadow: SHEET_SHADOW,
          transform:
            motionEnabled() && !settled() ? "translateY(-8px)" : "translateY(0)",
          opacity: settled() ? 1 : 0,
          ...fadeTransition(true),
        }}
        onPress={() => {
          // Swallow the press so the veil does not close the sheet.
        }}
      >
        <Header deckName={open()!.deck.name} />
        <View
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 16,
            paddingHorizontal: 24,
            paddingTop: 16,
            paddingBottom: 16,
          }}
        >
          <CodeSection />
          <View style={{ height: 1, backgroundColor: paletteToken("border_soft") }} />
          <BundleSection />
        </View>
      </View>
    </View>
  );
}

/** `px-6 pt-4 pb-3` + border-b, same header chrome as the deck editor. */
function Header(props: { deckName: string }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        width: "100%",
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 12,
        flexShrink: 0,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
      }}
    >
      <Icon name="share" size={16} color={paletteToken("accent")} />
      <Text
        style={{
          fontSize: 16,
          fontWeight: "600",
          color: paletteToken("fg"),
        }}
      >
        {t("skillCards.shareDeck")}
      </Text>
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 14,
          color: paletteToken("fg_muted"),
          overflow: "hidden",
        }}
      >
        {props.deckName}
      </Text>
    </View>
  );
}

function SectionTitle(props: {
  icon: Parameters<typeof Icon>[0]["name"];
  label: string;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
      <Icon name={props.icon} size={16} color={paletteToken("accent")} />
      <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

function CodeSection(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <SectionTitle icon="qrCode" label={t("skillCards.shareCodeSection")} />
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
        {`${t("shareResultCard.pasteToImportHint")} · ${tf("skillCards.shareCodeCount", {
          count: codeRemote(),
        })}`}
      </Text>
      {/* The code well: a fixed 96px viewport (the GPUI max_h) that scrolls
          inside — an explicit pixel frame, never a max-h scroll wrapper. */}
      <View
        style={{
          width: "100%",
          height: 96,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("well"),
          overflow: "hidden",
        }}
      >
        <Scroll
          testID="share-code"
          style={{ width: "100%", height: 96, padding: 8 }}
          contentStyle={{ display: "flex", flexDirection: "column" }}
        >
          <Text
            style={{
              fontSize: 12,
              lineHeight: 16,
              color: paletteToken("fg_muted"),
            }}
          >
            {code() ?? t("skillCards.shareCodeGenerating")}
          </Text>
        </Scroll>
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <PrimaryButton testID="share-copy" label={t("common.copy")} onPress={() => void copyCode()} />
        <If when={codeLocal() > 0}>
          <Text style={{ fontSize: 12, color: paletteToken("warn") }}>
            {tf("skillCards.shareCodeExcluded", { count: codeLocal() })}
          </Text>
        </If>
      </View>
    </View>
  );
}

function BundleSection(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <SectionTitle icon="packageOpen" label={t("skillCards.shareBundleSection")} />
      <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg_muted") }}>
        {t("skillCards.shareBundleHint")}
      </Text>
      <PrimaryButton
        testID="share-export"
        label={t("skillCards.shareBundleExport")}
        onPress={() => void runExport()}
        leading={
          <View style={{ width: 16, height: 16, flexShrink: 0 }}>
            <IconSpin
              name="loader"
              size={16}
              color={paletteToken("on_accent")}
              spin={exporting()}
            />
          </View>
        }
      />
      <If when={exportPath()}>
        {(path) => (
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              minWidth: 0,
            }}
          >
            <Text
              testID="share-export-path"
              style={{
                flex: 1,
                minWidth: 0,
                fontSize: 12,
                color: paletteToken("fg_muted"),
                overflow: "hidden",
              }}
            >
              {path()}
            </Text>
            <If when={dirname(path()).length > 0 && dirname(path()) !== path()}>
              <GhostButton
                testID="share-open-folder"
                label={t("skillCards.shareBundleOpenFolder")}
                onPress={() => void revealFolder(path())}
              />
            </If>
          </View>
        )}
      </If>
    </View>
  );
}

/** `chrome::primary_button` parity: h-28, px-20, accent fill, hover swaps to
 *  accent_hover in zero frames (no color transition — channel 1). */
function PrimaryButton(props: {
  testID: string;
  label: string;
  onPress: () => void;
  leading?: JSX.Element;
}): JSX.Element {
  const accent = paletteToken("accent");
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        height: 28,
        paddingHorizontal: 20,
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: accent,
      }}
      hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
      activeStyle={{ backgroundColor: mixHex(accent, paletteToken("on_accent"), 0.12) }}
      onPress={props.onPress}
    >
      {props.leading}
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** `chrome::ghost_button` parity: h-28, px-12, transparent, hover wash. */
function GhostButton(props: { testID: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        height: 28,
        paddingHorizontal: 12,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      activeStyle={{ backgroundColor: mixHex(paletteToken("panel_hover"), paletteToken("fg"), 0.12) }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}
