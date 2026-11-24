// 设置基座测试 — settings/{mod,general,parts}.rs + prefs.rs 的语义对位：
// 滚动联动纯计算、状态机（prefs 三字段整读整写 / Blur 保存 / 表单编辑清
// 状态行）、host 装置上的 UI 交互（rail、滑块、开关、类型下拉的渲染期
// 同步）、真桥只读冒烟（形状；读写 round-trip 在 native crate 的沙箱
// cargo test）。不写真实 $HOME。
import { expect, open, test } from "nsh/test";
import { Settings } from "skillstar-gui/native";
import { WINDOW_H, WINDOW_W } from "../../layout";
import { t } from "../../services/translation";
import { activeSectionFromScroll, createSettingsState } from "./state";
import type { PrefsDto, ProxyDto, SettingsBridge } from "./bridge";
import { SETTINGS_SECTIONS } from "./sections";

const HOST = "./settings.test.host.tsx";

async function openSettings() {
  const app = await open(HOST);
  // 产品默认窗宽：面板内容宽 ≥ 1074，rail 展开标签态（shareSheet.test
  // 同款纪律——无头引擎默认窗口比产品窄）。
  await app.setViewport(WINDOW_W, WINDOW_H);
  return app;
}

// ---- 纯语义（mod.rs sync_active_from_scroll 的派生半） ---------------------

test("scroll sync picks the section at the top and pins the last near the bottom", () => {
  const tops = [0, 300, 620, 900, 1300];
  // 顶部：第一个分区。
  expect(activeSectionFromScroll(tops, 0, 1600, 400)).toBe(0);
  // 滚过第二个分区顶：高亮跟随。
  expect(activeSectionFromScroll(tops, 310, 1600, 400)).toBe(1);
  expect(activeSectionFromScroll(tops, 700, 1600, 400)).toBe(2);
  // 距底 24px 内（max - offset < 24）：钉在最后一个分区（GPUI at_bottom）。
  // max = 1600 - 400 = 1200：offset 1177 距底 23px 钉尾；1176 距底 24px 不钉。
  expect(activeSectionFromScroll(tops, 1177, 1600, 400)).toBe(4);
  expect(activeSectionFromScroll(tops, 1176, 1600, 400)).toBe(3);
  // 空表不炸，回到第 0 项。
  expect(activeSectionFromScroll([], 500, 1600, 400)).toBe(0);
});

// ---- 状态机（假桥；general.rs 的读写语义） ---------------------------------

function fakeBridge(): { bridge: SettingsBridge; saves: { prefs: PrefsDto[]; proxies: ProxyDto[] } } {
  const saves = { prefs: [] as PrefsDto[], proxies: [] as ProxyDto[] };
  let prefs: PrefsDto = { language: "zh-CN", background_style: "current", background_run: false };
  let proxy: ProxyDto = {
    enabled: false,
    proxy_type: "http",
    host: "127.0.0.1",
    port: 7897,
    username: null,
    password: null,
    bypass: null,
  };
  return {
    saves,
    bridge: {
      loadPrefs: () => ({ ...prefs }),
      savePrefs(next) {
        prefs = { ...next };
        saves.prefs.push({ ...next });
        return null;
      },
      loadProxy: () => ({ ...proxy }),
      saveProxy(next) {
        proxy = { ...next };
        saves.proxies.push({ ...next });
        return null;
      },
    },
  };
}

test("language and style writes keep all three prefs fields intact", () => {
  const { bridge, saves } = fakeBridge();
  const seeds: string[] = [];
  const state = createSettingsState(bridge, {
    applyLocale: (lang) => seeds.push(`locale:${lang}`),
    applyTheme: (style) => seeds.push(`theme:${style}`),
    applyBackgroundRun: (enabled) => seeds.push(`bgrun:${enabled}`),
  });
  state.setLanguage("en");
  state.setBackgroundStyle("paper");
  state.setBackgroundRun(true);
  expect(saves.prefs).toHaveLength(3);
  // 三字段整写（prefs.rs save 语义：整文件落盘，无部分写）。
  expect(saves.prefs[0]).toEqual({ language: "en", background_style: "current", background_run: false });
  expect(saves.prefs[1]).toEqual({ language: "en", background_style: "paper", background_run: false });
  expect(saves.prefs[2]).toEqual({ language: "en", background_style: "paper", background_run: true });
  // 播种钩子跟随（set_language / set_mode / background_run 对位）。
  expect(seeds).toEqual(["locale:en", "theme:paper", "bgrun:true"]);
});

test("start() seeds the globals from the stored prefs once", () => {
  const { bridge } = fakeBridge();
  const seeds: string[] = [];
  const state = createSettingsState(bridge, {
    applyLocale: (lang) => seeds.push(`locale:${lang}`),
    applyTheme: (style) => seeds.push(`theme:${style}`),
    applyBackgroundRun: (enabled) => seeds.push(`bgrun:${enabled}`),
  });
  state.start();
  expect(seeds).toEqual(["locale:zh-CN", "theme:current", "bgrun:false"]);
  expect(state.proxyForm().host).toBe("127.0.0.1");
  expect(state.proxyForm().port).toBe("7897");
});

test("proxy blur-save assembles the form: bad port falls back, blanks become null", () => {
  const { bridge, saves } = fakeBridge();
  const state = createSettingsState(bridge);
  state.setProxyExpanded(true);
  state.editProxyField("host", "proxy.lan");
  state.editProxyField("port", "not-a-port");
  state.editProxyField("user", "alice");
  state.editProxyField("pass", "");
  state.editProxyField("bypass", "localhost");
  // 编辑只清状态行，不写盘（InputEvent::Change 对位）。
  expect(saves.proxies).toHaveLength(0);
  state.commitProxy(); // Blur（InputEvent::Blur 对位）。
  expect(saves.proxies).toHaveLength(1);
  const saved = saves.proxies[0];
  expect(saved.port).toBe(7897); // 解析失败回退默认端口（general.rs）。
  expect(saved.host).toBe("proxy.lan");
  expect(saved.username).toBe("alice");
  expect(saved.password).toBeNull(); // 空串落 null（save_proxy 语义）。
  expect(saved.bypass).toBe("localhost");
  expect(state.proxyStatus()).toBe(t("common.saved"));
});

test("proxy toggle and type pick save immediately with the form values", () => {
  const { bridge, saves } = fakeBridge();
  const state = createSettingsState(bridge);
  state.editProxyField("host", "proxy.lan");
  state.editProxyField("port", "1080");
  state.commitProxy({ enabled: true }); // 开关（proxy_enabled 对位）。
  expect(saves.proxies[0].enabled).toBe(true);
  expect(saves.proxies[0].port).toBe(1080); // 表单即时值一并提交。
  state.commitProxy({ proxyType: "socks5h" }); // 类型菜单（proxy_type 对位）。
  expect(saves.proxies[1].proxy_type).toBe("socks5h"); // socks5h 不折叠成 socks5。
  expect(saves.proxies[1].host).toBe("proxy.lan");
});

// ---- host UI（假桥装置；渲染期同步与 JSX 绑定） ------------------------------

test("the settings page mounts every section in order with its rail", async () => {
  const app = await openSettings();
  expect(app.shows("id:settings-window-drag")).toBe(true);
  expect(app.shows("id:settings-toolbar-drag")).toBe(false);
  for (const section of SETTINGS_SECTIONS) {
    await app.waitUntil(() => app.shows(`id:${section.domId}`), { timeout: 2000 });
  }
  // rail 标签态露出 6 大精简分区标题；标题文案来自 locales。
  expect(app.texts()).toContain(t("settings.general"));
  expect(app.texts()).toContain(t("settings.network"));
  expect(app.texts()).toContain(t("settings.translation"));
});

test("sliders and switches persist through the bridge on press", async () => {
  const app = await openSettings();
  await app.press("id:settings-general");
  await app.waitUntil(() => app.shows("id:ui-lang-en"));
  expect(app.shows("id:skill-update-mode-manual")).toBe(true);
  expect(app.shows("id:tray-quota-toggle")).toBe(true);
  await app.press("id:tray-quota-toggle");
  await app.press("id:tray-quota-toggle");
  await app.press("id:ui-lang-en");
  await app.waitUntil(() => app.texts().some((s) => s.includes("prefs:en|current|false")));
  await app.press("id:appearance-paper");
  await app.waitUntil(() => app.texts().some((s) => s.includes("prefs:en|paper|false")));
  await app.press("id:bg-run-toggle");
  await app.waitUntil(() => app.texts().some((s) => s.includes("prefs:en|paper|true")));
});

test("sidebar theme toggle requests persist background_style through settings", async () => {
  const app = await openSettings();
  await app.press("id:emit-theme-paper");
  await app.waitUntil(() => app.texts().some((s) => s.includes("prefs:zh-CN|paper|false")));
  await app.press("id:emit-theme-current");
  await app.waitUntil(() => app.texts().some((s) => s.includes("prefs:zh-CN|current|false")));
});

test("the proxy form expands, its type menu syncs in render, and picks save", async () => {
  const app = await openSettings();
  await app.press("id:settings-network");
  await app.waitUntil(() => app.shows("id:proxy-collapse"));
  expect(app.shows("id:gh-mirror-collapse")).toBe(true);
  expect(app.shows("id:doctor-run")).toBe(true);
  await app.press("id:proxy-collapse");
  await app.waitUntil(() => app.shows("id:proxy-host"));
  // 类型下拉（渲染期同步：开合分支与当前值都在 JSX 里）。
  await app.press("id:proxy-type-button");
  await app.waitUntil(() => app.shows("id:proxy-type-socks5h"));
  await app.press("id:proxy-type-socks5h");
  // 选择即写盘：HTTP://127.0.0.1:7897 落成 socks5h，表单值一并提交。
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("proxy:false|socks5h|127.0.0.1:7897"))
  );
  // 保存状态行出现（proxy_status：成功 ok 色「已保存」）。
  expect(app.texts()).toContain(t("common.saved"));
  // 开关走同一提交路径。
  await app.press("id:proxy-enabled");
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("proxy:true|socks5h|127.0.0.1:7897"))
  );
});

// ---- 真桥只读冒烟（形状；写路径由 native cargo test 沙箱覆盖） ---------------

test("the real bridge answers prefs and proxy with their default shapes", () => {
  const prefs = Settings.loadPrefs() as PrefsDto;
  expect(typeof prefs.language).toBe("string");
  expect(typeof prefs.background_style).toBe("string");
  expect(typeof prefs.background_run).toBe("boolean");
  const proxy = Settings.loadProxy() as ProxyDto;
  expect(["http", "https", "socks5", "socks5h"]).toContain(proxy.proxy_type);
  expect(typeof proxy.port).toBe("number");
});
