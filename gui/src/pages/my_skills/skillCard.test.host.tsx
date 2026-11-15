import { Text, View, mount, type JSX } from "nsh";
import { SkillCard } from "./skillCard";
import { busy, selectedSkill } from "./state";
import type { SkillRecord } from "./filters";

// 技能卡副标题（来源 chip）的无头装置：卡身点击与来源点击的落点必须分开
// ——来源点击经系统浏览器外开，卡身点击才开详情抽屉。
//
// 装置纪律（同 detailDrawer.test.host.tsx）：state.busy 先置 hold，装载尾
// 的后台更新检查被挡住，本机不发真实网络检查。

const LINKED: SkillRecord = {
  name: "chief-of-staff",
  description: "Pursue a long-running goal in a single session.",
  skillType: "hub",
  stars: 0,
  installed: true,
  updateAvailable: false,
  upstreamChange: null,
  lastUpdated: "",
  gitUrl: "https://github.com/mattpocock/skills",
  author: "mattpocock",
  agentLinks: [],
  source: "mattpocock/skills",
};

busy.set("card-fixture-hold");

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12, padding: 16 }}>
      <View style={{ display: "flex", flexDirection: "row" }}>
        <SkillCard skill={() => LINKED} listMode={false} />
      </View>
      <Text testID="card-selected">{`selected:${selectedSkill() ?? "none"}`}</Text>
    </View>
  );
}

mount(App);
