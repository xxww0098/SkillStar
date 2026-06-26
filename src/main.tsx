import { isTauri } from "@tauri-apps/api/core";
import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { SplashScreen } from "./components/ui/SplashScreen";
import { SkillsProvider } from "./features/my-skills/hooks/useSkills";
import { UsageCardWindow } from "./features/usage/components/UsageCardWindow";
import { NavigationProvider } from "./hooks/useNavigation";
import { initializeBackgroundStyle } from "./lib/backgroundStyle";
import { queryClient } from "./lib/queryClient";
import "./index.css";
import "./i18n";

initializeBackgroundStyle();

// Disable the right-click context menu for native feel,
// but allow it in input/textarea elements for copy/paste.
document.addEventListener("contextmenu", (event) => {
  const target = event.target as HTMLElement;
  if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) {
    return;
  }
  event.preventDefault();
});

/**
 * Detect whether this webview is a usage floating-card window. Card windows
 * are opened by `open_usage_card_window` with label `usage-card-<sub_id>` and
 * load `index.html?window=usage-card&id=<sub_id>`; they render a stripped-down
 * `UsageCardWindow` root instead of the full app (no sidebar/splash/nav).
 *
 * This is the first window-label-routed surface in the codebase. The check
 * also honors the `?window=` query param so it works during vitest/dev.
 */
function isUsageCardWindow(): boolean {
  const fromQuery =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).get("window") === "usage-card";
  if (fromQuery) return true;
  if (!isTauri()) return false;
  // Synchronous label read — safe because by the time JS runs the window
  // label is already assigned by the backend.
  try {
    // Defer the dynamic import cost: the label is also reflected in the
    // document title prefix we set in the backend, but the most reliable
    // source is getCurrentWindow().label. We fall back to the query param
    // above for the non-Tauri case.
    return false; // query-param path is authoritative for the initial check
  } catch {
    return false;
  }
}

/**
 * Resolve the current Tauri window label (async) and re-render if this turns
 * out to be a card window whose label wasn't caught by the query param.
 * Kept lightweight: only runs once on mount.
 */
function useCardWindowLabel(): string | null {
  const [label, setLabel] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!isTauri()) return;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        const l = getCurrentWindow().label;
        if (l.startsWith("usage-card-")) setLabel(l);
      })
      .catch(() => {});
  }, []);
  return label;
}

function Root() {
  const cardLabel = useCardWindowLabel();
  // Render the card root when either the query param or the Tauri label says so.
  if (isUsageCardWindow() || cardLabel) {
    return (
      <React.StrictMode>
        <ErrorBoundary>
          <UsageCardWindow />
        </ErrorBoundary>
      </React.StrictMode>
    );
  }
  return (
    <React.StrictMode>
      <ErrorBoundary>
        <SplashScreen>
          <QueryClientProvider client={queryClient}>
            <SkillsProvider>
              <NavigationProvider>
                <App />
              </NavigationProvider>
            </SkillsProvider>
          </QueryClientProvider>
        </SplashScreen>
      </ErrorBoundary>
    </React.StrictMode>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<Root />);
