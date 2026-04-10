import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { SplashScreen } from "./components/ui/SplashScreen";
import { SkillsProvider } from "./features/my-skills/hooks/useSkills";
import { SecurityScanProvider } from "./features/security/hooks/useSecurityScan";
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
