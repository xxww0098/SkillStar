import { Image, Text, View, openUrl, type JSX } from "nsh";
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
//     settings, theme, collapse.
//
// The GPUI rail is a replayed view kept out of page animation frames
// (sidebar.rs doc comment); the nsh DOM keeps that property natively — the
// rail subtree only re-renders when its own signals change.

/** Public source repository (sidebar.rs REPOSITORY_URL). */
const REPOSITORY_URL = "https://github.com/xxww0098/SkillStar";
const BRAND_ICON_URI = Settings.brandIconDataUri();

/** Inset around the mode switcher (sidebar.rs MODE_SWITCHER_INSET_PX). */
const MODE_SWITCHER_INSET = 12;
const MODE_SWITCHER_INSET_COLLAPSED = 6;

export function Sidebar(): JSX.Element {
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
          AccountsPage::render_provider_menu parity). */}
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          flex: 1,
          minHeight: 0,
          gap: 2,
          paddingVertical: 8,
          paddingHorizontal: collapsed() ? 6 : 8,
          overflow: "hidden",
        }}
      >
        <If
          when={railMode() === "skills"}
          else={<AccountsProviderMenu collapsed={collapsed()} />}
        >
          <SkillsNav />
        </If>
      </View>

      <SidebarFooter collapsed={collapsed()} />
    </View>
  );
}

function renderBrandMark(): JSX.Element {
  return (
    <View
      testID="sidebar-brand-mark"
      tooltip={<Tip text={t("sidebar.openRepository")} />}
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
      // The mark opts out of the drag region around it (occlude parity).
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

/** The 32px brand mark + wordmark row (inside sidebar-top-drag in both
 * expanded and collapsed states; the mark itself opts out and opens the repo). */
function BrandRow(props: { collapsed: boolean }): JSX.Element {
  return (
    <If
      when={props.collapsed}
      else={
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
            gap: 10,
            paddingHorizontal: 12,
            paddingBottom: 10,
          }}
        >
          {renderBrandMark()}
          <View style={{ display: "flex", flexDirection: "column" }}>
            <Text style={{ fontSize: 14, fontWeight: "700", color: paletteToken("fg") }}>SkillStar</Text>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{t("sidebar.tagline")}</Text>
          </View>
        </View>
      }
    >
      <View style={{ display: "flex", alignItems: "center", justifyContent: "center", paddingBottom: 10 }}>
        {renderBrandMark()}
      </View>
    </If>
  );
}

function renderModeSwitcher(isCollapsed: boolean): JSX.Element {
  const inset = isCollapsed ? MODE_SWITCHER_INSET_COLLAPSED : MODE_SWITCHER_INSET;
  // Slot geometry derives from the rail width minus the wrapper inset and
  // the track's own chrome (mode_switcher_track parity).
  const railW = isCollapsed ? RAIL_COLLAPSED_W : RAIL_W;
  const innerW = railW - 2 * inset - 2 * (4 + 1);
  return (
    <View style={{ paddingHorizontal: inset, display: "flex" }} desktop={{ windowDrag: true }}>
      <SliderSegmented
        motionId={isCollapsed ? "mode-switch-motion-collapsed" : "mode-switch-motion"}
        geo={{
          slotW: isCollapsed ? innerW : (innerW - 2) / 2,
          slotH: 32,
          pad: 4,
          iconSize: isCollapsed ? 16 : 14,
        }}
        vertical={isCollapsed}
        options={[
          { id: "mode-skills", icon: "layoutGrid", label: isCollapsed ? undefined : t("sidebar.modeSkills") },
          { id: "mode-accounts", icon: "users", label: isCollapsed ? undefined : t("sidebar.modeAccounts") },
        ]}
        selected={() => (railMode() === "accounts" ? 1 : 0)}
        onClick={(ix) => setRailMode(ix === 1 ? "accounts" : "skills")}
      />
    </View>
  );
}

/** Skills/Accounts capsule — the shared sliding-thumb segmented control
 * (render_mode_switcher parity). Collapsed: icon-only stacked pills. */
function ModeSwitcher(props: { collapsed: boolean }): JSX.Element {
  return (
    <If when={props.collapsed} else={renderModeSwitcher(false)}>
      {renderModeSwitcher(true)}
    </If>
  );
}

function renderCollapseBtn(isCollapsed: boolean): JSX.Element {
  return (
    <FooterButton
      id="collapse"
      icon={isCollapsed ? "panelLeftOpen" : "panelLeftClose"}
      label={isCollapsed ? t("sidebar.expand") : t("sidebar.collapse")}
      active={false}
      onClick={() => collapsed.set(!collapsed())}
    />
  );
}

function renderThemeBtn(): JSX.Element {
  return (
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
  );
}

function renderSettingsBtn(): JSX.Element {
  return (
    <FooterButton
      id="settings"
      icon="settings"
      label={t("sidebar.settings")}
      active={currentPage() === "settings"}
      onClick={() => setCurrentPage("settings")}
    />
  );
}

/** Footer row — expanded: collapse | theme | settings (justify-between);
 * collapsed stacks settings, theme, collapse (sidebar.rs footer parity). */
function SidebarFooter(props: { collapsed: boolean }): JSX.Element {
  return (
    <If
      when={props.collapsed}
      else={
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 4,
            paddingHorizontal: 4,
            paddingVertical: 8,
            borderTopWidth: 1,
            borderTopColor: paletteToken("border"),
          }}
        >
          {renderCollapseBtn(false)}
          {renderThemeBtn()}
          {renderSettingsBtn()}
        </View>
      }
    >
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 2,
          paddingHorizontal: 4,
          paddingVertical: 8,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border"),
        }}
      >
        {renderSettingsBtn()}
        {renderThemeBtn()}
        {renderCollapseBtn(true)}
      </View>
    </If>
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
  return (
    <View
      testID={`sidebar-${props.id}`}
      tooltip={<Tip text={props.label} />}
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

// The Shell decides the lane's absolute inset; the rail decides its own
// width inside it (RAIL_W / RAIL_COLLAPSED_W live in ../layout).
