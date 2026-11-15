// GitHub 加速分区的无头测试装置（settings.test.host.tsx 样板）：假桥注入
// GhMirrorWorkbench 的 state 工厂位，读数 signal 在假桥调用点直接 set（波
// 1 教训的装置版：不赌模块顶层 effect 的重放）。读数一律「纯 accessor 子」
// 形态（`{readout()}`）——模板字面量拼 signal 的子表达式在本引擎实测不响
// 应（本轮实测：probeCount.set 后 Text 冻在初值），故计数以整串进 signal。
// 探活走可控 Promise，装置按钮按脚本放行最旧一笔（detailDrawer.test.host
// 样板）；「重置」钮把装置拨回初值（同一测试文件的多个 test 共享模块
// 态）。不触文件系统、不写 $HOME——真域路径由 native crate 的 cargo 测试
// 在 SKILLSTAR_DATA_DIR 沙箱覆盖（facades/settings_network.rs）。

import { Text, View, mount, signal, type JSX } from "nsh";
import {
  GhMirrorWorkbench,
  createGhMirrorState,
  type GhMirrorBridge,
  type MirrorConfigDto,
  type MirrorPresetDto,
} from "./ghMirror";

// ---- 假域件 ----------------------------------------------------------------

const PRESETS: MirrorPresetDto[] = [
  { id: "ghproxy_vip", name: "GHProxy.vip", url: "https://ghproxy.vip/", supports_clone: true },
  { id: "gh_proxy_com", name: "GH-Proxy.com", url: "https://gh-proxy.com/", supports_clone: true },
  { id: "gh_llkk", name: "GH LLKK", url: "https://gh.llkk.cc/", supports_clone: true },
  { id: "ghfast_top", name: "GHFast.top", url: "https://ghfast.top/", supports_clone: true },
];

/** 带完整 order 的已保存盘态（custom 槽在链尾）——host 走已保存序；缺文件
 * 的原始默认（order=null 回落 preset 序）由状态机单测覆盖。 */
const INITIAL: MirrorConfigDto = {
  enabled: true,
  preset_id: "ghproxy_vip",
  custom_url: "https://mirror.example",
  order: ["ghproxy_vip", "gh_proxy_com", "gh_llkk", "ghfast_top", "custom"],
};

function cloneConfig(config: MirrorConfigDto): MirrorConfigDto {
  return { ...config, order: config.order === null ? null : [...config.order] };
}

let disk: MirrorConfigDto = cloneConfig(INITIAL);

/** 最近一次写盘 dump（假桥调用点直接 set）。 */
const readout = signal("boot");
/** 在途探活数（整串进 signal，纯 accessor 子渲染）。 */
const probeText = signal("probes:0");

/** 在途探活（装置按钮按脚本放行最旧一笔）。 */
const pendingProbes: {
  url: string;
  resolve: (ms: number) => void;
  reject: (reason: unknown) => void;
}[] = [];

const bridge: GhMirrorBridge = {
  loadConfig() {
    return cloneConfig(disk);
  },
  loadPresets() {
    return PRESETS.map((preset) => ({ ...preset }));
  },
  saveConfig(config) {
    disk = cloneConfig(config);
    readout.set(
      `save:${config.enabled}|${config.preset_id ?? "-"}|${config.custom_url ?? "-"}|${(config.order ?? []).join(",")}`
    );
    return null;
  },
  testMirror(url) {
    return new Promise<number>((resolve, reject) => {
      pendingProbes.push({ url, resolve, reject });
      probeText.set(`probes:${pendingProbes.length}`);
    });
  },
};

const state = createGhMirrorState(bridge);

// ---- 装置按钮（测试按选择器驱动） -------------------------------------------

function releaseProbe(mode: "ok" | "fail"): void {
  const probe = pendingProbes.shift();
  if (!probe) return;
  probeText.set(`probes:${pendingProbes.length}`);
  if (mode === "ok") probe.resolve(123);
  else probe.reject(new Error("mirror down"));
}

function resetDevice(): void {
  disk = cloneConfig(INITIAL);
  pendingProbes.length = 0;
  probeText.set("probes:0");
  readout.set("boot");
  state.start();
}

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", padding: 16 }}>
      <Text testID="mirror-readout" style={{ fontSize: 10 }}>
        {readout()}
      </Text>
      <Text testID="mirror-probes" style={{ fontSize: 10 }}>
        {probeText()}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", gap: 8, marginBottom: 12 }}>
        <DeviceButton label="reset" id="device-reset" onPress={resetDevice} />
        <DeviceButton label="probe-ok" id="device-probe-ok" onPress={() => releaseProbe("ok")} />
        <DeviceButton
          label="probe-fail"
          id="device-probe-fail"
          onPress={() => releaseProbe("fail")}
        />
      </View>
      <GhMirrorWorkbench state={state} />
    </View>
  );
}

function DeviceButton(props: { id: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.id}
      role="button"
      style={{
        paddingHorizontal: 8,
        paddingVertical: 4,
        borderWidth: 1,
        borderColor: "#888",
        cursor: "pointer",
      }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 10 }}>{props.label}</Text>
    </View>
  );
}

export default App;

mount(App);
