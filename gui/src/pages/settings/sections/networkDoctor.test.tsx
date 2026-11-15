// 网络诊断分区测试 — network_doctor.rs 的语义对位：状态机（单飞门、resolve
// 落 diagnosis / reject 落 error）+ 推荐键映射（recommendation() 对位，未
// 知键透传）+ host 装置上的 UI 交互（doctor-run 按钮、check 行的三态
// chip、建议框）。诊断打真网——单测不打：host 走 wire-shape 桩，真域路径
// 由 native crate 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖（facades/
// settings_network.rs 的 diagnosis_wire_shape_over_a_stub）。不写 $HOME。
import { expect, open, test } from "nsh/test";
import { t } from "../../../services/translation";
import {
  createDoctorState,
  recommendationKey,
  type DiagnosisDto,
  type DoctorBridge,
} from "./networkDoctor";

const HOST = "./networkDoctor.test.host.tsx";

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
  }
}

// ---- 纯映射（recommendation() 对位） ----------------------------------------

test("recommendation keys map to their i18n keys, unknown keys pass through", () => {
  expect(recommendationKey("enable_github_mirrors")).toBe(
    "settings.networkDoctorRecEnableMirrors"
  );
  expect(recommendationKey("enable_proxy")).toBe("settings.networkDoctorRecEnableProxy");
  expect(recommendationKey("use_socks5h")).toBe("settings.networkDoctorRecUseSocks5h");
  expect(recommendationKey("check_proxy_reachability")).toBe(
    "settings.networkDoctorRecCheckProxy"
  );
  expect(recommendationKey("use_marketplace_wrap")).toBe(
    "settings.networkDoctorRecMarketplaceWrap"
  );
  expect(recommendationKey("all_github_paths_blocked")).toBe(
    "settings.networkDoctorRecAllBlocked"
  );
  // GPUI other => other：未知键原样透传。
  expect(recommendationKey("future_key")).toBe("future_key");
});

// ---- 状态机（run_diagnosis 对位） --------------------------------------------

interface DoctorHarness {
  calls: number;
  resolve: (diagnosis: DiagnosisDto) => void;
  reject: (reason: unknown) => void;
  state: ReturnType<typeof createDoctorState>;
}

function harness(): DoctorHarness {
  const handle = {
    calls: 0,
    resolve: (_diagnosis: DiagnosisDto) => {},
    reject: (_reason: unknown) => {},
  } as DoctorHarness;
  const bridge: DoctorBridge = {
    diagnose() {
      handle.calls += 1;
      return new Promise<DiagnosisDto>((resolve, reject) => {
        handle.resolve = resolve;
        handle.reject = reject;
      });
    },
  };
  handle.state = createDoctorState(bridge);
  return handle;
}

test("run is single-flight: a second press while diagnosing is swallowed", async () => {
  const machine = harness();
  machine.state.run();
  expect(machine.state.diagnosing()).toBe(true);
  machine.state.run(); // diagnosing 门吞掉。
  expect(machine.calls).toBe(1);
  machine.resolve({
    proxy_enabled: false,
    proxy_type: null,
    checks: [],
    recommendations: [],
  });
  await settle();
  expect(machine.state.diagnosing()).toBe(false);
  expect(machine.state.diagnosis()).not.toBeNull();
  expect(machine.state.error()).toBeNull();
});

test("a rejected diagnosis lands in the error line and clears the busy flag", async () => {
  const machine = harness();
  machine.state.run();
  machine.reject(new Error("probe blew up"));
  await settle();
  expect(machine.state.diagnosing()).toBe(false);
  expect(machine.state.error()).toContain("probe blew up");
  expect(machine.state.diagnosis()).toBeNull();
  // 失败后可重跑（单飞门只在运行中生效）。
  machine.state.run();
  expect(machine.calls).toBe(2);
});

// ---- host UI（装置交互；wire-shape 桩） --------------------------------------

async function openDoctor() {
  const app = await open(HOST);
  await app.press("id:device-reset");
  return app;
}

test("doctor-run probes once, swallows double presses and paints every check", async () => {
  const app = await openDoctor();
  await app.press("id:doctor-run");
  await app.waitUntil(() => app.texts().some((s) => s === "calls:1"));
  // 运行中再点被单飞门吞掉。
  await app.press("id:doctor-run");
  await app.press("id:device-ok");
  // check 行：标签 + URL + detail；chip 三态（ok 带延迟 / fail / skip 透传）。
  await app.waitUntil(() => app.texts().some((s) => s === "123ms"));
  expect(app.texts()).toContain("GitHub");
  expect(app.texts()).toContain("https://github.com/");
  expect(app.texts()).toContain("HTTP 403");
  expect(app.texts()).toContain("fail");
  expect(app.texts()).toContain("skip");
  expect(app.texts()).toContain("proxy disabled");
  // 建议框：标题 + 已知键映射文案（行前缀 "• "）+ 未知键透传。
  expect(app.texts()).toContain(t("settings.networkDoctorRecommendations"));
  const recText = t("settings.networkDoctorRecEnableMirrors");
  expect(app.texts().some((s) => s.includes(recText))).toBe(true);
  expect(app.texts().some((s) => s.includes("unmapped_key_x"))).toBe(true);
  // 整个过程只有一笔诊断（双击被吞）。
  expect(app.texts().some((s) => s === "calls:1")).toBe(true);
});

test("a failed diagnosis shows the error line verbatim", async () => {
  const app = await openDoctor();
  await app.press("id:doctor-run");
  await app.press("id:device-err");
  await app.waitUntil(() => app.texts().some((s) => s.includes("boom: offline")));
  // 提示行仍在（卡面常驻说明）。
  expect(app.texts()).toContain(t("settings.networkDoctorHint"));
});
