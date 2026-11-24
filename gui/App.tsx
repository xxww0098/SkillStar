import { ErrorBoundary, mount, signal } from "nsh";
import { installScrollbarTheme } from "./src/services/themeMode";
import { installResidentWindow } from "./src/shell/residentWindow";
import { Shell, ShellErrorFallback } from "./src/shell/Shell";
import { TrayQuotaWindow } from "./src/shell/trayQuotaWindow";

// Ready gate: shell-level services (theme mode, locale) carry their defaults
// synchronously, so the gate opens on the first frame today. The bridge boot
// task (bootstrap facade, later waves) joins this gate before the shell
// mounts, mirroring ss-gpui's run() waiting on prepare_process.
const ready = signal(true);

function App() {
  // Closing the main window hides it and keeps the app running: see
  // src/shell/residentWindow.ts. Registered while rendering, so the listener,
  // native menus/shortcuts, and scrollbar theme belong to this root component.
  installScrollbarTheme();
  installResidentWindow();
  return ready() ? (
    <>
      <ErrorBoundary
        fallback={(error, reset) => (
          <ShellErrorFallback error={error} onReset={reset} />
        )}
      >
        <Shell />
      </ErrorBoundary>
      <ErrorBoundary fallback={() => null}>
        <TrayQuotaWindow />
      </ErrorBoundary>
    </>
  ) : null;
}

mount(App);
