import { mount, signal } from "nsh";
import { installResidentWindow } from "./src/shell/residentWindow";
import { Shell } from "./src/shell/Shell";

// Ready gate: shell-level services (theme mode, locale) carry their defaults
// synchronously, so the gate opens on the first frame today. The bridge boot
// task (bootstrap facade, later waves) joins this gate before the shell
// mounts, mirroring ss-gpui's run() waiting on prepare_process.
const ready = signal(true);

// Window chrome: nsh.json's "window" key declares the main window's title and
// its hidden 44pt title bar (nsh 0.5.x; the pre-0.5 imperative
// Window.setTitle/setTitleBar API this file used is gone). Both desktop hosts
// run that hidden bar, so the platform's window controls are drawn over the
// app and src/shell/caption.ts is what keeps the rail and the page bands out
// of their way (macOS's traffic lights on the left, Windows' caption buttons on
// the right, D-115). The app bar stays draggable through
// desktop={{ windowDrag: true }} regions (the sidebar traffic lane, the
// toolbar band and its slack) on both.

function App() {
  // Closing the main window hides it and keeps the app running: see
  // src/shell/residentWindow.ts. Registered while rendering, so the listener
  // belongs to this component (which never unmounts).
  installResidentWindow();
  return ready() ? <Shell /> : null;
}

mount(App);
