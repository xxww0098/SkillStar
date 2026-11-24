// 账号页的偏好切片 —— gui_prefs.json 里属于本页的那一个键
// （native/src/facades/settings.rs 的 Settings.loadAccountsAutoRefresh /
// saveAccountsAutoRefresh）。
//
// 为什么单独一个文件：nsh 的 JS 环境没有 DOM（gui/.nsh/tsconfig.json 的 lib
// 只有 ES2023，globals.d.ts 写明「there is no DOM」），浏览器式的
// localStorage 连类型检查都过不去；跨重启的偏好一律落 gui_prefs.json。
// 放在本页目录而不是 settings 页：唯一读者是账号页，settings 页的
// bridge.ts 只管它自己的三个字段，页面之间也不互相 import。

import { Settings } from "skillstar-gui/native";

/** 自动刷新周期的持久化接缝：调度器只面对 load/save，测试装置注入内存实现。 */
export interface AutoRefreshPrefs {
  /** 已保存或默认的周期（ms）；`null` = 显式关闭。 */
  load(): number | null;
  /** 保存周期；`null` 持久化显式关闭。写失败静默：偏好不该打断刷新。 */
  save(ms: number | null): void;
}

/** 真实现：native Settings facade 直调（与 settings 页的 bridge 同款纪律）。 */
export const realAutoRefreshPrefs: AutoRefreshPrefs = {
  load() {
    const saved = Settings.loadAccountsAutoRefresh();
    return typeof saved === "number" ? saved : null;
  },
  save(ms) {
    try {
      Settings.saveAccountsAutoRefresh(ms);
    } catch {
      // 只读数据目录 / 磁盘满：本次会话内仍按内存里的周期跑。
    }
  },
};
