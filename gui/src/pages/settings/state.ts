import { batch, signal, type Signal } from "nsh";
import { setLocale, normalizeLocale } from "../../services/locale";
import { setThemeMode } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { setBackgroundRunEnabled } from "../../shell/residentWindow";
import type { ProxyDto, ProxyScheme, SettingsBridge } from "./bridge";

// 设置页状态机 — crates/ss-gpui/src/settings/mod.rs + general.rs 的读写
// 半边对位。页面模块级单例即页面实体（keep-alive 只挂载一次）；测试装
// 置自建（bridge/seeds 注入，githubSignInSlot 同款纪律）。
//
// 读写纪律：
// - prefs 三字段（language/background_style/background_run）整读整写
//   （prefs.rs load/save 对位）；语言/背景样式切换写盘后把全局
//   locale/themeMode signal 一并播种（theme.rs set_mode / i18n set_language
//   对位——themeMode.ts 注释里"prefs-bridge wave re-seeds"即本文件）。
// - proxy 表单五个字段是受控输入的即时值；GPUI 的语义是 InputEvent::Blur
//   才 save_proxy，InputEvent::Change 只清状态行——对位：editProxyField
//   清 proxyStatus，commitProxy（每个输入框 onBlur + 开关/类型选择）才
//   写盘并置状态行。
// - 一切动态量都在 signal 里；组件体不重跑（Solid 式语义），signal 消费
//   必须落在 JSX 响应式绑定或事件回调上。

/** 全局播种钩子：语言/主题/后台运行开关落进 shell 级 signal。测试注入 spy。 */
export interface SettingsSeedHooks {
  applyLocale(language: string): void;
  applyTheme(style: string): void;
  applyBackgroundRun?(enabled: boolean): void;
}

export const defaultSeedHooks: SettingsSeedHooks = {
  // i18n set_language 对位：set_language 内部就是 normalize 后装目录。
  applyLocale(language) {
    setLocale(normalizeLocale(language));
  },
  // theme.rs set_mode 对位：只有 "paper" 是浅色，其余一律深色
  // （prefs_style_is_light）。
  applyTheme(style) {
    setThemeMode(style === "paper" ? "paper" : "current");
  },
  applyBackgroundRun(enabled) {
    setBackgroundRunEnabled(enabled);
  },
};

/** 代理表单字段（GPUI 五个 InputState 的对位）。 */
export type ProxyFormField = "host" | "port" | "user" | "pass" | "bypass";

export interface SettingsState {
  /** gui_prefs.json 的三字段（整读整写）。 */
  prefs: Signal<{ language: string; background_style: string; background_run: boolean }>;
  /** proxy.json 当前落盘值。 */
  proxy: Signal<ProxyDto>;
  /** 代理表单即时值（受控输入；GPUI InputState 的对位）。 */
  proxyForm: Signal<{
    host: string;
    port: string;
    user: string;
    pass: string;
    bypass: string;
  }>;
  /** 代理保存状态行（general.rs proxy_status）：null 隐藏。 */
  proxyStatus: Signal<string | null>;
  /** 代理卡展开（proxy_expanded）。 */
  proxyExpanded: Signal<boolean>;
  /** 代理类型下拉开合（proxy_type_open）——渲染期同步，开合与当前值都
   * 在 JSX 里读 signal。 */
  proxyTypeOpen: Signal<boolean>;
  /** rail 高亮分区（mod.rs section）。 */
  activeSection: Signal<string>;
  /** 各分区内容顶（onLayout 实测；滚动联动的唯一输入）。 */
  sectionTops: Signal<number[]>;
  /** 页面挂载时读盘一次（构造函数 load 系列对位）并播种全局。 */
  start(): void;
  /** general.rs：切界面语言（写盘 + set_language）。 */
  setLanguage(language: "zh-CN" | "en"): void;
  /** general.rs：切背景样式（写盘 + set_mode）。 */
  setBackgroundStyle(style: "paper" | "current"): void;
  /** general.rs：切后台运行（写盘）。 */
  setBackgroundRun(enabled: boolean): void;
  /** 表单编辑：只清状态行（InputEvent::Change 对位）。 */
  editProxyField(field: ProxyFormField, value: string): void;
  /** general.rs save_proxy：从表单即时值组装写盘（Blur/开关/类型选择）。 */
  commitProxy(partial?: { enabled?: boolean; proxyType?: ProxyScheme }): void;
  setProxyExpanded(open: boolean): void;
  setProxyTypeOpen(open: boolean): void;
  /** rail 点击（focus_section）：记高亮，滚动由页面层执行。 */
  activateSection(id: string): void;
  /** 记录一个分区的内容顶（section slot onLayout）。 */
  reportSectionTop(index: number, top: number): void;
}

/** GPUI sync_active_from_scroll 的纯计算半边（top_item 近似）：
 * 滚到距底 24px 内钉在最后一个分区；否则取内容顶不高于滚动位置的最
 * 后一个分区。导出供单测。 */
export function activeSectionFromScroll(
  tops: number[],
  offset: number,
  contentHeight: number,
  viewportHeight: number,
): number {
  if (tops.length === 0) return 0;
  const max = contentHeight - viewportHeight;
  if (max > 24 && max - offset < 24) return tops.length - 1;
  let index = 0;
  for (let i = 0; i < tops.length; i += 1) {
    // 分区顶已经越过视口顶（top <= offset）→ 它是当前顶部分区。
    if (tops[i] <= offset + 1) index = i;
  }
  return index;
}

const DEFAULT_PROXY: ProxyDto = {
  enabled: false,
  proxy_type: "http",
  host: "",
  port: 7897,
  username: null,
  password: null,
  bypass: null,
};

export function createSettingsState(
  bridge: SettingsBridge,
  seeds: SettingsSeedHooks = defaultSeedHooks,
  opts?: { deferLoad?: boolean },
): SettingsState {
  const prefs = signal({
    language: "zh-CN",
    background_style: "current",
    background_run: false,
  });
  const proxy = signal(opts?.deferLoad ? { ...DEFAULT_PROXY } : bridge.loadProxy());
  const initialForm = {
    host: proxy().host,
    port: String(proxy().port),
    user: proxy().username ?? "",
    pass: proxy().password ?? "",
    bypass: proxy().bypass ?? "",
  };
  const proxyForm = signal(initialForm);
  const proxyStatus = signal<string | null>(null);
  const proxyExpanded = signal(false);
  const proxyTypeOpen = signal(false);
  const activeSection = signal("agents");
  const sectionTops = signal<number[]>([]);

  const savePrefs = (next: { language: string; background_style: string; background_run: boolean }): void => {
    prefs.set(next);
    const error = bridge.savePrefs(next);
    if (error !== null) {
      // GPUI 侧 tracing::warn 后仍继续（页面不因写盘失败翻脸）。
      console.warn(`failed to save gui prefs: ${error}`);
    }
  };

  const state: SettingsState = {
    prefs,
    proxy,
    proxyForm,
    proxyStatus,
    proxyExpanded,
    proxyTypeOpen,
    activeSection,
    sectionTops,
    start() {
      // 读盘值先进局部变量再落 signal：effect(() => state.start()) 的追
      // 踪面必须为空（start 内不读任何 signal），否则 set 又触发 effect
      // 无限循环。
      const loadedPrefs = bridge.loadPrefs();
      const loadedProxy = bridge.loadProxy();
      batch(() => {
        prefs.set(loadedPrefs);
        proxy.set(loadedProxy);
        proxyForm.set({
          host: loadedProxy.host,
          port: String(loadedProxy.port),
          user: loadedProxy.username ?? "",
          pass: loadedProxy.password ?? "",
          bypass: loadedProxy.bypass ?? "",
        });
      });
      seeds.applyLocale(loadedPrefs.language);
      seeds.applyTheme(loadedPrefs.background_style);
      seeds.applyBackgroundRun?.(loadedPrefs.background_run);
    },
    setLanguage(language) {
      const next = { ...prefs(), language };
      savePrefs(next);
      seeds.applyLocale(language);
    },
    setBackgroundStyle(style) {
      const next = { ...prefs(), background_style: style };
      savePrefs(next);
      seeds.applyTheme(style);
    },
    setBackgroundRun(enabled) {
      savePrefs({ ...prefs(), background_run: enabled });
      seeds.applyBackgroundRun?.(enabled);
    },
    editProxyField(field, value) {
      proxyForm.set({ ...proxyForm(), [field]: value });
      proxyStatus.set(null);
    },
    commitProxy(partial) {
      const form = proxyForm();
      const port = Number.parseInt(form.port, 10);
      const next = {
        ...proxy(),
        ...(partial?.enabled !== undefined ? { enabled: partial.enabled } : {}),
        ...(partial?.proxyType !== undefined ? { proxy_type: partial.proxyType } : {}),
        host: form.host,
        port: Number.isFinite(port) && port > 0 && port <= 65535 ? port : 7897,
        username: form.user.length > 0 ? form.user : null,
        password: form.pass.length > 0 ? form.pass : null,
        bypass: form.bypass.length > 0 ? form.bypass : null,
      };
      proxy.set(next);
      const error = bridge.saveProxy(next);
      proxyStatus.set(error === null ? t("common.saved") : tf("settings.connectionFailed", { error }));
    },
    setProxyExpanded(open) {
      proxyExpanded.set(open);
    },
    setProxyTypeOpen(open) {
      proxyTypeOpen.set(open);
    },
    activateSection(id) {
      activeSection.set(id);
    },
    reportSectionTop(index, top) {
      const tops = sectionTops().slice();
      tops[index] = top;
      sectionTops.set(tops);
    },
  };
  return state;
}
