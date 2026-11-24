// 「关于」分区的无头测试装置（storage.test.host.tsx 同款纪律）：假桥注入
// AboutWorkbench 的 state 工厂位，装置按钮切换环境脚本（Git 装与未装、gh
// 装与未装、release 记录四种 outcome）并放行手动检查的受控 Promise；读数
// signal 在事件回调点直接 set。不触文件系统、不写 $HOME——真域路径
// （check_git_status / last_record / run_release_check 的 cooldown 短路）由
// native crate 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖。

import { Text, View, mount, signal, type JSX } from "nsh";
import {
  AboutWorkbench,
  createAboutState,
  type AboutBridge,
  type AboutSnapshotDto,
  type ReleaseRecordDto,
} from "./about";

// ---- 环境脚本 ---------------------------------------------------------------

type Scenario = "ok" | "nogit";

function snapshotOf(scenario: Scenario): AboutSnapshotDto {
  if (scenario === "ok") {
    return {
      version: "1.2.3",
      data_root: "/opt/skillstar-data",
      gh_installed: true,
      gh_install_command: "brew install gh",
      git_status: { status: "Installed", version: "2.44.0" },
      release_check: {
        last_checked_unix: 1,
        current_version: "1.2.3",
        outcome: { kind: "available", tag: "v1.3.0", url: "https://example.com/v1.3.0" },
      },
    };
  }
  return {
    version: "1.2.3",
    data_root: "/opt/skillstar-data",
    gh_installed: false,
    gh_install_command: "brew install gh",
    git_status: {
      status: "NotInstalled",
      os: "macOS",
      install_instructions: [
        { label: "Homebrew", command: "brew install git" },
        { label: "Conda", command: "conda install git" },
      ],
      download_url: "https://git-scm.com/downloads",
    },
    release_check: null,
  };
}

// ---- 假桥 + 装置状态 ----------------------------------------------------------

let scenario: Scenario = "ok";
const envText = signal("env:ok");
const checkText = signal("calls:0");

/** 手动检查的受控 Promise（装置按钮放行）。 */
let pendingCheck: { resolve: (record: ReleaseRecordDto) => void } | null = null;

const bridge: AboutBridge = {
  aboutSnapshot() {
    const snap = snapshotOf(scenario);
    return new Promise<AboutSnapshotDto>((resolve) => {
      setTimeout(() => resolve(snap), 0);
    });
  },
  runReleaseCheck() {
    // 计数按 ":" 之后解析（前缀只是展示），装置重放安全。
    const calls = Number(checkText().slice(checkText().indexOf(":") + 1)) + 1;
    checkText.set(`calls:${calls}`);
    return new Promise<ReleaseRecordDto>((resolve) => {
      pendingCheck = { resolve };
    });
  },
  openFolder() {
    /* fire-and-forget */
  },
};

const state = createAboutState(bridge);

function resetDevice(): void {
  scenario = "ok";
  pendingCheck = null;
  envText.set("env:ok");
  checkText.set("calls:0");
  // 读盘值直接重放（构造函数 load 对位）。
  void state.start();
}

function App(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        padding: 16,
      }}
    >
      <Text testID="about-env" style={{ fontSize: 10 }}>
        {envText()}
      </Text>
      <Text testID="about-checks" style={{ fontSize: 10 }}>
        {checkText()}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", gap: 8, marginBottom: 12 }}>
        <DeviceButton label="reset" id="device-reset" onPress={resetDevice} />
        <DeviceButton
          label="nogit"
          id="device-nogit"
          onPress={() => {
            scenario = "nogit";
            envText.set("env:nogit");
            void state.start();
          }}
        />
        <DeviceButton
          label="check-ok"
          id="device-check-ok"
          onPress={() => {
            const pending = pendingCheck;
            pendingCheck = null;
            pending?.resolve({
              last_checked_unix: 9,
              current_version: "1.2.3",
              outcome: { kind: "up_to_date" },
            });
          }}
        />
      </View>
      <AboutWorkbench state={state} />
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
