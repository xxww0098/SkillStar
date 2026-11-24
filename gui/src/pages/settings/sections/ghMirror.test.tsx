// GitHub 加速分区测试 — gh_mirror.rs 的语义对位：状态机（order 推导/选择
// 置首/邻位交换/收起保存/URL trim/探活单飞）+ host 装置上的 UI 交互（折叠、
// 行选择、排序箭头、开关、custom 输入提交、测试按钮记忆）。真域读写由
// native crate 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖（facades/
// settings_network.rs）。不写真实 $HOME、不打真网（探活走可控桩）。
import { expect, open, test } from "nsh/test";
import { t } from "../../../services/translation";
import {
  CUSTOM_ENTRY_ID,
  createGhMirrorState,
  type GhMirrorBridge,
  type MirrorConfigDto,
  type MirrorPresetDto,
} from "./ghMirror";

const HOST = "./ghMirror.test.host.tsx";

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
  }
}

// ---- 假桥（状态机单测） ------------------------------------------------------

const PRESETS: MirrorPresetDto[] = [
  { id: "ghproxy_vip", name: "GHProxy.vip", url: "https://ghproxy.vip/", supports_clone: true },
  { id: "gh_proxy_com", name: "GH-Proxy.com", url: "https://gh-proxy.com/", supports_clone: true },
  { id: "gh_llkk", name: "GH LLKK", url: "https://gh.llkk.cc/", supports_clone: true },
  { id: "ghfast_top", name: "GHFast.top", url: "https://ghfast.top/", supports_clone: true },
];

interface MirrorHarness {
  saves: MirrorConfigDto[];
  probes: { url: string; resolve: (ms: number) => void; reject: (reason: unknown) => void }[];
  state: ReturnType<typeof createGhMirrorState>;
}

function harness(initial: MirrorConfigDto): MirrorHarness {
  const saves: MirrorConfigDto[] = [];
  const probes: MirrorHarness["probes"] = [];
  let disk = initial;
  const bridge: GhMirrorBridge = {
    loadConfig: () => ({ ...disk, order: disk.order === null ? null : [...disk.order] }),
    loadPresets: () => PRESETS.map((preset) => ({ ...preset })),
    saveConfig(config) {
      disk = { ...config, order: config.order === null ? null : [...config.order] };
      saves.push({ ...config, order: config.order === null ? null : [...config.order] });
      return null;
    },
    testMirror(url) {
      return new Promise<number>((resolve, reject) => {
        probes.push({ url, resolve, reject });
      });
    },
  };
  return { saves, probes, state: createGhMirrorState(bridge) };
}

const RAW_DEFAULT: MirrorConfigDto = {
  enabled: false,
  preset_id: "ghproxy_vip",
  custom_url: null,
  order: null,
};

// ---- 状态机（gh_mirror.rs 的读写语义） ---------------------------------------

test("a null order (raw missing-file default) falls back to the preset chain with custom slot", () => {
  const { state } = harness(RAW_DEFAULT);
  // gh_mirror_order 对位：缺文件默认也应包含 custom 槽位。
  expect(state.order()).toEqual(["ghproxy_vip", "gh_proxy_com", "gh_llkk", "ghfast_top", "custom"]);
  expect(state.entries().map((entry) => entry.id)).toEqual([
    "ghproxy_vip",
    "gh_proxy_com",
    "gh_llkk",
    "ghfast_top",
    "custom",
  ]);
  expect(state.selectedId()).toBe("ghproxy_vip");
  // 分区头 chip 读内存 order 的生效 URL（不读盘）。
  expect(state.effectiveUrl()).toBe("https://ghproxy.vip/");
});

test("selecting a row moves it to the front; preset_id follows order[0]", () => {
  const { state, saves } = harness(RAW_DEFAULT);
  state.selectEntry("gh_llkk");
  expect(saves).toHaveLength(1);
  expect(saves[0].order).toEqual(["gh_llkk", "ghproxy_vip", "gh_proxy_com", "ghfast_top", "custom"]);
  expect(saves[0].preset_id).toBe("gh_llkk");
  // 选中 custom 槽：preset_id 落 null（save_gh_mirror 的 filter 语义）。
  state.selectEntry(CUSTOM_ENTRY_ID);
  expect(saves[1].order?.[0]).toBe(CUSTOM_ENTRY_ID);
  expect(saves[1].preset_id).toBeNull();
  // custom 槽的生效 URL 读内存 custom_url。
  expect(state.effectiveUrl()).toBe("");
});

test("reorder arrows swap neighbours and ignore the boundary", () => {
  const { state, saves } = harness(RAW_DEFAULT);
  state.moveEntry("ghproxy_vip", -1); // 已在链首：不动、不写盘。
  expect(saves).toHaveLength(0);
  state.moveEntry(CUSTOM_ENTRY_ID, 1); // 已在链尾：不动、不写盘。
  expect(saves).toHaveLength(0);
  state.moveEntry("ghproxy_vip", 1); // 与邻位交换。
  expect(saves).toHaveLength(1);
  expect(saves[0].order).toEqual(["gh_proxy_com", "ghproxy_vip", "gh_llkk", "ghfast_top", "custom"]);
});

test("collapse-close commits the custom URL from the form, trimmed", () => {
  const initial: MirrorConfigDto = {
    enabled: false,
    preset_id: null,
    custom_url: "https://old.example",
    order: [CUSTOM_ENTRY_ID, "ghproxy_vip", "gh_proxy_com", "gh_llkk", "ghfast_top"],
  };
  const { state, saves } = harness(initial);
  state.editCustomUrl("  https://mirror.example  ");
  state.setExpanded(true); // 展开不写盘。
  expect(saves).toHaveLength(0);
  state.setExpanded(false); // 收起分支保存（gh_mirror.rs closing 对位）。
  expect(saves).toHaveLength(1);
  expect(saves[0].custom_url).toBe("https://mirror.example");
  // 空白串落 null；编辑清状态行（InputEvent::Change 对位）。
  state.save();
  expect(state.status()).toBe(t("common.saved"));
  state.editCustomUrl("   ");
  expect(state.status()).toBeNull();
  state.setExpanded(false);
  expect(saves[saves.length - 1].custom_url).toBeNull();
});

test("the enable toggle persists through the bridge", () => {
  const { state, saves } = harness(RAW_DEFAULT);
  state.toggle();
  expect(saves[0].enabled).toBe(true);
  expect(saves[0].preset_id).toBe("ghproxy_vip");
});

test("probe runs are single-flight and results are remembered per row", async () => {
  const { state, probes } = harness(RAW_DEFAULT);
  state.runTest("ghproxy_vip", "https://ghproxy.vip/");
  expect(state.testingId()).toBe("ghproxy_vip");
  state.runTest("gh_proxy_com", "https://gh-proxy.com/"); // 单飞：第二笔吞掉。
  expect(probes).toHaveLength(1);
  probes[0].resolve(123);
  await settle();
  expect(state.testingId()).toBeNull();
  expect(state.testResults()["ghproxy_vip"]).toBe(123);
  // 空地址不可测（can_test 对位）。
  state.runTest("custom", "");
  expect(probes).toHaveLength(1);
  // 失败记忆为 "fail"。
  state.runTest("gh_llkk", "https://gh.llkk.cc/");
  expect(probes).toHaveLength(2);
  probes[1].reject(new Error("down"));
  await settle();
  expect(state.testResults()["gh_llkk"]).toBe("fail");
});

// ---- host UI（装置交互；渲染期同步与 JSX 绑定） -------------------------------

async function openMirror() {
  const app = await open(HOST);
  await app.press("id:device-reset");
  return app;
}

test("the section renders the chip and expands the chain on collapse press", async () => {
  const app = await openMirror();
  // 分区头 chip：enabled 且生效 URL 非空（读内存 order）。
  await app.waitUntil(() => app.texts().some((s) => s.includes("https://ghproxy.vip/")));
  // 折叠头展开表单：说明 + 全部行（含 custom 槽）。
  await app.press("id:gh-mirror-collapse");
  await app.waitUntil(() => app.shows("id:gh-entry-ghproxy_vip"));
  expect(app.shows("id:gh-entry-custom")).toBe(true);
  expect(app.texts()).toContain(t("settings.githubMirrorNotice"));
  expect(app.texts()).toContain("GH-Proxy.com");
});

test("selecting a row saves it to the front with the derived preset_id", async () => {
  const app = await openMirror();
  await app.press("id:gh-mirror-collapse");
  await app.press("id:gh-entry-gh_llkk");
  await app.waitUntil(() =>
    app.texts().some((s) => s.startsWith("save:true|gh_llkk|https://mirror.example|gh_llkk,"))
  );
});

test("the enable switch and reorder arrows persist through the bridge", async () => {
  const app = await openMirror();
  await app.press("id:gh-mirror-collapse");
  await app.press("id:gh-down-ghproxy_vip");
  await app.waitUntil(() =>
    app.texts().some((s) =>
      s.startsWith("save:true|gh_proxy_com|") && s.includes("gh_proxy_com,ghproxy_vip,")
    )
  );
  await app.press("id:gh-mirror-enabled");
  await app.waitUntil(() => app.texts().some((s) => s.startsWith("save:false|")));
});

test("the custom URL input commits on collapse close", async () => {
  const app = await openMirror();
  await app.press("id:gh-mirror-collapse");
  // 选中 custom 槽 → 受控输入框出现（gh-url-custom）。
  await app.press("id:gh-entry-custom");
  await app.waitUntil(() => app.shows("id:gh-url-custom"));
  await app.type("id:gh-url-custom", "https://fast.mirror/");
  // 折叠收起 = closing 分支保存（表单值随提交）。
  await app.press("id:gh-mirror-collapse");
  await app.waitUntil(() => app.texts().some((s) => s.includes("https://fast.mirror/")));
});

test("the test button resolves and fails through the row memory", async () => {
  const app = await openMirror();
  await app.press("id:gh-mirror-collapse");
  await app.press("id:gh-test-ghproxy_vip");
  await app.waitUntil(() => app.texts().some((s) => s === "probes:1"));
  await app.press("id:device-probe-ok");
  await app.waitUntil(() => app.texts().some((s) => s === "123ms"));
  await app.press("id:gh-test-gh_proxy_com");
  await app.press("id:device-probe-fail");
  await app.waitUntil(() => app.texts().some((s) => s === t("settings.mirrorTestFail")));
});
