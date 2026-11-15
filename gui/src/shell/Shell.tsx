import { Text, View, effect, signal, type JSX } from "nsh";
import { paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { MySkillsPage } from "../pages/my_skills";
import { MarketplacePage } from "../pages/marketplace";
// Publisher drill-down: the marketplace wave's registered placeholder page
// (src/pages/marketplace/publisher/) — the publisher slice replaces that
// directory, never this file.
import { MarketPublisherPage } from "../pages/marketplace/publisher";
import { SkillCardsPage } from "../pages/skill_cards";
import { ProjectsPage } from "../pages/projects";
import { AccountsPage } from "../pages/accounts";
import { SettingsPage } from "../pages/settings";
import { PANEL_RADIUS, RAIL_COLLAPSED_W, RAIL_W, SHELL_GAP, TOOLBAR_H } from "../layout";
import {
  collapsed as railCollapsed,
  currentPage,
  railPage,
  setCurrentPage,
  type PageId,
} from "./nav";
import {
  selectPage,
  selectPublisher,
  type PublisherRef,
} from "./events";
import { Sidebar } from "./Sidebar";
import { NoticeBand } from "./notify";
import { DialogHost } from "../chrome/dialog";

// The application shell — shell.rs parity.
//
// Canvas (palette.bg) with two floating surfaces:
//   - the rail lane, absolutely at left: gap, top 0, bottom gap (the panel
//     starts at top-2, so the lane only guards the traffic-light band);
//   - the main panel, inset gap + rail + gap on the left and gap on the
//     other sides: 16px radius, 1px border, opaque panel fill.
//
// Pages are KEEP-ALIVE: every page mounts once and switches by display, so
// page state survives navigation (GPUI's 7 live entities, shell.rs:382-424;
// the nsh DOM keeps each subtree alive natively). Pages never import each
// other — cross-page hops run over the event bus (events.ts), translated
// here exactly as Shell subscribes them in shell.rs:59-127.

/// Publisher payload for the pushed detail page. The marketplace wave
/// replaces this with the bridge-typed record.
export const currentPublisher = signal<PublisherRef | null>(null);

// Marketplace → publisher detail: neither side knows the other exists.
effect(() =>
  selectPublisher.subscribe((publisher) => {
    currentPublisher.set(publisher);
    setCurrentPage("publisher_detail");
  })
);

// Page-internal nav hops (empty-state CTAs, cross-page links).
effect(() => selectPage.subscribe((page) => setCurrentPage(page)));

// Page containers: mounted once, switched by display (keep-alive).
const PAGES: PageId[] = [
  "my_skills",
  "marketplace",
  "skill_cards",
  "projects",
  "accounts",
  "publisher_detail",
  "settings",
];

function pageBody(page: PageId): JSX.Element {
  switch (page) {
    case "my_skills":
      return <MySkillsPage />;
    case "marketplace":
      return <MarketplacePage />;
    case "skill_cards":
      return <SkillCardsPage />;
    case "projects":
      return <ProjectsPage />;
    case "accounts":
      return <AccountsPage />;
    case "publisher_detail":
      return <MarketPublisherPage publisher={currentPublisher()} />;
    case "settings":
      return <SettingsPage />;
  }
}

export function Shell(): JSX.Element {
  return <ShellBody />;
}

function ShellBody(): JSX.Element {
  const railW = () => (railCollapsed() ? RAIL_COLLAPSED_W : RAIL_W);
  return (
    <View
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        backgroundColor: paletteToken("bg"),
      }}
    >
      {/* Top canvas drag band: covers the top SHELL_GAP strip above the main
          panel and the left/inter-pane/right canvas gutters across the header. */}
      <View
        testID="shell-top-drag"
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: SHELL_GAP + TOOLBAR_H + 24,
        }}
        desktop={{ windowDrag: true }}
      />

      {/* The rail lane. Collapsed clips the labels the rail still lays out
          at the expanded width (shell.rs render parity). */}
      <View
        style={{
          position: "absolute",
          top: 0,
          left: SHELL_GAP,
          bottom: SHELL_GAP,
          width: railW(),
          overflow: "hidden",
        }}
      >
        <Sidebar />
      </View>

      {/* The main panel: floats on the canvas, opaque, rounded, bordered. */}
      <View
        style={{
          position: "absolute",
          top: SHELL_GAP,
          right: SHELL_GAP,
          bottom: SHELL_GAP,
          left: SHELL_GAP + railW() + SHELL_GAP,
          display: "flex",
          flexDirection: "column",
          borderRadius: PANEL_RADIUS,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("panel"),
          overflow: "hidden",
        }}
      >
        <View style={{ position: "relative", flex: 1, minHeight: 0, width: "100%", height: "100%" }}>
          {PAGES.map((page) => (
            <View
              testID={`page-${page}`}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                right: 0,
                bottom: 0,
                display: currentPage() === page ? "flex" : "none",
                flexDirection: "column",
              }}
            >
              {pageBody(page)}
            </View>
          ))}
        </View>
        {/* The alert band floats over the page at the top center; last
            child, so it paints above the page. */}
        <NoticeBand />
      </View>

      {/* Prompt dialogs ride above everything. */}
      <DialogHost />
    </View>
  );
}

// Re-exported for tests: which rail row the active page maps onto.
export { railPage };
