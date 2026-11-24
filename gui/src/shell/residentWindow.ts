// 常驻后台窗口与系统状态栏 / 托盘编排（纯 TS 状态与宿主桥接，不含 JSX；弹出窗视图在 trayQuotaWindow.tsx）：
// 1. 主窗口关闭即隐藏（window.closeOnPress: false -> window.hide()），进程与后台唤醒继续（D-115）。
// 2. 采用 nsh 0.6 原生 `platform.os` + `nsh/desktop` 的 `isAvailable` 统一驱动桌面托盘、菜单栏、
//    全局快捷键与 Dock/任务栏进度（详细扩展在 ./desktopShell.ts）。
// 3. 启用 trayQuotaEnabled 时，macOS 状态栏与 Windows 通知区托盘统一挂载 Tray：
//    - macOS 使用单色系统模板图标 `assets/tray-icon.svg`（无底板、自动适配深浅菜单栏），仅展示图标、不显示百分比数值；
//    - 弹出窗 `<Window id="quota-popover">` 启动后静默预热并常驻保活（关闭时 `hide()` 而非销毁重建，规避新建窗口首帧 250ms 等待）；
//    - 左键点击（或全局快捷键 ⌥⇧Q / Alt+Shift+Q）零延迟 `setBounds` + 呈现（见 presentTrayQuotaPopover：
//      应用不在最前时只 `show()`，绝不抢键盘，避免 AppKit 为给面板键盘而激活整个 App、把主窗口一并带到前台），
//      随后在下一帧异步对账与补刷配额；
//    - 右键菜单提供「显示 SkillStar」「打开账号工作台」「刷新全部配额」「退出 SkillStar」。

import {
  computed,
  effect,
  platform,
  signal,
  untracked,
  type Bounds,
  type Point,
} from "nsh";
import {
  Tray,
  currentWindow,
  displays,
  focusedWindow,
  isAvailable as isDesktopAvailable,
  type Display,
  type WindowRef,
} from "nsh/desktop";
import { Settings } from "skillstar-gui/native";
import { sharedAccountsState } from "../pages/accounts";
import { computeTrayPopoverOrigin } from "../pages/accounts/quotaPopoverLogic";
import { qt } from "../pages/accounts/quotaPopoverStrings";
import { locale } from "../services/locale";
import {
  installDesktopFeatures,
  requestAppQuit,
} from "./desktopShell";
import { setCurrentPage } from "./nav";
import { seedTrayQuotaPrefs, trayQuotaEnabled } from "./trayQuotaPrefs";

const FALLBACK_TRAY_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="#e8eef8">' +
  '<path d="M 4.30 15.46 A 7.50 7.50 0 0 1 17.96 9.66 L 15.77 10.90 A 4.98 4.98 0 0 0 6.72 14.74 Z"/>' +
  '<path d="M 18.99 14.52 A 7.50 7.50 0 0 1 7.08 19.58 L 10.74 18.46 A 4.98 4.98 0 0 0 15.65 16.38 Z"/>' +
  '<path d="M 18.63 8.38 A 11.75 2.90 -23.0 0 1 22.37 8.96 A 11.75 2.90 -23.0 0 1 0.73 18.14 A 11.75 2.90 -23.0 0 1 2.91 15.05 A 8.77 8.77 0 0 0 3.32 16.57 A 8.90 1.02 -23.0 0 0 3.26 16.79 A 8.90 1.02 -23.0 0 0 19.64 9.83 A 8.90 1.02 -23.0 0 0 19.44 9.72 A 8.77 8.77 0 0 0 18.63 8.38 Z"/>' +
  '<path d="M 18.75 0.90 Q 18.75 3.19 21.80 3.95 Q 19.51 3.95 18.75 7.00 Q 18.75 4.71 15.70 3.95 Q 17.99 3.95 18.75 0.90 Z"/>' +
  "</svg>";

function resolveTrayIconUri(): string {
  try {
    if (typeof Settings.trayIconDataUri === "function") {
      return Settings.trayIconDataUri();
    }
  } catch {
    // 假桥或未重编桥接环境自动回退同源内联 SVG。
  }
  return `data:image/svg+xml;base64,${btoa(FALLBACK_TRAY_ICON_SVG)}`;
}

const TRAY_ICON_URI = resolveTrayIconUri();

/** 防止弹出窗已聚焦时点击状态栏图标先触发失焦隐藏、数毫秒后又被 tray onClick 重新拉起。 */
const BLUR_REOPEN_GUARD_MS = 180;
/** 托盘展开与启动预热共享的最小对账/补刷冷却时间（毫秒），避免频繁开关托盘触发重对账风暴。 */
const TRAY_SYNC_COOLDOWN_MS = 15_000;

export const trayPopoverOpen = signal(false);
export const trayPopupWarm = signal(false);
export const trayPopoverOrigin = signal<Point>({ x: 920, y: 36 });
export const backgroundRunEnabled = signal(false);
let popupWindowRef: WindowRef | null = null;
let mainWindowRef: WindowRef | null = null;
let lastTrayBounds: Bounds | null = null;
let lastBlurDismissAt = 0;
let lastTraySyncAt = 0;
let residentBootScheduled = false;

/** 启动时读取 gui_prefs.json 初始化后台运行开关（缺失或异常降级为 false）。 */
export function seedBackgroundRunPrefs(
  loadPrefs: () => unknown = () => Settings.loadPrefs(),
): void {
  try {
    const raw = loadPrefs() as Partial<{ background_run: boolean }> | null | undefined;
    backgroundRunEnabled.set(raw?.background_run === true);
  } catch {
    backgroundRunEnabled.set(false);
  }
}

export function setBackgroundRunEnabled(enabled: boolean): void {
  backgroundRunEnabled.set(enabled);
}

/**
 * 主窗口关闭按钮分发：开启后台运行时隐藏主窗口保活；关闭后台运行时收起额度弹窗并退出应用。
 */
export function handleMainWindowClosePress(
  win: Pick<WindowRef, "hide" | "focus">,
  opts?: {
    backgroundRun?: boolean;
    closePopover?: () => void;
    quitApp?: (target: Pick<WindowRef, "hide" | "focus">) => void;
  },
): "hidden" | "quit" {
  const stayInBackground = opts?.backgroundRun ?? untracked(backgroundRunEnabled);
  if (stayInBackground) {
    win.hide();
    return "hidden";
  }
  (opts?.closePopover ?? (() => closeTrayQuotaPopover(false)))();
  (opts?.quitApp ?? ((target) => requestAppQuit(target as WindowRef)))(win);
  return "quit";
}

/**
 * 把额度面板呈现到屏幕上。
 *
 * macOS 上 `focus()` 会让面板成为 key window：应用不在最前时 AppKit 为了让这个窗口拿到键盘，
 * 会把 **整个应用**激活——主窗口（哪怕本来只是压在别的 App 后面）随之上浮，用户看到的
 * 就是「只想展开菜单，结果主窗口也跳到前台」。因此 macOS 后台唤起只 `show()`（orderFrontRegardless，
 * 不激活应用、不抢键盘），应用本来就在最前时才 `focus()`（不会再有激活，key/失焦收回照旧）；
 * 非激活呈现期间的外点收起由 `watchTrayPopoverOutsideClick()` 挂载的全局/本地鼠标监听负责。
 * 非 macOS 平台（Windows / Linux）无全局应用激活副作用，统一走 `focus()` 以保留原生失焦收起。
 * 改动这里前先读 docs/errors.md 的「2026-11-17 展开菜单却把主窗口带到前台」。
 */
export function presentTrayQuotaPopover(): void {
  const win = popupWindowRef;
  if (win === null) return;
  win.setBounds({ origin: trayPopoverOrigin(), animated: false });
  if (platform.os !== "macos" || focusedWindow() !== null) {
    win.focus();
  } else {
    win.show();
  }
}

/**
 * macOS 面板展开期间挂载全局与本地鼠标按下监听，点击面板之外的任意空白或其它窗口时自动收起。
 * 忽略面板自身、原生菜单及左键点击状态栏图标（交由 LeftMouseUp 统一切换开关）。
 */
export function watchTrayPopoverOutsideClick(): () => void {
  if (!isDesktopAvailable || platform.os !== "macos") {
    return () => {};
  }
  if (
    typeof Settings.waitPopoverOutsideClick !== "function" ||
    typeof Settings.cancelPopoverOutsideClick !== "function"
  ) {
    return () => {};
  }
  let active = true;
  void Settings.waitPopoverOutsideClick()
    .then((dismissed) => {
      if (active && dismissed && trayPopoverOpen()) {
        closeTrayQuotaPopover(true);
      }
    })
    .catch(() => {
      // 无头测试或未就绪宿主环境忽略取消/未实现异常。
    });
  return () => {
    if (!active) return;
    active = false;
    try {
      Settings.cancelPopoverOutsideClick();
    } catch {
      // 忽略测试环境清理时的桥接异常。
    }
  };
}

export function bindPopupWindowRef(win: WindowRef | null): void {
  popupWindowRef = win;
  if (win === null) return;
  if (trayPopoverOpen()) {
    presentTrayQuotaPopover();
  } else {
    // 启动静默预热：立即切入 Hidden（orderOut），使平台层 250ms first_frame 定时器不自动置顶或抢焦点。
    win.hide();
  }
}

export function closeTrayQuotaPopover(fromBlur = false): void {
  if (fromBlur && trayPopoverOpen()) {
    lastBlurDismissAt = Date.now();
  }
  if (trayPopoverOpen()) {
    trayPopoverOpen.set(false);
  }
  popupWindowRef?.hide();
}

/** Brings the main window forward, a hidden one included. */
export function showMainWindow(): void {
  mainWindowRef?.focus();
}

function findTargetDisplay(trayBounds: Bounds | null): Display | null {
  const list = displays();
  if (list.length === 0) return null;
  if (trayBounds !== null) {
    const cx = trayBounds.origin.x + trayBounds.size.width / 2;
    const cy = trayBounds.origin.y + trayBounds.size.height / 2;
    const hit = list.find(
      (d) =>
        cx >= d.bounds.origin.x &&
        cx <= d.bounds.origin.x + d.bounds.size.width &&
        cy >= d.bounds.origin.y &&
        cy <= d.bounds.origin.y + d.bounds.size.height,
    );
    if (hit !== undefined) return hit;
  }
  return list.find((d) => d.primary) ?? list[0];
}

export function toggleTrayQuotaPopover(
  trayBounds: Bounds | null = lastTrayBounds,
  fromTrayClick = false,
): void {
  if (trayBounds !== null) {
    lastTrayBounds = trayBounds;
  }
  if (trayPopoverOpen()) {
    closeTrayQuotaPopover(false);
    return;
  }
  if (fromTrayClick && Date.now() - lastBlurDismissAt < BLUR_REOPEN_GUARD_MS) {
    return;
  }

  const display = findTargetDisplay(trayBounds);
  const origin = computeTrayPopoverOrigin({
    trayBounds,
    visibleBounds: display?.visibleBounds ?? null,
    platform: platform.os,
  });
  trayPopoverOrigin.set(origin);
  trayPopupWarm.set(true);
  trayPopoverOpen.set(true);
  presentTrayQuotaPopover();

  // 先让预热好的原生弹出窗零延迟上屏，下一帧再异步执行在役凭据对账与过期配额补刷。
  setTimeout(() => {
    sharedAccountsState.start();
    const now = Date.now();
    if (now - lastTraySyncAt >= TRAY_SYNC_COOLDOWN_MS) {
      lastTraySyncAt = now;
      sharedAccountsState.reconcileLive();
      sharedAccountsState.refreshStale();
    }
  }, 16);
}

export function focusWorkbenchFromTray(agentId?: string): void {
  closeTrayQuotaPopover(false);
  sharedAccountsState.start();
  if (agentId !== undefined && agentId.length > 0) {
    sharedAccountsState.selectAgent(agentId);
  }
  setCurrentPage("accounts");
  showMainWindow();
}

/**
 * Makes closing the main window hide it, and wires the reactive menu-bar /
 * system-tray entry, native application menu, global shortcut, lifecycle
 * resume sync, and Dock/taskbar job progress. Call while a root component
 * renders (`currentWindow()`).
 */
export function installResidentWindow(): void {
  const window = currentWindow();
  mainWindowRef = window;
  seedBackgroundRunPrefs();
  window.on("closePress", () => {
    handleMainWindowClosePress(window);
  });

  seedTrayQuotaPrefs();

  installDesktopFeatures({
    window,
    closePopover: () => closeTrayQuotaPopover(false),
    toggleQuotaPopover: () => toggleTrayQuotaPopover(),
  });

  const hasAccounts = computed(() => sharedAccountsState.accounts().length > 0);

  effect(() => {
    if (!isDesktopAvailable) return;
    const plat = platform.os;

    // 跟踪语言切换，确保右键菜单文案即时刷新。
    locale();

    const enabled = trayQuotaEnabled();
    const hasRows = hasAccounts();

    if (!enabled) {
      closeTrayQuotaPopover(false);
      if (untracked(trayPopupWarm)) trayPopupWarm.set(false);
      if (plat === "windows") {
        Tray.set({
          tooltip: "SkillStar",
          onClick: () => window.focus(),
          menu: [
            { label: qt("quotaPopover.trayShowMain"), onSelect: () => window.focus() },
            { type: "separator" },
            { label: qt("quotaPopover.trayQuit"), onSelect: () => requestAppQuit(window) },
          ],
        });
      } else {
        Tray.set(null);
      }
      return;
    }

    // 常驻托盘启用时：120ms 装载本地快照并静默预热弹出窗，800ms 静默补刷过期配额并启动全局轮询。
    if (!residentBootScheduled) {
      residentBootScheduled = true;
      setTimeout(() => {
        sharedAccountsState.start();
        lastTraySyncAt = Date.now();
        sharedAccountsState.reconcileLive();
        if (trayQuotaEnabled()) {
          trayPopupWarm.set(true);
        }
      }, 120);
      setTimeout(() => {
        sharedAccountsState.refreshStale();
      }, 800);
    } else if (!untracked(trayPopupWarm)) {
      trayPopupWarm.set(true);
    }

    Tray.set({
      icon: plat === "windows" ? undefined : TRAY_ICON_URI,
      tooltip: "SkillStar",
      onClick: (event) => toggleTrayQuotaPopover(event.bounds, true),
      menu: [
        {
          label: qt("quotaPopover.trayShowMain"),
          onSelect: () => {
            closeTrayQuotaPopover(false);
            window.focus();
          },
        },
        {
          label: qt("quotaPopover.trayOpenAccounts"),
          onSelect: () => focusWorkbenchFromTray(),
        },
        {
          label: qt("quotaPopover.trayRefreshAll"),
          disabled: !hasRows,
          onSelect: () => {
            sharedAccountsState.start();
            const ids = sharedAccountsState.accounts().map((r) => r.id);
            if (ids.length > 0) sharedAccountsState.refreshAll(ids);
          },
        },
        { type: "separator" },
        {
          label: qt("quotaPopover.trayQuit"),
          onSelect: () => requestAppQuit(window),
        },
      ],
    });
  });
}
