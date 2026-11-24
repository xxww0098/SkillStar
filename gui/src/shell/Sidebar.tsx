import { Image, Text, View, effect, openUrl, signal, untracked, type JSX } from "nsh";
import { Settings } from "skillstar-gui/native";
import {
  isLight,
  paletteToken,
  setThemeMode,
  type ThemeMode,
} from "../services/themeMode";
import { t } from "../services/translation";
import { RAIL_COLLAPSED_W, RAIL_W } from "../layout";
import { Icon, type IconName } from "../chrome/icons";
import { Tip } from "../chrome/tooltip";
import { SliderSegmented } from "../chrome/segmented";
import { trafficLightLane } from "./caption";
import { requestThemeMode } from "./events";
import {
  collapsed,
  currentPage,
  railMode,
  setCurrentPage,
  setRailMode,
} from "./nav";
import { SkillsNav } from "./skillsNav";
import { SidebarGitHubEntry } from "./SidebarGitHubEntry";
import { AccountsProviderMenu } from "../pages/accounts/rail";

// The sidebar rail — shell/sidebar.rs parity.
//
// The rail lives in its own absolutely positioned lane on the shell canvas
// (the Shell mounts it); this view fills that lane. Geometry constants are
// layout.ts's RAIL_W / RAIL_COLLAPSED_W. Structure top to bottom:
//   - the traffic-light clearance lane (a pure drag strip),
//   - the brand row (the 32px mark opens the repository: pointer cursor,
//     hover tip, and it opts OUT of the drag region around it),
//   - the Skills/Accounts mode capsule (the shared sliding thumb),
//   - the mode's list: skills pages, or the accounts provider menu
//     (pages/accounts/rail.tsx, since the accounts wave),
//   - the footer: expanded = collapse | theme | settings, collapsed stacks
//     settings, theme, collapse. Accounts quotas stay reachable from the tray
//     window and the app menu (⇧⌘U), not from this rail.

/** Public source repository (sidebar.rs REPOSITORY_URL). */
const REPOSITORY_URL = "https://github.com/xxww0098/SkillStar";
const BRAND_ICON_URI = Settings.brandIconDataUri();

/** Inset around the mode switcher (sidebar.rs MODE_SWITCHER_INSET_PX). */
const MODE_SWITCHER_INSET = 12;
const MODE_SWITCHER_INSET_COLLAPSED = 6;

export function Sidebar(): JSX.Element {
  const accountsRailVisited = signal(untracked(railMode) === "accounts");
  effect(() => {
    if (railMode() === "accounts" && !untracked(accountsRailVisited)) {
      accountsRailVisited.set(true);
    }
  });
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: collapsed() ? RAIL_COLLAPSED_W : RAIL_W,
        height: "100%",
        gap: 2,
        paddingBottom: 8,
        overflow: "hidden",
      }}
    >
      {/* Traffic-light clearance + brand row: one continuous drag region. */}
      <View
        testID="sidebar-top-drag"
        style={{ display: "flex", flexDirection: "column", gap: 2, flexShrink: 0 }}
        desktop={{ windowDrag: true }}
      >
        <View style={{ width: "100%", height: trafficLightLane(), flexShrink: 0 }} />
        <BrandRow collapsed={collapsed()} />
      </View>

      <ModeSwitcher collapsed={collapsed()} />

      {/* Skills mode lists pages; accounts mode lists providers in this same
          slot (the accounts wave's provider menu — pages/accounts/rail.tsx,
          AccountsPage::render_provider_menu parity). Keep both subtrees
          mounted so toggling between Skills and Accounts never destroys or
          re-decodes the navigation/agent SVG rows mid-animation. */}
      <View
        style={{
          position: "relative",
          flex: 1,
          minHeight: 0,
          overflow: "hidden",
        }}
      >
        <View
          style={{
            position: "absolute",
            top: 8,
            bottom: 8,
            left: collapsed() ? 6 : 8,
            right: collapsed() ? 6 : 8,
            display: "flex",
            flexDirection: "column",
            gap: 2,
            opacity: railMode() === "skills" ? 1 : 0,
            pointerEvents: railMode() === "skills" ? "auto" : "none",
            zIndex: railMode() === "skills" ? 1 : 0,
          }}
        >
          <SkillsNav />
        </View>
        <View
          style={{
            position: "absolute",
            top: 8,
            bottom: 8,
            left: collapsed() ? 6 : 8,
            right: collapsed() ? 6 : 8,
            display: "flex",
            flexDirection: "column",
            gap: 2,
            opacity: railMode() === "accounts" ? 1 : 0,
            pointerEvents: railMode() === "accounts" ? "auto" : "none",
            zIndex: railMode() === "accounts" ? 1 : 0,
          }}
        >
          <If when={accountsRailVisited()}>
            <AccountsProviderMenu collapsed={railMode() === "accounts" ? collapsed() : false} />
          </If>
        </View>
      </View>

      <SidebarGitHubEntry collapsed={collapsed()} />
      <SidebarFooter collapsed={collapsed()} />
    </View>
  );
}

function BrandMark(): JSX.Element {
  const hovered = signal(false);
  return (
    <View
      testID="sidebar-brand-mark"
      tooltip={hovered() ? <Tip text={t("sidebar.openRepository")} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
      style={{
        width: 32,
        height: 32,
        flexShrink: 0,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        overflow: "hidden",
        alignItems: "center",
        justifyContent: "center",
        display: "flex",
        cursor: "pointer",
      }}
      desktop={{ windowDrag: false }}
      onPress={() => {
        void openUrl(REPOSITORY_URL);
      }}
    >
      <Image
        testID="sidebar-brand-icon"
        src={BRAND_ICON_URI}
        fadeDuration={0}
        style={{ width: 40, height: 40, flexShrink: 0, objectFit: "cover" }}
      />
    </View>
  );
}

function BrandRow(props: { collapsed: boolean }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: props.collapsed ? 0 : 10,
        paddingHorizontal: props.collapsed ? 0 : 12,
        paddingBottom: 10,
      }}
    >
      <BrandMark />
      <View
        style={{
          display: props.collapsed ? "none" : "flex",
          flexDirection: "column",
        }}
      >
        <Text style={{ fontSize: 14, fontWeight: "700", color: paletteToken("fg") }}>SkillStar</Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{t("sidebar.tagline")}</Text>
      </View>
    </View>
  );
}

function ModeSwitcher(props: { collapsed: boolean }): JSX.Element {
  const inset = () => (props.collapsed ? MODE_SWITCHER_INSET_COLLAPSED : MODE_SWITCHER_INSET);
  const modeGeo = () => {
    const isCollapsed = props.collapsed;
    const padInset = isCollapsed ? MODE_SWITCHER_INSET_COLLAPSED : MODE_SWITCHER_INSET;
    const railW = isCollapsed ? RAIL_COLLAPSED_W : RAIL_W;
    const innerW = railW - 2 * padInset - 2 * (4 + 1);
    return {
      slotW: isCollapsed ? innerW : (innerW - 2) / 2,
      slotH: 32,
      pad: 4,
      iconSize: isCollapsed ? 16 : 14,
    };
  };
  return (
    <View style={{ paddingHorizontal: inset(), display: "flex" }} desktop={{ windowDrag: true }}>
      <SliderSegmented
        motionId="mode-switch-motion"
        geo={modeGeo}
        vertical={() => props.collapsed}
        options={[
          {
            id: "mode-skills",
            icon: "layoutGrid",
            label: () => (props.collapsed ? "" : t("sidebar.modeSkills")),
          },
          {
            id: "mode-accounts",
            icon: "users",
            label: () => (props.collapsed ? "" : t("sidebar.modeAccounts")),
          },
        ]}
        selected={() => (railMode() === "accounts" ? 1 : 0)}
        onClick={(ix) => setRailMode(ix === 1 ? "accounts" : "skills")}
      />
    </View>
  );
}

/** Footer row — expanded: collapse | theme | settings; collapsed stacks
 * settings, theme, collapse (`column-reverse` keeps one button set mounted). */
function SidebarFooter(props: { collapsed: boolean }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: props.collapsed ? "column-reverse" : "row",
        alignItems: "center",
        justifyContent: props.collapsed ? "flex-start" : "space-between",
        gap: props.collapsed ? 2 : 4,
        paddingHorizontal: 4,
        paddingVertical: 8,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border"),
      }}
    >
      <FooterButton
        id="collapse"
        icon={props.collapsed ? "panelLeftOpen" : "panelLeftClose"}
        label={props.collapsed ? t("sidebar.expand") : t("sidebar.collapse")}
        active={false}
        onClick={() => collapsed.update((prev) => !prev)}
      />
      <FooterButton
        id="theme"
        icon={isLight() ? "sun" : "moon"}
        label={t("settings.backgroundStyle")}
        active={false}
        onClick={() => {
          const next: ThemeMode = isLight() ? "current" : "paper";
          setThemeMode(next);
          requestThemeMode.publish(next);
        }}
      />
      <FooterButton
        id="settings"
        icon="settings"
        label={t("sidebar.settings")}
        active={currentPage() === "settings"}
        onClick={() => setCurrentPage("settings")}
      />
    </View>
  );
}

/** 28×28 icon button (footer_btn parity): zero-frame hover fill, press
 * darkens toward the ink; active state paints panel_active + accent ink. */
function FooterButton(props: {
  id: string;
  icon: IconName;
  label: string;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  const hovered = signal(false);
  return (
    <View
      testID={`sidebar-${props.id}`}
      tooltip={hovered() ? <Tip text={props.label} /> : undefined}
      onMouseEnter={() => hovered.set(true)}
      onMouseLeave={() => hovered.set(false)}
      role="button"
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 28,
        height: 28,
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("panel_active") : "transparent",
      }}
      hoverStyle={{
        backgroundColor: props.active
          ? paletteToken("panel_active")
          : paletteToken("panel_hover"),
      }}
      focusVisibleStyle={{
        outlineWidth: 2,
        outlineColor: paletteToken("accent"),
        outlineOffset: 1,
      }}
      onPress={props.onClick}
    >
      <Icon
        name={props.icon}
        size={15}
        color={props.active ? paletteToken("accent_fg") : paletteToken("fg_muted")}
      />
    </View>
  );
}
