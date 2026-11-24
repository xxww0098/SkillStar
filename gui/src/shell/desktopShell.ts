// nsh 0.6+ 原生桌面宿主增强（菜单栏 / 全局快捷键 / 前后台生命周期 / 任务栏与 Dock 进度 / 退出确认）：
// - 独立成纯 TS 模块，保持 residentWindow.ts 精简可读（< 220 行）。
// - 严格基于 nsh 最新稳定版 API：`platform`、`lifecycle`（来自 "nsh"），以及
//   `Application`、`isAvailable`、`Menu`、`GlobalShortcut`、`WindowProgress`（来自 "nsh/desktop"）。

import { cleanup, computed, effect, lifecycle, untracked } from "nsh";
import {
  Application,
  GlobalShortcut,
  Menu,
  isAvailable as isDesktopAvailable,
  type MenuItem,
  type WindowProgress,
  type WindowRef,
} from "nsh/desktop";
import { closeConfirm, openConfirm } from "../chrome/dialog";
import { sharedAccountsState } from "../pages/accounts";
import { qt } from "../pages/accounts/quotaPopoverStrings";
import { locale } from "../services/locale";
import { isLight, setThemeMode, type ThemeMode } from "../services/themeMode";
import { t } from "../services/translation";
import { requestThemeMode } from "./events";
import { jobList, type Job } from "./jobBoard";
import { collapsed, currentPage, setCurrentPage, type PageId } from "./nav";
import { trayQuotaEnabled } from "./trayQuotaPrefs";

/** 全局唤起 / 收起账号额度下拉窗的系统级快捷键（macOS: ⌥⇧Q，Windows: Alt+Shift+Q）。 */
export const GLOBAL_QUOTA_SHORTCUT = "alt+shift+q";

/**
 * 将后台任务板（jobBoard）映射为 nsh 0.6 `WindowProgress`（Dock 图标与 Windows 任务栏进度条）。
 * 纯函数导出以便单测直接断言确定性与不确定性进度规则。
 */
export function computeWindowProgress(jobs: readonly Job[]): WindowProgress | null {
  const active = jobs.filter((job) => !job.done);
  if (active.length === 0) return null;
  const fractions = active
    .map((job) => job.fraction)
    .filter((f): f is number => typeof f === "number" && Number.isFinite(f));
  if (fractions.length === 0) {
    return { state: "indeterminate" };
  }
  const avg = fractions.reduce((sum, item) => sum + Math.max(0, Math.min(1, item)), 0) / fractions.length;
  return {
    state: "normal",
    value: Math.round(avg * 1000) / 1000,
  };
}

function menuLabel(zh: string, en: string): string {
  return locale() === "en" ? en : zh;
}

const QUIT_CONFIRM_TOKEN = "shell-quit-confirm";
let quitConfirmOpen = false;
let shellWindowRef: WindowRef | null = null;

/** 返回当前仍在进行中且支持取消的后台任务。 */
export function activeCancellableJobs(jobs: readonly Job[]): Job[] {
  return jobs.filter((job) => !job.done && job.cancellable);
}

/** 生成退出确认弹窗的双语文案，最多展示 3 项进行中的可取消任务。 */
export function quitConfirmCopy(
  jobs: readonly Job[],
  lang: "zh-CN" | "en" = locale(),
): { title: string; body: string; okText: string } {
  const preview = jobs.slice(0, 3).map((job) => `· ${job.title}`);
  if (jobs.length > 3) {
    preview.push(
      lang === "en"
        ? `· …and ${jobs.length - 3} more`
        : `· ……等共 ${jobs.length} 项任务`,
    );
  }
  if (lang === "en") {
    return {
      title:
        jobs.length === 1
          ? "Cancel active task and quit?"
          : `Cancel ${jobs.length} active tasks and quit?`,
      body: `Quitting now will cancel the ongoing tasks below:\n${preview.join("\n")}`,
      okText: "Cancel & Quit",
    };
  }
  return {
    title:
      jobs.length === 1
        ? "取消进行中的任务并退出？"
        : `取消 ${jobs.length} 项进行中的任务并退出？`,
    body: `立即退出将会中断以下正在执行的任务：\n${preview.join("\n")}`,
    okText: "中止并退出",
  };
}

/**
 * 统一的应用退出入口（供原生 `quitRequest` 拦截与托盘「退出 SkillStar」复用）。
 * 存在可取消的进行中任务时弹出确认框；无活跃可取消任务或非桌面环境则安全退让/直接退出。
 */
export function requestAppQuit(window: WindowRef | null = shellWindowRef): void {
  const active = activeCancellableJobs(jobList());
  if (active.length === 0) {
    if (quitConfirmOpen) {
      quitConfirmOpen = false;
      untracked(() => closeConfirm(QUIT_CONFIRM_TOKEN));
    }
    if (isDesktopAvailable) {
      Application.quit();
    }
    return;
  }

  window?.focus();
  if (quitConfirmOpen) return;

  quitConfirmOpen = true;
  let approved = false;
  const copy = quitConfirmCopy(active);

  openConfirm({
    token: QUIT_CONFIRM_TOKEN,
    title: copy.title,
    body: copy.body,
    okText: copy.okText,
    danger: true,
    fields: () => {
      effect(() => {
        if (activeCancellableJobs(jobList()).length === 0 && quitConfirmOpen) {
          quitConfirmOpen = false;
          untracked(() => closeConfirm(QUIT_CONFIRM_TOKEN));
        }
      });
      cleanup(() => {
        quitConfirmOpen = false;
        if (!approved && isDesktopAvailable) {
          Application.cancelQuitRequest();
        }
      });
      return null;
    },
    onOk: () => {
      approved = true;
      quitConfirmOpen = false;
      const current = activeCancellableJobs(jobList());
      for (const job of current) {
        try {
          job.onCancel?.();
        } catch {
          // Best-effort cancellation before quitting.
        }
      }
      if (isDesktopAvailable) {
        Application.quit();
      }
    },
  });
}

function navigateFromMenu(window: WindowRef, page: PageId, closePopover: () => void): void {
  closePopover();
  if (page === "accounts") {
    sharedAccountsState.start();
  }
  setCurrentPage(page);
  window.focus();
}

/** 构建桌面端原生应用菜单（macOS 顶部系统菜单栏 + 跨平台菜单快捷键）。 */
export function buildApplicationMenu(opts: {
  window: WindowRef;
  closePopover: () => void;
  toggleQuotaPopover: () => void;
}): MenuItem[] {
  const activePage = currentPage();
  const railCollapsed = collapsed();
  const light = isLight();

  return [
    {
      label: "SkillStar",
      submenu: [
        { role: "about" },
        { type: "separator" },
        {
          label: `${t("sidebar.settings")}…`,
          shortcut: "mod+,",
          onSelect: () => navigateFromMenu(opts.window, "settings", opts.closePopover),
        },
        { type: "separator" },
        { role: "hide" },
        { role: "hideOthers" },
        { role: "showAll" },
        { type: "separator" },
        { role: "quit" },
      ],
    },
    {
      label: menuLabel("编辑", "Edit"),
      submenu: [
        { role: "undo" },
        { role: "redo" },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { role: "selectAll" },
      ],
    },
    {
      label: menuLabel("视图", "View"),
      submenu: [
        {
          label: t("sidebar.skills"),
          shortcut: "mod+1",
          checked: activePage === "my_skills",
          onSelect: () => navigateFromMenu(opts.window, "my_skills", opts.closePopover),
        },
        {
          label: t("sidebar.market"),
          shortcut: "mod+2",
          checked: activePage === "marketplace" || activePage === "publisher_detail",
          onSelect: () => navigateFromMenu(opts.window, "marketplace", opts.closePopover),
        },
        {
          label: t("sidebar.groups"),
          shortcut: "mod+3",
          checked: activePage === "skill_cards",
          onSelect: () => navigateFromMenu(opts.window, "skill_cards", opts.closePopover),
        },
        {
          label: t("sidebar.projects"),
          shortcut: "mod+4",
          checked: activePage === "projects",
          onSelect: () => navigateFromMenu(opts.window, "projects", opts.closePopover),
        },
        {
          label: t("sidebar.mcp"),
          shortcut: "mod+5",
          checked: activePage === "mcp",
          onSelect: () => navigateFromMenu(opts.window, "mcp", opts.closePopover),
        },
        {
          label: t("sidebar.accounts"),
          shortcut: "mod+6",
          checked: activePage === "accounts",
          onSelect: () => navigateFromMenu(opts.window, "accounts", opts.closePopover),
        },
        { type: "separator" },
        {
          label: qt("quotaPopover.triggerTip"),
          shortcut: "mod+shift+u",
          onSelect: opts.toggleQuotaPopover,
        },
        {
          label: railCollapsed ? t("sidebar.expand") : t("sidebar.collapse"),
          shortcut: "mod+b",
          when: "notEditing",
          onSelect: () => collapsed.update((prev) => !prev),
        },
        {
          label: t("settings.backgroundStyle"),
          shortcut: "mod+shift+l",
          checked: light,
          onSelect: () => {
            const next: ThemeMode = isLight() ? "current" : "paper";
            setThemeMode(next);
            requestThemeMode.publish(next);
          },
        },
      ],
    },
    {
      label: menuLabel("窗口", "Window"),
      submenu: [
        { role: "minimize" },
        { role: "zoom" },
        { role: "toggleFullScreen" },
        { type: "separator" },
        { role: "close" },
      ],
    },
  ];
}

/**
 * 挂载 nsh 0.6+ 桌面壳增强能力（必须在根组件渲染期内调用，与 `currentWindow()` 同生命周期）。
 */
export function installDesktopFeatures(opts: {
  window: WindowRef;
  closePopover: () => void;
  toggleQuotaPopover: () => void;
}): void {
  shellWindowRef = opts.window;
  cleanup(() => {
    if (shellWindowRef === opts.window) {
      shellWindowRef = null;
    }
  });

  // 1. 前后台生命周期感知（lifecycle）：从后台/失焦切回前台时，自动对账本机凭据并静默补刷过期配额。
  const RESUME_SYNC_COOLDOWN_MS = 15_000;
  let lastResumeSyncAt = 0;
  let prevState = lifecycle();
  effect(() => {
    const nextState = lifecycle();
    const resumed = prevState !== "active" && nextState === "active";
    prevState = nextState;
    if (!resumed) return;
    // 后台唤起的面板不抢键盘（不激活应用），所以它收不到"失焦"；用户回到 SkillStar
    // （点主窗口 / Dock 图标）就当作一次「点外面」，收起面板。
    opts.closePopover();
    untracked(() => {
      const now = Date.now();
      if (now - lastResumeSyncAt < RESUME_SYNC_COOLDOWN_MS) return;
      if (currentPage() === "accounts" || trayQuotaEnabled()) {
        lastResumeSyncAt = now;
        sharedAccountsState.start();
        sharedAccountsState.reconcileLive();
        sharedAccountsState.refreshStale();
      }
    });
  });

  if (!isDesktopAvailable) return;

  // 2. 原生 Dock / Windows 任务栏进度镜像（WindowRef.set({ progress })）。
  let lastProgressKey = "";
  effect(() => {
    const progress = computeWindowProgress(jobList());
    const key =
      progress === null
        ? "none"
        : `${progress.state ?? "normal"}:${progress.value ?? -1}`;
    if (key === lastProgressKey) return;
    lastProgressKey = key;
    opts.window.set({ progress });
  });

  // 3. 响应式原生应用菜单与快捷键（Menu.setApplicationMenu）。
  // 首次同步挂载以立即注册快捷键；后续因侧栏折叠/主题/切页触发的菜单重建延迟到
  // 下一事件循环合并提交，避免在点击帧同步调用 NSApp.setMainMenu 拖慢首帧响应。
  let menuInitialized = false;
  let menuTimer: ReturnType<typeof setTimeout> | null = null;
  effect(() => {
    const items = buildApplicationMenu(opts);
    if (!menuInitialized) {
      menuInitialized = true;
      Menu.setApplicationMenu(items);
      return;
    }
    if (menuTimer !== null) clearTimeout(menuTimer);
    menuTimer = setTimeout(() => {
      menuTimer = null;
      Menu.setApplicationMenu(items);
    }, 0);
  });

  // 4. 系统全局快捷键（GlobalShortcut）：启用状态栏/托盘额度窗时支持任意应用内一键呼出。
  effect(() => {
    if (!trayQuotaEnabled()) {
      GlobalShortcut.unregister(GLOBAL_QUOTA_SHORTCUT);
      return;
    }
    void GlobalShortcut.register(GLOBAL_QUOTA_SHORTCUT, () => {
      opts.toggleQuotaPopover();
    }).catch(() => {
      // 若快捷键已被其他常驻工具占用，静默降级不阻断主界面。
    });
    return () => {
      GlobalShortcut.unregister(GLOBAL_QUOTA_SHORTCUT);
    };
  });

  // 5. 原生退出拦截（Application.on("quitRequest")）：仅在存在可取消后台任务时订阅拦截。
  const hasCancellableJob = computed(() => activeCancellableJobs(jobList()).length > 0);
  effect(() => {
    if (!hasCancellableJob()) {
      if (quitConfirmOpen) {
        quitConfirmOpen = false;
        untracked(() => closeConfirm(QUIT_CONFIRM_TOKEN));
      }
      return;
    }
    Application.setQuitRequestOptions?.({ system: true, mainWindowClose: false });
    const stop = Application.on("quitRequest", () => {
      requestAppQuit(opts.window);
    });
    return () => {
      stop();
      if (quitConfirmOpen) {
        quitConfirmOpen = false;
        untracked(() => closeConfirm(QUIT_CONFIRM_TOKEN));
      }
    };
  });
}
