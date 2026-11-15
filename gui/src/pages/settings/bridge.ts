import { Settings } from "skillstar-gui/native";

// 设置基座桥 —— native/src/facades/settings.rs（Settings 模块）的 TS 半
// 边。与 accounts/bridge.ts 同款纪律：页面与测试装置只面对
// SettingsBridge；真实现是薄薄一层 JSON→类型收窄，测试装置注入假桥。
// 方法切面与 GPUI 侧 spawn/直调一一对应：prefs 读写对位
// ss-gpui/src/prefs.rs 的 load/save（同一 gui_prefs.json、同 schema、同
// 默认值），proxy 读写对位 settings/general.rs 的
// ss_core::config::proxy::{load_config, save_config}（save 是原子写）。
// 全部是本地文件 IO：同步方法（GPUI 构造函数里同样同步直调）。

/** gui_prefs.json 的三字段（prefs.rs GuiPrefs；字段名与磁盘一致）。 */
export interface PrefsDto {
  language: string;
  background_style: string;
  background_run: boolean;
}

/** proxy.json（ss_core::config::proxy::ProxyConfig 的 serde 形状）。 */
export interface ProxyDto {
  enabled: boolean;
  proxy_type: ProxyScheme;
  host: string;
  port: number;
  username: string | null;
  password: string | null;
  bypass: string | null;
}

/** proxy_type 的四个取值（ProxyType serde lowercase）。 */
export type ProxyScheme = "http" | "https" | "socks5" | "socks5h";

export const PROXY_SCHEMES: ProxyScheme[] = ["http", "https", "socks5", "socks5h"];

function asPrefs(value: unknown): PrefsDto {
  const raw = (value ?? {}) as Partial<PrefsDto>;
  return {
    language: typeof raw.language === "string" ? raw.language : "zh-CN",
    background_style:
      typeof raw.background_style === "string" ? raw.background_style : "current",
    background_run: raw.background_run === true,
  };
}

function asProxy(value: unknown): ProxyDto {
  const raw = (value ?? {}) as Partial<ProxyDto>;
  const scheme = PROXY_SCHEMES.find((item) => item === raw.proxy_type) ?? "http";
  return {
    enabled: raw.enabled === true,
    proxy_type: scheme,
    host: typeof raw.host === "string" ? raw.host : "",
    port: typeof raw.port === "number" ? raw.port : 7897,
    username: typeof raw.username === "string" ? raw.username : null,
    password: typeof raw.password === "string" ? raw.password : null,
    bypass: typeof raw.bypass === "string" ? raw.bypass : null,
  };
}

export interface SettingsBridge {
  /** prefs.rs load：缺文件/坏 JSON → 三字段默认（zh-CN/current/false）。 */
  loadPrefs(): PrefsDto;
  /** prefs.rs save：三字段全量写（pretty JSON）。返回错误文本或 null。 */
  savePrefs(prefs: PrefsDto): string | null;
  /** proxy::load_config：缺文件 → 默认（含默认 NO_PROXY 绕过清单）。 */
  loadProxy(): ProxyDto;
  /** proxy::save_config：原子写。返回错误文本或 null。 */
  saveProxy(proxy: ProxyDto): string | null;
}

/** 真桥：native facade 直调（页面默认使用）。 */
export const realSettingsBridge: SettingsBridge = {
  loadPrefs() {
    return asPrefs(Settings.loadPrefs());
  },
  savePrefs(prefs) {
    try {
      Settings.savePrefs(prefs.language, prefs.background_style, prefs.background_run);
      return null;
    } catch (error) {
      return String(error);
    }
  },
  loadProxy() {
    return asProxy(Settings.loadProxy());
  },
  saveProxy(proxy) {
    try {
      // 生成的 facade 签名把 JsonValue 收窄为 unknown（serde_json::Value
      // 的 TS 面），形状由 native 侧的 ProxyConfig 反序列化把守。
      Settings.saveProxy(proxy);
      return null;
    } catch (error) {
      return String(error);
    }
  },
};
