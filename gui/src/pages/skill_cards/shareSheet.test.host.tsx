import { View, mount, type JSX } from "nsh";
import { ShareSheetHost, openShareSheet } from "./shareSheet";
import { NoticeBand } from "../../shell/notify";
import { DialogHost } from "../../chrome/dialog";
import { paletteToken } from "../../services/themeMode";

// Test host for the deck-share slice (the deck page mounts the same host
// when its wave wires the toolbar; until then this entry is what the
// headless tests open). It pins a deck whose two members have no remote
// source anywhere (empty deck sources, and no hub carries these names),
// so the sheet's data flow is decided by the deck payload alone:
// 0 remote, 2 local — the warning line must show. The bundle export is
// pointed at a temp directory through the sheet's test injection so a
// test never writes a real $HOME.
//
// Named *.test.host.tsx (not *.test.tsx): `nsh test` collects the latter
// as test files; this is an app entry (createGroup.test.host.tsx shape).

const TEST_DECK = {
  name: "测试卡组 Deck",
  description: "share-sheet harness",
  icon: "sparkle",
  skills: ["nsh-harness-alpha", "nsh-harness-beta"],
  skillSources: {} as Record<string, string>,
};

/** Where the harness directs the bundle export (the test cleans it up). */
export const HARNESS_OUT_DIR = "/tmp/skillstar-nsh-share-sheet-host";

function ShareSheetHarness(): JSX.Element {
  return (
    // A sized, positioned root: the sheet's veil layer is absolute against
    // it, as against the shell's main panel in the product mount.
    <View
      style={{
        position: "relative",
        width: 900,
        height: 600,
        backgroundColor: paletteToken("bg"),
      }}
    >
      <View
        testID="harness-open"
        style={{ width: 32, height: 16 }}
        onPress={() => openShareSheet(TEST_DECK, { exportOutDir: HARNESS_OUT_DIR })}
      />
      <ShareSheetHost />
      <NoticeBand />
      <DialogHost />
    </View>
  );
}

mount(ShareSheetHarness);
