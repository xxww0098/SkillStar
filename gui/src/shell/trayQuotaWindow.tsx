// 独立无边框系统状态栏 / 托盘额度弹出窗视图：启动后静默预热并保持挂载（keep-alive），
// 收起时仅调用 WindowRef.hide() 而不销毁原生窗口，避免每次点击触发平台层新建窗口与
// 250ms first_frame 首帧等待延迟。
// 注意：无边框（frame="none"）透明窗口必须显式设 backdrop="none"，避免 macOS
// NSVisualEffectView 铺满矩形窗口底板导致四角露出直角灰边。

import { ErrorBoundary, effect, keymap, type JSX } from "nsh";
import { Window, isAvailable as isDesktopAvailable } from "nsh/desktop";
import { sharedAccountsState } from "../pages/accounts";
import { QuotaPopoverPanel } from "../pages/accounts/quotaPopover";
import {
  QUOTA_POPOVER_ESTIMATED_H,
  QUOTA_POPOVER_W,
} from "../pages/accounts/quotaPopoverLogic";
import { qt } from "../pages/accounts/quotaPopoverStrings";
import { ShellErrorFallback } from "./Shell";
import {
  bindPopupWindowRef,
  closeTrayQuotaPopover,
  focusWorkbenchFromTray,
  trayPopoverOpen,
  trayPopoverOrigin,
  trayPopupWarm,
  watchTrayPopoverOutsideClick,
} from "./residentWindow";
import { trayQuotaEnabled } from "./trayQuotaPrefs";

export function TrayQuotaWindow(): JSX.Element {
  effect(() => {
    if (!trayPopoverOpen()) return;
    const stopEscape = keymap.bind("escape", () => {
      closeTrayQuotaPopover(false);
    });
    const stopOutside = watchTrayPopoverOutsideClick();
    return () => {
      stopEscape();
      stopOutside();
    };
  });

  return (
    <If when={isDesktopAvailable && trayQuotaEnabled() && (trayPopupWarm() || trayPopoverOpen())}>
      <Window
        id="quota-popover"
        kind="popup"
        open={trayQuotaEnabled()}
        activate={false}
        title={qt("quotaPopover.title")}
        frame="none"
        transparent
        backdrop="none"
        shadow
        resizable={false}
        minimizable={false}
        maximizable={false}
        taskbar={false}
        closeOnPress={false}
        closeOnEscape={false}
        closeOnBlur={false}
        level="aboveDock"
        defaultSize={{ width: QUOTA_POPOVER_W, height: QUOTA_POPOVER_ESTIMATED_H }}
        sizeToContent="height"
        defaultPosition={trayPopoverOrigin()}
        ref={(win) => {
          bindPopupWindowRef(win);
        }}
        onClosePress={() => {
          closeTrayQuotaPopover(false);
        }}
        onOpenChange={(event) => {
          if (!event.open) {
            bindPopupWindowRef(null);
            trayPopoverOpen.set(false);
          }
        }}
        onFocusChange={(event) => {
          if (!event.focused && trayPopoverOpen()) {
            closeTrayQuotaPopover(true);
          }
        }}
      >
        <ErrorBoundary
          fallback={(error, reset) => (
            <ShellErrorFallback error={error} onReset={reset} compact />
          )}
        >
          <QuotaPopoverPanel
            state={sharedAccountsState}
            inPopupWindow
            active={trayPopoverOpen()}
            onOpenWorkbench={(agentId) => focusWorkbenchFromTray(agentId)}
            onOpenAdd={(agentId) => {
              const fallback =
                agentId ??
                sharedAccountsState.selected() ??
                sharedAccountsState.agents()[0]?.id ??
                "codex";
              focusWorkbenchFromTray(fallback);
              sharedAccountsState.openAdd(fallback);
            }}
            onReauthAccount={(accountId) => {
              const target = sharedAccountsState
                .accounts()
                .find((row) => row.id === accountId);
              focusWorkbenchFromTray(target?.agent);
              sharedAccountsState.reauth(accountId);
            }}
          />
        </ErrorBoundary>
      </Window>
    </If>
  );
}
