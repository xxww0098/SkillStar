// nsh 0.6+ 原生右键上下文菜单（Menu.popup + Clipboard）：
// 供账号工作台三态视图（卡片 / 紧凑 / 列表）与全局额度下拉窗共用，提供零额外 DOM 开销的原生菜单交互。

import { Clipboard, type ContextMenuEvent } from "nsh";
import { Menu, type MenuItem } from "nsh/desktop";
import { t } from "../../services/translation";
import { toast } from "../../shell/events";
import { accountName } from "./logic";
import { qt } from "./quotaPopoverStrings";
import type { AccountView } from "./types";

export interface AccountContextMenuOptions {
  account: AccountView;
  serving?: boolean;
  busy?: boolean;
  refreshing?: boolean;
  onActivate?: () => void;
  onRefresh?: () => void;
  onReauth?: () => void;
  onOpenAgent?: () => void;
  onDelete?: () => void;
}

export function buildAccountContextMenu(opts: AccountContextMenuOptions): MenuItem[] {
  const { account } = opts;
  const serving = Boolean(opts.serving);
  const busy = Boolean(opts.busy);
  const refreshing = Boolean(opts.refreshing);
  const identity = (account.email?.trim() || accountName(account)).trim();

  const items: MenuItem[] = [];

  if (opts.onActivate) {
    items.push({
      label: serving ? t("accountsPage.current") : t("accountsPage.switch"),
      checked: serving ? true : undefined,
      disabled: serving || busy,
      onSelect: opts.onActivate,
    });
  }

  if (opts.onRefresh) {
    items.push({
      label: t("common.refresh"),
      disabled: busy || refreshing,
      onSelect: opts.onRefresh,
    });
  }

  if (account.requiresReauth && opts.onReauth) {
    items.push({
      label: t("accountsPage.reauthorize"),
      disabled: busy,
      onSelect: opts.onReauth,
    });
  }

  if (identity.length > 0) {
    if (items.length > 0) items.push({ type: "separator" });
    items.push({
      label: qt("quotaPopover.copyIdentity"),
      onSelect: () => {
        void Clipboard.writeText(identity).then(() => {
          toast(t("accountsPage.copied"), "success");
        });
      },
    });
  }

  if (opts.onOpenAgent) {
    items.push({
      label: qt("quotaPopover.openWorkbench"),
      onSelect: opts.onOpenAgent,
    });
  }

  if (opts.onDelete) {
    if (items.length > 0) items.push({ type: "separator" });
    items.push({
      label: t("common.delete"),
      disabled: busy || refreshing,
      onSelect: opts.onDelete,
    });
  }

  return items;
}

export function openAccountContextMenu(
  event: ContextMenuEvent,
  opts: AccountContextMenuOptions,
): void {
  const items = buildAccountContextMenu(opts);
  if (items.length === 0) return;
  void Menu.popup(items, event.pagePosition);
}
