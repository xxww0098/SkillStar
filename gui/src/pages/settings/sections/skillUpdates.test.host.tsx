// 「技能更新」分区的无头测试装置（settings.test.host.tsx 样板）：假桥注入
// SkillUpdatesWorkbench 的 state 工厂位；读数 signal 在假桥调用点直接 set
// （不赌模块顶层 effect 的重放，波 1 教训的装置版）。不触文件系统、不写
// $HOME——真域路径由 native crate 的 cargo 测试在 SKILLSTAR_DATA_DIR 沙箱
// 覆盖（facades/settings_updates.rs 的 round-trip）。

import { Text, View, mount, signal, type JSX } from "nsh";
import {
  SkillUpdatesWorkbench,
  createSkillUpdatesState,
  type SkillUpdatesBridge,
  type SkillUpdatesDto,
} from "./skillUpdates";

// ---- 假桥：内存态 + 调用记录 ---------------------------------------------

let disk: SkillUpdatesDto = { auto_update: false, interval_minutes: 60 };

/** 最近一次写盘调用的 dump（假桥调用点直接写 signal）。 */
const readout = signal("none");

const fakeBridge: SkillUpdatesBridge = {
  load() {
    return { ...disk };
  },
  save(next) {
    disk = { ...next };
    readout.set(`updates:${next.auto_update}|${next.interval_minutes}`);
    return null;
  },
};

const state = createSkillUpdatesState(fakeBridge);

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <Text testID="host-readout" style={{ fontSize: 10 }}>
        {readout()}
      </Text>
      <SkillUpdatesWorkbench state={state} />
    </View>
  );
}

export default App;

mount(App);
