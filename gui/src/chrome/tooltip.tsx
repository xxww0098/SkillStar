import { Text, View, type JSX, type ViewStyle } from "nsh";

// Hover tip — chrome::tooltip parity (chrome/mod.rs:45-58). Black surface,
// white text, in BOTH modes; menus, popovers and search suggestions keep the
// card color instead. The nsh `tooltip` prop on any View takes this bubble.
//
// Usage: wrap the trigger in <View tooltip={<Tip text={...} />}>.
export function Tip(props: { text: string }): JSX.Element {
  return (
    <View style={TIP_STYLE}>
      <Text style={{ fontSize: 12, color: TIP_FG }}>{props.text}</Text>
    </View>
  );
}

// The tip is mode-invariant by contract (tooltip hard rule): constants, not
// palette lookups, so a theme flip cannot repaint an open tip.
const TIP_FG = "#ffffff";
const TIP_STYLE: ViewStyle = {
  backgroundColor: "#000000",
  paddingHorizontal: 8,
  paddingVertical: 4,
  borderRadius: 6,
  borderWidth: 1,
  borderColor: "rgba(255,255,255,0.16)",
};
