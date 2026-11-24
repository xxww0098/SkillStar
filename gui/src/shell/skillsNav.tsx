import { Text, View, type JSX } from "nsh";
import { mixHsl, paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { motionTransition } from "../chrome/motion";
import { NAV_GAP, NAV_ROW_H, NAV_STRIDE } from "../layout";
import { Icon } from "../chrome/icons";
import {
  SKILLS_NAV,
  collapsed,
  currentPage,
  navBoxShown,
  navSlot,
  railPage,
  setCurrentPage,
  type NavItem,
} from "./nav";

// Skills-mode destinations — shell/nav.rs parity.
//
// ONE selection box is shared by the four rows: it is absolutely positioned
// and glides to the active row (on-screen move, --ease-in-out,
// --duration-sm), and the rows themselves stay transparent, so the box can
// travel without each item painting its own highlight. Opening settings
// fades the box out in place (nothing selected); publisher detail keeps the
// box on the market row. Reduce Motion drops the travel — the box lands on
// its end state. The opacity change is not transitioned.
//
// Row emphasis ink crossfades by the same slot position (nav_emphasis):
// rows near the box wear accent ink, far rows stay muted.

export function SkillsNav(): JSX.Element {
  return (
    <View style={{ position: "relative", width: "100%" }}>
      {/* The shared selection box (paint_nav_thumb): panel_active fill,
          accent hairline, travels by transform only. */}
      <View
        testID="skills-nav-box"
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          width: "100%",
          height: NAV_ROW_H,
          borderRadius: 6,
          backgroundColor: paletteToken("panel_active"),
          borderWidth: 1,
          borderColor: paletteToken("accent"),
          opacity: navBoxShown(currentPage()) ? 1 : 0,
          transform: `translateY(${navSlot() * NAV_STRIDE}px)`,
          ...motionTransition(),
        }}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          width: "100%",
          gap: NAV_GAP,
        }}
      >
        {SKILLS_NAV.map((item, index) => (
          <SkillsNavRow item={item} index={index} />
        ))}
      </View>
    </View>
  );
}

/** How strongly row `index` wears the selected ink (shell/nav.rs nav_emphasis). */
export function navEmphasis(index: number, slot: number, opacity: number): number {
  const near = Math.min(1, Math.max(0, 1 - Math.abs(slot - index)));
  return near * Math.min(1, Math.max(0, opacity));
}

function SkillsNavRow(props: {
  item: NavItem;
  index: number;
}): JSX.Element {
  const selected = () => props.item.id === railPage(currentPage());
  const emphasis = () =>
    navEmphasis(props.index, navSlot(), navBoxShown(currentPage()) ? 1 : 0);
  const ink = () => mixHsl(paletteToken("fg_muted"), paletteToken("accent_fg"), emphasis());
  return (
    <View
      testID={props.item.domId}
      role="button"
      style={{
        position: "relative",
        width: "100%",
        height: NAV_ROW_H,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        borderRadius: 6,
        cursor: "pointer",
        justifyContent: collapsed() ? "center" : "flex-start",
        gap: collapsed() ? 0 : 10,
        paddingHorizontal: collapsed() ? 0 : 10,
      }}
      // Hover fill only on unselected rows — the traveling box owns the
      // selected fill (skills_nav_row). Zero-frame color channel.
      hoverStyle={
        selected()
          ? undefined
          : {
              backgroundColor: paletteToken("panel_hover"),
            }
      }
      onPress={() => setCurrentPage(props.item.id)}
    >
      <Icon name={props.item.icon} size={collapsed() ? 18 : 16} color={ink()} />
      <Text
        style={{
          display: collapsed() ? "none" : "flex",
          fontSize: 14,
          color: ink(),
          fontWeight: selected() ? "600" : "400",
        }}
      >
        {t(props.item.labelKey)}
      </Text>
    </View>
  );
}
