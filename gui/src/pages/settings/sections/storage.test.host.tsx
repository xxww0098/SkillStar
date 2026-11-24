// 「存储」分区的无头测试装置（settings.test.host.tsx 样板）：假桥注入
// StorageWorkbench 的 state 工厂位（overview/preview/force 全部立即
// resolve 固定夹具）；读数 signal 在 effect 里从 state 派生（confirm/busy/
// status 的追踪面）。不触文件系统、不写 $HOME——真域路径由 native crate
// 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖。

import { Text, View, effect, mount, signal, type JSX } from "nsh";
import { StorageWorkbench, createStorageState, type StorageBridge } from "./storage";

const fakeBridge: StorageBridge = {
  async storageOverview() {
    return {
      data_root_path: "/data",
      hub_root_path: "/data/agents",
      is_hub_under_data: true,
      config_bytes: 10,
      config_path: "/data/config",
      skills_data_path: "/data/data/skills",
      hub_bytes: 20,
      hub_path: "/data/agents/skills",
      hub_count: 2,
      broken_count: 0,
      local_count: 0,
      local_bytes: 0,
      local_path: "/data/agents/local",
      cache_bytes: 0,
      cache_path: "/data/agents/repos",
      cache_count: 0,
      cache_unused_count: 0,
      cache_unused_bytes: 0,
      health_issue_count: 0,
      health_repairable: 0,
      intake: [],
      history_count: 0,
    };
  },
  async clearAllCaches() {
    return { repos_removed: 0, history_cleared: 0 };
  },
  async previewHubDelete() {
    return ["owned-skill"];
  },
  async forceDeleteHub() {
    return { removed: ["owned-skill"], failed: [], kept: ["manual-skill"] };
  },
  async forceDeleteCaches() {
    return 0;
  },
  async previewRepair() {
    return { issues: 0, steps: [], untouched: [] };
  },
  async applyRepair() {
    return { applied: 0, failed: [] };
  },
  async previewIntake() {
    return { steps: [], reported: [], would_apply: 0 };
  },
  async applyIntake() {
    return { applied: 0, failed: [] };
  },
  openFolder() {
    /* fire-and-forget */
  },
};

const state = createStorageState(fakeBridge);

/** 派生读数：行数 + armed 相 + 状态行（effect 追踪 confirm/busy/status）。 */
const readout = signal("none");
effect(() => {
  const confirm = state.storageConfirm() ?? "none";
  const status = state.storageStatus() ?? "none";
  readout.set(`storage:${state.storage()?.hub_count ?? "-"} confirm:${confirm} status:${status}`);
});

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <Text testID="host-readout" style={{ fontSize: 10 }}>
        {readout()}
      </Text>
      <StorageWorkbench state={state} />
    </View>
  );
}

export default App;

mount(App);
