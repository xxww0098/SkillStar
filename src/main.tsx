import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { SkillsProvider } from "./features/my-skills/hooks/useSkills";
import { SecurityScanProvider } from "./features/security/hooks/useSecurityScan";
import { NavigationProvider } from "./hooks/useNavigation";
import { SplashScreen } from "./components/ui/SplashScreen";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { initializeBackgroundStyle } from "./lib/backgroundStyle";
import { queryClient } from "./lib/queryClient";
import "./index.css";
import "./i18n";

initializeBackgroundStyle();

// Disable the right-click context menu for native feel,
// but allow it in input/textarea elements for copy/paste.
document.addEventListener('contextmenu', (event) => {
  const target = event.target as HTMLElement;
  if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) {
    return;
  }
  event.preventDefault();
});

// Intercept external link clicks — open in system browser instead of navigating the WebView.
// Without this, <a target="_blank" href="https://..."> either navigates the WebView on Windows
// or silently fails, because Tauri's WebView does not support multi-window navigation.
document.addEventListener('click', (event) => {
  const anchor = (event.target as HTMLElement).closest('a');
  if (!anchor) return;
  const href = anchor.getAttribute('href');
  if (!href) return;
  // Only intercept absolute http(s) URLs
  if (!/^https?:\/\//i.test(href)) return;
  event.preventDefault();
  import('@tauri-apps/plugin-shell').then(({ open }) => open(href)).catch(() => {
    // Fallback: try window.open in case we're in a browser dev environment
    window.open(href, '_blank');
  });
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <SplashScreen>
        <QueryClientProvider client={queryClient}>
          <SkillsProvider>
            <SecurityScanProvider>
              <NavigationProvider>
                <App />
              </NavigationProvider>
            </SecurityScanProvider>
          </SkillsProvider>
        </QueryClientProvider>
      </SplashScreen>
    </ErrorBoundary>
  </React.StrictMode>,
);
