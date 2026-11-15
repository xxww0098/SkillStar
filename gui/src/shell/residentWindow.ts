// The main window stays resident when the user closes it: the close press
// hides it instead of ending the app. nsh.json asks the app first
// (`window.closeOnPress: false`), so the platform reports the press and
// leaves the window open; hiding it keeps the engine, the bridge, the domain
// runtime and `ss-app`'s background wakes (`channel_wake`, `skill_wake`,
// `release_check_wake`) running, and the window comes back without
// rebuilding its state.
//
// The way back is the platform's:
// - macOS: the Dock icon (AppKit's `applicationShouldHandleReopen:`); the
//   host also passes `keep_alive_on_close`, so the platform never closes the
//   window behind the app's back.
// - Windows: a hidden window leaves the taskbar too, so the notification area
//   carries the way back and the only visible Quit. nsh.json
//   `"singleInstance": true` also hands a second launch to this instance
//   (the same window forward), which is what a Start-menu launch does.
import os from "os";
import { Application, Tray, currentWindow, type WindowRef } from "nsh/desktop";

/** Brings the main window forward, a hidden one included. */
function showMain(window: WindowRef): void {
  window.focus();
}

/**
 * Makes closing the main window hide it, and gives Windows the tray entry
 * that brings it back. Call it while a component renders: `currentWindow()`
 * throws outside one, and the listener then lives with that component.
 */
export function installResidentWindow(): void {
  const window = currentWindow();
  window.on("closePress", () => window.hide());
  if (os.platform() === "windows") {
    Tray.set({
      tooltip: "SkillStar",
      onClick: () => showMain(window),
      menu: [
        { label: "显示 SkillStar", onSelect: () => showMain(window) },
        { type: "separator" },
        { label: "退出 SkillStar", onSelect: () => Application.quit() },
      ],
    });
  }
}
