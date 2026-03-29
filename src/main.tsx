import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SkillsProvider } from "./hooks/useSkills";
import { initializeBackgroundStyle } from "./lib/backgroundStyle";
import "./index.css";
import "./i18n";

initializeBackgroundStyle();

// Disable the right-click context menu
document.addEventListener('contextmenu', event => {
  // Allow context menu only if we're explicitly trying to use devtools or in development maybe? 
  // For a strictly native feel, we just disable it globally.
  event.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SkillsProvider>
      <App />
    </SkillsProvider>
  </React.StrictMode>,
);
