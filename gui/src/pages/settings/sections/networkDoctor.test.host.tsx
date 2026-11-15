// 网络诊断分区的无头测试装置（ghMirror.test.host.tsx 同款）：假桥注入
// DoctorWorkbench 的 state 工厂位（诊断不打真网——真域路径由 native crate
// 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱按 wire-shape 桩覆盖）。诊断走可
// 控 Promise，装置按钮按脚本放行；读数 signal 在假桥调用点直接 set。

import { Text, View, mount, signal, type JSX } from "nsh";
import {
  DoctorWorkbench,
  createDoctorState,
  type DiagnosisDto,
  type DoctorBridge,
} from "./networkDoctor";

// ---- 假域件 ----------------------------------------------------------------

/** wire-shape 桩：三种状态的 check 各一枚 + 已知/未知两条建议键。 */
function stubDiagnosis(): DiagnosisDto {
  return {
    proxy_enabled: false,
    proxy_type: null,
    checks: [
      {
        id: "github",
        label: "GitHub",
        url: "https://github.com/",
        status: "ok",
        latency_ms: 123,
        detail: "HTTP 200",
      },
      {
        id: "api_github",
        label: "GitHub API",
        url: "https://api.github.com/",
        status: "fail",
        latency_ms: null,
        detail: "HTTP 403",
      },
      {
        id: "proxy",
        label: "Proxy",
        url: "",
        status: "skip",
        latency_ms: null,
        detail: "proxy disabled",
      },
    ],
    recommendations: ["enable_github_mirrors", "unmapped_key_x"],
  };
}

/** 诊断调用数（普通变量镜像进 signal，整串形态、纯 accessor 子渲染——
 * ghMirror 装置同款，避免模板字面量拼 signal 的子表达式不响应）。 */
const callText = signal("calls:0");
let calls = 0;

/** 在途诊断（装置按钮按脚本放行）。 */
let pendingDiagnose: {
  resolve: (diagnosis: DiagnosisDto) => void;
  reject: (reason: unknown) => void;
} | null = null;

const bridge: DoctorBridge = {
  diagnose() {
    calls += 1;
    callText.set(`calls:${calls}`);
    return new Promise<DiagnosisDto>((resolve, reject) => {
      pendingDiagnose = { resolve, reject };
    });
  },
};

const state = createDoctorState(bridge);

// ---- 装置按钮 ---------------------------------------------------------------

function resetDevice(): void {
  pendingDiagnose = null;
  calls = 0;
  callText.set("calls:0");
}

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", padding: 16 }}>
      <Text testID="doctor-calls" style={{ fontSize: 10 }}>
        {callText()}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", gap: 8, marginBottom: 12 }}>
        <DeviceButton label="reset" id="device-reset" onPress={resetDevice} />
        <DeviceButton
          label="ok"
          id="device-ok"
          onPress={() => {
            const pending = pendingDiagnose;
            pendingDiagnose = null;
            pending?.resolve(stubDiagnosis());
          }}
        />
        <DeviceButton
          label="err"
          id="device-err"
          onPress={() => {
            const pending = pendingDiagnose;
            pendingDiagnose = null;
            pending?.reject(new Error("boom: offline"));
          }}
        />
      </View>
      <DoctorWorkbench state={state} />
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
