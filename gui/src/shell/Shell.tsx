import { ErrorBoundary, Text, View, effect, signal, untracked, type JSX } from "nsh";
import { Settings } from "skillstar-gui/native";
import { setLocale, normalizeLocale, locale } from "../services/locale";
import { paletteToken, setThemeMode } from "../services/themeMode";
import { t } from "../services/translation";
import { MySkillsPage } from "../pages/my_skills";
import { MarketplacePage } from "../pages/marketplace";
// Publisher drill-down: the marketplace wave's registered placeholder page
// (src/pages/marketplace/publisher/) — the publisher slice replaces that
// directory, never this file.
import { MarketPublisherPage } from "../pages/marketplace/publisher";
import { SkillCardsPage } from "../pages/skill_cards";
import { ProjectsPage } from "../pages/projects";
import { McpPage } from "../pages/mcp";
import { AccountsPage } from "../pages/accounts";
import { SettingsPage } from "../pages/settings";
import { PANEL_RADIUS, RAIL_COLLAPSED_W, RAIL_W, SHELL_GAP, TOOLBAR_H } from "../layout";
import {
  collapsed as railCollapsed,
  currentPage,
  railPage,
  setCurrentPage,
  shellWidth,
  type PageId,
} from "./nav";
import {
  requestThemeMode,
  selectPage,
  selectPublisher,
  type PublisherRef,
} from "./events";
import { Sidebar } from "./Sidebar";
import { NoticeBand } from "./notify";
import { JobBand } from "./jobBand";
import { DialogHost } from "../chrome/dialog";
import { GitHubSignInHost } from "../pages/my_skills/githubSignInSlot";

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
  "mcp",
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
    case "mcp":
      return <McpPage />;
    case "accounts":
      return <AccountsPage />;
    case "publisher_detail":
      return <MarketPublisherPage publisher={currentPublisher()} />;
    case "settings":
      return <SettingsPage />;
  }
}

function seedBootPrefs(): void {
  try {
    const raw = Settings.loadPrefs() as Partial<{
      language: string;
      background_style: string;
    }>;
    setLocale(normalizeLocale(typeof raw.language === "string" ? raw.language : "zh-CN"));
    setThemeMode(raw.background_style === "paper" ? "paper" : "current");
  } catch {
    // Keep default theme and locale when prefs are unavailable.
  }
}

export function Shell(): JSX.Element {
  return <ShellBody />;
}

function ShellBody(): JSX.Element {
  seedBootPrefs();
  const mountedPages = signal<Partial<Record<PageId, true>>>({ [untracked(currentPage)]: true });
  effect(() => {
    const page = currentPage();
    if (!untracked(() => mountedPages()[page])) {
      mountedPages.set({ ...untracked(mountedPages), [page]: true });
    }
  });
  // Persist sidebar/menu theme flips even before SettingsPage is visited;
  // once SettingsPage mounts, SettingsWorkbench owns prefs state + persistence.
  // Defer off Frame 1 so the theme repaint commits immediately.
  let pendingThemePersistTimer: ReturnType<typeof setTimeout> | null = null;
  effect(() =>
    requestThemeMode.subscribe((style) => {
      if (untracked(mountedPages).settings) return;
      if (pendingThemePersistTimer !== null) clearTimeout(pendingThemePersistTimer);
      pendingThemePersistTimer = setTimeout(() => {
        pendingThemePersistTimer = null;
        if (untracked(mountedPages).settings) return;
        try {
          const raw = Settings.loadPrefs() as Partial<{
            language: string;
            background_run: boolean;
          }>;
          const language = typeof raw.language === "string" ? raw.language : "zh-CN";
          const backgroundRun = raw.background_run === true;
          Settings.savePrefs(language, style, backgroundRun);
        } catch {
          // Ignore persistence failure on theme flip.
        }
      }, 0);
    }),
  );
  const railW = () => (railCollapsed() ? RAIL_COLLAPSED_W : RAIL_W);
  return (
    <View
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        backgroundColor: paletteToken("bg"),
      }}
      onLayout={(event) => {
        const w = event.bounds.size.width;
        if (w > 0 && w !== untracked(shellWidth)) shellWidth.set(w);
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
          {PAGES.map((page) => {
            const active = () => currentPage() === page;
            return (
              <View
                testID={`page-${page}`}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 0,
                  bottom: 0,
                  display: "flex",
                  flexDirection: "column",
                  opacity: active() ? 1 : 0,
                  pointerEvents: active() ? "auto" : "none",
                  zIndex: active() ? 1 : 0,
                }}
              >
                <If when={mountedPages()[page] === true}>
                  {() => (
                    <ErrorBoundary
                      fallback={(error, reset) => (
                        <ShellErrorFallback page={page} error={error} onReset={reset} />
                      )}
                    >
                      {pageBody(page)}
                    </ErrorBoundary>
                  )}
                </If>
              </View>
            );
          })}
        </View>
        {/* The alert band floats over the page at the top center; last
            child, so it paints above the page. */}
        <NoticeBand />
        <JobBand />
      </View>

      {/* Prompt dialogs & GitHub App device sign-in ride above everything. */}
      <GitHubSignInHost />
      <DialogHost />
    </View>
  );
}

function formatBoundaryError(error: unknown): string {
  if (error instanceof Error) {
    return error.message || error.name || "Unknown error";
  }
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }
  return String(error ?? "Unknown error");
}

/**
 * 壳层与页面级 `<ErrorBoundary>` 主题感知兜底视图，提供可恢复的「重试」重置按钮。
 */
export function ShellErrorFallback(props: {
  error: unknown;
  onReset: () => void;
  page?: PageId;
  compact?: boolean;
}): JSX.Element {
  const baseId = () =>
    props.page
      ? `page-error-${props.page}`
      : props.compact
        ? "tray-error-fallback"
        : "shell-error-fallback";
  const title = () => {
    if (locale() === "en") {
      return props.page ? "This view ran into an error" : "Something went wrong";
    }
    return props.page ? "页面渲染出现异常" : "界面渲染出现异常";
  };
  const detail = () => formatBoundaryError(props.error);
  const compact = () => props.compact === true;

  return (
    <View
      testID={baseId()}
      style={{
        flex: 1,
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        padding: compact() ? 16 : 28,
        backgroundColor: paletteToken(compact() ? "panel" : "bg"),
      }}
    >
      <View
        style={{
          width: compact() ? "100%" : 420,
          maxWidth: "100%",
          display: "flex",
          flexDirection: "column",
          gap: 10,
          padding: compact() ? 14 : 20,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("danger_border"),
          backgroundColor: paletteToken("panel"),
        }}
      >
        <Text
          style={{
            fontSize: compact() ? 14 : 15,
            fontWeight: "600",
            color: paletteToken("danger_fg"),
          }}
        >
          {title()}
        </Text>
        <Text
          testID={`${baseId()}-message`}
          style={{
            fontSize: 12,
            lineHeight: 18,
            color: paletteToken("fg_muted"),
          }}
        >
          {detail()}
        </Text>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            justifyContent: "flex-end",
            marginTop: 4,
          }}
        >
          <View
            testID={`${baseId()}-retry`}
            role="button"
            focusable
            onPress={() => props.onReset()}
            style={{
              paddingLeft: 12,
              paddingRight: 12,
              paddingTop: 6,
              paddingBottom: 6,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("card"),
              cursor: "pointer",
            }}
            hoverStyle={{
              backgroundColor: paletteToken("card_hover"),
            }}
            activeStyle={{
              backgroundColor: paletteToken("panel_hover"),
            }}
          >
            <Text
              style={{
                fontSize: 12,
                fontWeight: "500",
                color: paletteToken("fg"),
              }}
            >
              {t("common.retry")}
            </Text>
          </View>
        </View>
      </View>
    </View>
  );
}

// Re-exported for tests: which rail row the active page maps onto.
export { railPage };
