// 「智能体连接」分区的无头测试装置（settings.test.host.tsx 样板）：假桥注入
// AgentConnectionsWorkbench 的 state 工厂位；读数在 Readout 组件的 JSX 表达
// 式里从 state 派生（渲染期跟随；模块顶层 effect 的重放不可靠——波 6 台账
// 的装置纪律），装置假桥的 managed 读写全部立即 resolve。不触文件系统、
// 不写 $HOME——真域路径由 native crate 的 cargo 测试在沙箱覆盖
// （facades/settings_agent_connections.rs）。

import { Text, View, mount, type JSX } from "nsh";
import {
  AgentConnectionsWorkbench,
  createAgentConnectionsState,
  statusCounts,
  type AgentConnectionsBridge,
  type AgentProfileDto,
} from "./agentConnections";

// ---- 假桥：内存态 + 立即 resolve --------------------------------------------

const profiles: AgentProfileDto[] = [
  {
    id: "beta",
    display_name: "Beta",
    icon: "",
    global_skills_dir: "/hub/beta/skills",
    project_skills_rel: "",
    installed: true,
    enabled: true,
    synced_count: 1,
  },
  {
    id: "alpha",
    display_name: "Alpha",
    icon: "",
    global_skills_dir: "/hub/alpha/skills",
    project_skills_rel: "",
    installed: true,
    enabled: false,
    synced_count: 0,
  },
  {
    id: "gamma",
    display_name: "Gamma",
    icon: "",
    global_skills_dir: "",
    project_skills_rel: ".gamma/skills",
    installed: true,
    enabled: false,
    synced_count: 0,
  },
];

const fakeBridge: AgentConnectionsBridge = {
  listProfiles() {
    return new Promise<AgentProfileDto[]>((resolve) => {
      setTimeout(() => resolve(profiles.map((item) => ({ ...item }))), 0);
    });
  },
  toggleProfile(id) {
    return new Promise<boolean>((resolve, reject) => {
      setTimeout(() => {
        const found = profiles.find((item) => item.id === id);
        if (found === undefined) {
          reject(new Error("unknown agent"));
          return;
        }
        found.enabled = !found.enabled;
        resolve(found.enabled);
      }, 0);
    });
  },
  listLinkedSkills() {
    return new Promise<string[]>((resolve) => {
      setTimeout(() => resolve([]), 0);
    });
  },
  async unlinkSkill() {
    /* the host suite never unlinks */
  },
  async unlinkAll() {
    return 0;
  },
  // managed 读写经 setTimeout 延迟 resolve（宏任务，渲染帧之后落地）——
  // 纯微任务 promise 在无头渲染器里与首帧渲染重入会卡死（真桥的 native
  // 调用本来就是跨帧回调；实验证实挂起的读不崩、立即 resolve 崩）。
  async getManagedState(agentId) {
    const managed =
      agentId === "beta"
        ? { active_skill_names: ["writer"], suspended_skill_names: [] }
        : { active_skill_names: [], suspended_skill_names: [] };
    return new Promise<typeof managed>((resolve) => {
      setTimeout(() => resolve(managed), 0);
    });
  },
  async toggleManaged() {
    const report = {
      action: "paused" as const,
      state: { active_skill_names: [], suspended_skill_names: ["writer"] },
      succeeded: ["writer"],
      skipped: [],
      failed: [],
    };
    return new Promise<typeof report>((resolve) => {
      setTimeout(() => resolve(report), 0);
    });
  },
};

const state = createAgentConnectionsState(fakeBridge);

/** 读数派生（组件 JSX 表达式里调用，signal 读取落在追踪面上）。 */
function readoutText(): string {
  const counts = statusCounts(state.profiles(), state.agentQuery());
  const pill =
    state.agentFilter() === "all" ? 0 : state.agentFilter() === "enabled" ? 1 : 2;
  return `agents:${counts[0]}|${counts[1]}|${counts[pill]} expanded:${state.expandedAgent() ?? "none"}`;
}

/** 读数组件：派生量在 JSX 表达式里求值（渲染期跟随；组件体内的 const
 * 求值会冻死——波 1 教训；模块顶层 effect 的重放也不可靠——波 6 台账）。 */
function Readout(): JSX.Element {
  return (
    <Text testID="host-readout" style={{ fontSize: 10 }}>
      {readoutText()}
    </Text>
  );
}

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <Readout />
      <AgentConnectionsWorkbench state={state} />
    </View>
  );
}

export default App;

mount(App);
