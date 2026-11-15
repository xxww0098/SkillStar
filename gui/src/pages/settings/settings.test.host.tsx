// 设置页的无头测试装置（accounts.test.host.tsx 样板）：假桥 + spy 播种
// 钩子注入 SettingsWorkbench 的 state 工厂位，装置按钮换脚本（切语言/
// 切外观/后台运行开关/代理展开与类型选择/代理开关），读数显示写盘调用
// 的最新参数供 app.texts() 断言。不触文件系统、不写 $HOME——真域路径由
// native crate 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖（facades/
// settings.rs 的 prefs/proxy round-trip）。

import { Text, View, mount, signal, type JSX } from "nsh";
import { requestThemeMode } from "../../shell/events";
import { SettingsWorkbench } from "./index";
import { createSettingsState, type SettingsSeedHooks } from "./state";
import type { PrefsDto, ProxyDto, SettingsBridge } from "./bridge";

// ---- 假桥：内存态 + 调用记录 ---------------------------------------------

let diskPrefs: PrefsDto = {
  language: "zh-CN",
  background_style: "current",
  background_run: false,
};
let diskProxy: ProxyDto = {
  enabled: false,
  proxy_type: "http",
  host: "127.0.0.1",
  port: 7897,
  username: null,
  password: null,
  bypass: null,
};

/** 最近一次写盘调用的 dump（假桥调用点直接写 signal——不赌模块顶层
 * effect 的重放语义，波 1 教训的装置版：普通变量不触发重渲染）。 */
const readout = signal("none none");
let lastPrefs: string = "none";
let lastProxy: string = "none";

function refreshReadout(): void {
  readout.set(`${lastPrefs} ${lastProxy}`);
}

const fakeBridge: SettingsBridge = {
  loadPrefs() {
    return { ...diskPrefs };
  },
  savePrefs(prefs) {
    diskPrefs = { ...prefs };
    lastPrefs = `prefs:${prefs.language}|${prefs.background_style}|${prefs.background_run}`;
    refreshReadout();
    return null;
  },
  loadProxy() {
    return { ...diskProxy };
  },
  saveProxy(proxy) {
    diskProxy = { ...proxy };
    lastProxy = `proxy:${proxy.enabled}|${proxy.proxy_type}|${proxy.host}:${proxy.port}|u=${proxy.username}|p=${proxy.password}|b=${proxy.bypass}`;
    refreshReadout();
    return null;
  },
};

/** spy 播种：记录全局 locale/theme 的应用，不真改全局 signal。 */
const seedLog: string[] = [];
const spySeeds: SettingsSeedHooks = {
  applyLocale(language) {
    seedLog.push(`locale:${language}`);
  },
  applyTheme(style) {
    seedLog.push(`theme:${style}`);
  },
};

const state = createSettingsState(fakeBridge, spySeeds);

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <Readout />
      <View style={{ display: "flex", flexDirection: "row", flex: 1, minHeight: 0 }}>
        <View style={{ display: "flex", flex: 1, minWidth: 0 }}>
          <SettingsWorkbench state={state} />
        </View>
      </View>
    </View>
  );
}

function Readout(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
      <Text testID="host-readout" style={{ fontSize: 10 }}>
        {readout()}
      </Text>
      <View
        testID="emit-theme-paper"
        style={{ width: 8, height: 8 }}
        onPress={() => requestThemeMode.publish("paper")}
      />
      <View
        testID="emit-theme-current"
        style={{ width: 8, height: 8 }}
        onPress={() => requestThemeMode.publish("current")}
      />
    </View>
  );
}

export default App;

mount(App);
