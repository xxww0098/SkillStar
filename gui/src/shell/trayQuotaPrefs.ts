// 状态栏 / 系统托盘额度下拉窗偏好切片（gui_prefs.json 的 tray_quota_enabled）。
// 放在 shell 层作为跨模块响应式真源：residentWindow.ts 的 Tray effect 与设置页
// GeneralSection 直接读写同一个 signal，无需页面互导。

import { signal } from "nsh";
import { Settings } from "skillstar-gui/native";

export interface TrayQuotaPrefsDto {
  enabled: boolean;
}

export interface TrayQuotaPrefsBridge {
  load(): TrayQuotaPrefsDto;
  save(prefs: TrayQuotaPrefsDto): void;
}

function parseTrayQuotaPrefs(raw: unknown): TrayQuotaPrefsDto {
  const obj = (raw ?? {}) as Partial<TrayQuotaPrefsDto>;
  return {
    enabled: obj.enabled !== false,
  };
}

export const realTrayQuotaPrefsBridge: TrayQuotaPrefsBridge = {
  load() {
    try {
      const fn = (Settings as unknown as { loadTrayQuotaPrefs?: () => unknown })
        .loadTrayQuotaPrefs;
      if (typeof fn === "function") {
        return parseTrayQuotaPrefs(fn());
      }
    } catch {
      // 读失败降级为默认开启，不阻塞壳挂载。
    }
    return { enabled: true };
  },
  save(prefs) {
    try {
      const fn = (
        Settings as unknown as {
          saveTrayQuotaPrefs?: (enabled: boolean) => void;
        }
      ).saveTrayQuotaPrefs;
      if (typeof fn === "function") {
        fn(prefs.enabled);
      }
    } catch {
      // 只读目录或测试假桥未实现时，当前会话仍保持内存里的开关状态。
    }
  },
};

export const trayQuotaEnabled = signal(true);

let seeded = false;

/** 应用启动或设置页挂载时同步一次磁盘偏好（幂等）。 */
export function seedTrayQuotaPrefs(
  bridge: TrayQuotaPrefsBridge = realTrayQuotaPrefsBridge,
): void {
  if (seeded && bridge === realTrayQuotaPrefsBridge) return;
  if (bridge === realTrayQuotaPrefsBridge) seeded = true;
  const loaded = bridge.load();
  trayQuotaEnabled.set(loaded.enabled);
}

export function setTrayQuotaEnabled(
  enabled: boolean,
  bridge: TrayQuotaPrefsBridge = realTrayQuotaPrefsBridge,
): void {
  trayQuotaEnabled.set(enabled);
  bridge.save({ enabled });
}
