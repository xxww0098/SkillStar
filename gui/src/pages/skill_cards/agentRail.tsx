import { Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { Icon } from "../../chrome/icons";
export {
  AgentBadge,
  AgentFooterBar,
  AgentIcon,
  AgentRail,
  type AgentRailSlot,
} from "../../chrome/agentRail";

// Shared footer & SVG agent rail carousel primitives come from chrome/agentRail.
// This module keeps only the deck-specific member-count badge.

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
        gap: 4,
        flexShrink: 0,
        paddingHorizontal: 8,
        paddingVertical: 2,
        borderRadius: 6,
        backgroundColor: paletteToken("card_hover"),
      }}
    >
      <Icon name="layers" size={12} color={paletteToken("accent")} />
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("accent_fg") }}>
        {String(props.count)}
      </Text>
    </View>
  );
}
