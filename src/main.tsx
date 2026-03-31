import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SkillsProvider } from "./hooks/useSkills";
import { SecurityScanProvider } from "./hooks/useSecurityScan";
import { SplashScreen } from "./components/ui/SplashScreen";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { initializeBackgroundStyle } from "./lib/backgroundStyle";
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <SplashScreen>
        <SkillsProvider>
          <SecurityScanProvider>
            <App />
          </SecurityScanProvider>
        </SkillsProvider>
      </SplashScreen>
    </ErrorBoundary>
  </React.StrictMode>,
);
