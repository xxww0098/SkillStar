import { Text, View, effect, mount, signal, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { groupsChanged } from "../../shell/events";
import { DeckImportHost, openDeckImport } from "./deckImport";

// Test-only entry for the deck-import slice (`nsh test` opens it instead of
// App.tsx): the deck page that owns the toolbar button is another slice's
// file, so this harness mounts the one thing the dialog needs from its
// page — the trigger and the host — plus a marker that flips when the
// dialog asks the deck page to reload (the groupsChanged contract).

const refreshes = signal(0);

effect(() =>
  groupsChanged.subscribe(() => refreshes.update((count) => count + 1))
);

function Harness(): JSX.Element {
  return (
    <View
      testID="deck-import-harness-root"
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        display: "flex",
        padding: 24,
      }}
    >
      <View
        testID="deck-import-harness-open"
        style={{
          display: "flex",
          height: 32,
          paddingHorizontal: 12,
          alignItems: "center",
          justifyContent: "center",
          borderRadius: 6,
          cursor: "pointer",
          backgroundColor: paletteToken("accent"),
        }}
        onPress={() => openDeckImport()}
      >
        <Text style={{ fontSize: 12, color: paletteToken("on_accent") }}>
          open deck import
        </Text>
      </View>
      <If when={refreshes() > 0}>
        <Text testID="deck-import-harness-refreshed" style={{ fontSize: 12 }}>
          refreshes: {refreshes()}
        </Text>
      </If>
      <DeckImportHost />
    </View>
  );
}

mount(Harness);
