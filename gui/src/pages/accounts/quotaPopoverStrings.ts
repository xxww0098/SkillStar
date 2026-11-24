// 全局账号额度下拉窗（系统状态栏/托盘 + 侧栏底栏快捷入口）双语文案。
// 保持 zh-CN 与 en 严格对齐，不继续膨胀共享 services/translation.ts。

import { locale, type Locale } from "../../services/locale";

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "quotaPopover.title": "账号额度",
    "quotaPopover.triggerTip": "账号额度速览",
    "quotaPopover.alertsCount": "{{count}} 项需关注",
    "quotaPopover.openWorkbench": "打开账号工作台",
    "quotaPopover.workbenchShort": "工作台",
    "quotaPopover.addShort": "添加",
    "quotaPopover.emptyTitle": "尚未添加受管账号",
    "quotaPopover.emptyHint": "在账号工作台中登录或从本机导入后，可在此随时查看剩余配额并一键切号。",
    "quotaPopover.emptyAction": "前往账号工作台",
    "quotaPopover.untrackedLive": "本机已登录（未入库）",
    "quotaPopover.importToTrack": "去工作台纳管以读取配额",
    "quotaPopover.notServing": "未激活",
    "quotaPopover.alternatesHeader": "备选账号（按剩余配额排序）",
    "quotaPopover.autoOff": "自动刷新未开启",
    "quotaPopover.autoEverySec": "自动每 {{n}} 秒",
    "quotaPopover.autoEveryMin": "自动每 {{n}} 分钟",
    "quotaPopover.autoToggleTip": "点击切换自动刷新周期",
    "quotaPopover.updatedAt": "更新于 {{stamp}}",
    "quotaPopover.trayShowMain": "显示 SkillStar",
    "quotaPopover.trayOpenAccounts": "打开账号工作台",
    "quotaPopover.trayRefreshAll": "刷新全部配额",
    "quotaPopover.trayQuit": "退出 SkillStar",
    "quotaPopover.copyIdentity": "复制账号标识",
    "quotaPopover.dismissError": "关闭错误提示",
    "quotaPopover.retry": "重试",
  },
  en: {
    "quotaPopover.title": "Account Quotas",
    "quotaPopover.triggerTip": "Quick Account Quotas",
    "quotaPopover.alertsCount": "{{count}} alert(s)",
    "quotaPopover.openWorkbench": "Open Accounts Workbench",
    "quotaPopover.workbenchShort": "Workbench",
    "quotaPopover.addShort": "Add",
    "quotaPopover.emptyTitle": "No Managed Accounts Yet",
    "quotaPopover.emptyHint":
      "Sign in or import local sessions in the Accounts workbench to monitor remaining quotas and switch accounts here.",
    "quotaPopover.emptyAction": "Open Accounts Workbench",
    "quotaPopover.untrackedLive": "Signed in locally (untracked)",
    "quotaPopover.importToTrack": "Import in workbench to read quota",
    "quotaPopover.notServing": "Standby",
    "quotaPopover.alternatesHeader": "Standby accounts (sorted by quota)",
    "quotaPopover.autoOff": "Auto-refresh off",
    "quotaPopover.autoEverySec": "Auto every {{n}}s",
    "quotaPopover.autoEveryMin": "Auto every {{n}}m",
    "quotaPopover.autoToggleTip": "Click to cycle auto-refresh interval",
    "quotaPopover.updatedAt": "Updated {{stamp}}",
    "quotaPopover.trayShowMain": "Show SkillStar",
    "quotaPopover.trayOpenAccounts": "Open Accounts Workbench",
    "quotaPopover.trayRefreshAll": "Refresh All Quotas",
    "quotaPopover.trayQuit": "Quit SkillStar",
    "quotaPopover.copyIdentity": "Copy Account Identity",
    "quotaPopover.dismissError": "Dismiss error",
    "quotaPopover.retry": "Retry",
  },
};

export function qt(key: string): string {
  const cur = locale();
  const alt: Locale = cur === "zh-CN" ? "en" : "zh-CN";
  return STRINGS[cur][key] ?? STRINGS[alt][key] ?? key;
}

export function qtf(key: string, params: Record<string, string | number>): string {
  let out = qt(key);
  for (const [k, v] of Object.entries(params)) {
    out = out.split(`{{${k}}}`).join(String(v));
  }
  return out;
}
