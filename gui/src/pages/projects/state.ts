// 项目页状态机 — crates/ss-gpui/src/projects/ 的 mod.rs（页面实体与
// set_active_project/select_project/toggle_*）+ actions.rs（保存与看护
// 动作）+ upkeep.rs（refresh_copy_status）对位。
//
// 工厂形态 createProjectsState(bridge)：页面用真桥建一份，测试装置注入
// 假桥建自己的（accounts/state.ts 同款）；组件只拿这份 state，不碰桥。
//
// 【并发纪律】与 GPUI 相同的两把「代际门」：
// - copy_epoch（upkeep.rs）：每次激活项目 / 开始变更 / 重新检查都自增，
//   过期的 copy_status 回包整包丢弃——它带的是旧项目的事实；
// - busy/dirty 互斥：busy 期间一切编辑与切换静默拒绝（GPUI 各入口的
//   `if self.busy { return; }`）；dirty 期间看护动作（重试/清理）拒绝，
//   弹窗一律不开（dialogs.rs mutation_blocked）。
// 【副本回填】copy_status 成功且 !dirty && !busy 时，把快照里的
// agents/deploy_modes/选中项回填进工作态（upkeep.rs:46-50）；dirty 时只
// 更新记录与规则，编辑不动。
// 【nsh 纪律】组件体只执行一次：派生读取（visibleAgents/canApply…）是
// 普通 getter，调用点必须在 JSX 表达式里。

import { signal, type Signal } from "nsh";
import type {
  AgentProfileRecord,
  CopyRecord,
  CopyStateTag,
  DeployMode,
  ProjectEntry,
  ProjectsBridge,
} from "./bridge";
import { publishToast } from "../../shell/events";
import { agentsChanged } from "../../shell/events";
import { tf } from "../../services/translation";

export interface ProjectsState {
  // ---- 实体信号（ProjectsPage 字段一一对位） ------------------------------
  projects: Signal<ProjectEntry[]>;
  profiles: Signal<AgentProfileRecord[]>;
  hubSkills: Signal<string[]>;
  selected: Signal<ProjectEntry | null>;
  searchQuery: Signal<string>;
  agentSkills: Signal<Record<string, string[]>>;
  deployModes: Signal<Record<string, DeployMode>>;
  expandedAgent: Signal<string | null>;
  dirty: Signal<boolean>;
  statusMessage: Signal<string | null>;
  statusIsError: Signal<boolean>;
  detectedRules: Signal<string[]>;
  busy: Signal<boolean>;
  copyRecords: Signal<CopyRecord[]>;
  copyLoading: Signal<boolean>;
  copyError: Signal<string | null>;

  // ---- 生命周期 ------------------------------------------------------------
  start(): void;
  refresh(): Promise<void>;
  reloadAgentProfiles(): void;

  // ---- 编辑动作（mod.rs） --------------------------------------------------
  selectProject(entry: ProjectEntry): void;
  toggleAgent(agentId: string): void;
  toggleDeployMode(relPath: string): void;
  addSkillToAgent(agentId: string, skill: string): void;
  removeSkillFromAgent(agentId: string, skill: string): void;

  // ---- 看护动作（actions.rs / upkeep.rs） ----------------------------------
  refreshCopyStatus(): void;
  saveAndSync(): void;
  retryCopies(): void;
  forgetCopy(project: string, rel: string, skill: string): void;

  // ---- 弹窗提交（dialogs.rs run_mutation 的页面半边） ----------------------
  /** 注册项目。Err 文案已按 failure_message 规则组装。 */
  commitRegister(path: string): Promise<{ ok: true; entry: ProjectEntry } | { ok: false; error: string }>;
  /** 更改项目路径。 */
  commitUpdatePath(name: string, path: string): Promise<{ ok: true } | { ok: false; error: string }>;
  /** 移除项目。 */
  commitRemove(name: string): Promise<{ ok: true } | { ok: false; error: string }>;

  // ---- 派生读取（必须在 JSX 里调用） ---------------------------------------
  /** detail.rs:190：启用且支持项目技能的 agent。 */
  visibleAgents(): AgentProfileRecord[];
  /** mod.rs 搜索谓词：名字或路径包含 query。 */
  filteredProjects(): ProjectEntry[];
  /** 状态非 current 的记录数（upkeep.rs pending）。 */
  pendingCopyCount(): number;
  /** actions.rs needs_deploy：存在 missing/broken_link 记录。 */
  needsDeploy(): boolean;
  /** 页脚使能：dirty || needs_deploy。 */
  canApply(): boolean;
  /** dialogs.rs mutation_blocked：busy || dirty。 */
  mutationBlocked(): boolean;
}

export function createProjectsState(bridge: ProjectsBridge): ProjectsState {
  const projects = signal<ProjectEntry[]>([]);
  const profiles = signal<AgentProfileRecord[]>([]);
  const hubSkills = signal<string[]>([]);
  const selected = signal<ProjectEntry | null>(null);
  const searchQuery = signal("");
  const agentSkills = signal<Record<string, string[]>>({});
  const deployModes = signal<Record<string, DeployMode>>({});
  const expandedAgent = signal<string | null>(null);
  const dirty = signal(false);
  const statusMessage = signal<string | null>(null);
  const statusIsError = signal(false);
  const detectedRules = signal<string[]>([]);
  const busy = signal(false);
  const copyRecords = signal<CopyRecord[]>([]);
  const copyLoading = signal(false);
  const copyError = signal<string | null>(null);

  // 代际门（GPUI 的 copy_epoch / 激活顺序）：普通变量即可——它们只在内
  // 部比对，不进渲染（自写自读的 signal 会被跟踪成循环，domainJob 教训）。
  let copyEpoch = 0;
  let activationToken = 0;
  let started = false;

  /** actions.rs begin_mutation：busy 上弦、清状态、副本门自增。 */
  function beginMutation(): void {
    busy.set(true);
    copyEpoch += 1;
    copyLoading.set(false);
    statusMessage.set(null);
  }

  /** mod.rs set_active_project：换项目即整组重置工作态。 */
  async function activate(entry: ProjectEntry, thenStatus: boolean): Promise<void> {
    const token = ++activationToken;
    copyEpoch += 1;
    const list = await bridge.skillsList(entry.name);
    if (token !== activationToken) return; // 更新的激活已发生：丢弃旧项目的清单
    agentSkills.set(list.agents);
    deployModes.set(list.deployModes);
    detectedRules.set([]);
    copyRecords.set([]);
    copyError.set(null);
    copyLoading.set(false);
    expandedAgent.set(Object.keys(list.agents)[0] ?? null);
    dirty.set(false);
    statusMessage.set(null);
    statusIsError.set(false);
    selected.set(entry);
    if (thenStatus) refreshCopyStatus();
  }

  /** upkeep.rs refresh_copy_status：锁下观测的配置与部署事实。 */
  function refreshCopyStatus(): void {
    const epoch = ++copyEpoch;
    const current = selected();
    if (current === null) {
      copyRecords.set([]);
      copyLoading.set(false);
      copyError.set(null);
      return;
    }
    const name = current.name;
    copyLoading.set(true);
    copyError.set(null);
    void bridge
      .copyStatus(name)
      .then((snapshot) => {
        if (epoch !== copyEpoch) return; // 过期回包：旧项目的事实不落屏
        copyLoading.set(false);
        copyRecords.set(snapshot.copies);
        detectedRules.set(snapshot.rules);
        if (!dirty() && !busy()) {
          selected.set(snapshot.project);
          agentSkills.set(snapshot.agents);
          deployModes.set(snapshot.deployModes);
        }
      })
      .catch((error: unknown) => {
        if (epoch !== copyEpoch) return;
        copyLoading.set(false);
        copyError.set(error instanceof Error ? error.message : String(error));
      });
  }

  /** mod.rs refresh：重读注册表，按名字保选择（不存在则退回首项）。 */
  async function refresh(): Promise<void> {
    const snapshot = await bridge.registry();
    projects.set(snapshot.projects);
    profiles.set(snapshot.profiles);
    hubSkills.set(snapshot.hubSkills);
    const prevName = selected()?.name ?? null;
    const kept =
      prevName !== null
        ? snapshot.projects.find((entry) => entry.name === prevName)
        : undefined;
    const next = kept ?? snapshot.projects[0];
    if (next !== undefined) {
      await activate(next, false);
    } else {
      activationToken += 1;
      copyEpoch += 1;
      selected.set(null);
      agentSkills.set({});
      deployModes.set({});
      expandedAgent.set(null);
      detectedRules.set([]);
      copyRecords.set([]);
    }
    refreshCopyStatus();
  }

  /** mod.rs reload_agent_profiles：设置换了启用集，只刷新名册不重载项目
   * （refresh 会重载选中项目并丢掉未保存的编辑）。 */
  function reloadAgentProfiles(): void {
    void bridge.registry().then((snapshot) => {
      profiles.set(snapshot.profiles);
    });
  }

  // ---- 弹窗提交（dialogs.rs run_mutation 对位） ---------------------------

  /** dialogs.rs failure_message：registerFailed 无插值槽，整句接错误链。 */
  function failureMessage(key: string, error: unknown): string {
    const reason = error instanceof Error ? error.message : String(error);
    if (key === "projects.registerFailed") {
      return `${tf(key, {})}\n${reason}`;
    }
    return tf(key, { err: reason });
  }

  async function runDialogMutation<T>(
    failureKey: string,
    job: () => Promise<T>
  ): Promise<{ ok: true; value: T } | { ok: false; error: string }> {
    beginMutation();
    statusMessage.set(tf("projects.operationRunning", {}));
    try {
      const value = await job();
      busy.set(false);
      statusMessage.set(null);
      statusIsError.set(false);
      return { ok: true, value };
    } catch (error: unknown) {
      const message = failureMessage(failureKey, error);
      busy.set(false);
      statusIsError.set(true);
      statusMessage.set(message);
      refreshCopyStatus(); // 失败的调和可能留下可恢复的部分部署（dialogs.rs 注）
      return { ok: false, error: message };
    }
  }

  const state: ProjectsState = {
    projects,
    profiles,
    hubSkills,
    selected,
    searchQuery,
    agentSkills,
    deployModes,
    expandedAgent,
    dirty,
    statusMessage,
    statusIsError,
    detectedRules,
    busy,
    copyRecords,
    copyLoading,
    copyError,

    start(): void {
      if (started) return;
      started = true;
      void refresh();
      agentsChanged.subscribe(() => reloadAgentProfiles());
    },

    refresh,

    reloadAgentProfiles,

    selectProject(entry: ProjectEntry): void {
      if (busy()) return;
      void activate(entry, true);
    },

    toggleAgent(agentId: string): void {
      if (busy()) return;
      const current = { ...agentSkills() };
      if (agentId in current) {
        delete current[agentId];
        if (expandedAgent() === agentId) expandedAgent.set(null);
      } else {
        current[agentId] = [];
        expandedAgent.set(agentId);
      }
      agentSkills.set(current);
      dirty.set(true);
    },

    toggleDeployMode(relPath: string): void {
      if (busy()) return;
      const current = { ...deployModes() };
      current[relPath] = current[relPath] === "copy" ? "symlink" : "copy";
      deployModes.set(current);
      dirty.set(true);
    },

    addSkillToAgent(agentId: string, skill: string): void {
      if (busy()) return;
      const current = { ...agentSkills() };
      const list = current[agentId];
      if (list === undefined) return;
      if (list.includes(skill)) return;
      current[agentId] = [...list, skill];
      agentSkills.set(current);
      dirty.set(true);
    },

    removeSkillFromAgent(agentId: string, skill: string): void {
      if (busy()) return;
      const current = { ...agentSkills() };
      const list = current[agentId];
      if (list === undefined) return;
      current[agentId] = list.filter((name) => name !== skill);
      agentSkills.set(current);
      dirty.set(true);
    },

    refreshCopyStatus,

    saveAndSync(): void {
      // actions.rs save_and_sync_current 的门：busy 或既无编辑也无待部署。
      const needs = state.needsDeploy();
      if (busy() || (!dirty() && !needs)) return;
      const project = selected();
      if (project === null) return;
      const agents = agentSkills();
      const modes = deployModes();
      beginMutation();
      void bridge
        .saveAndSync(project.path, agents, modes)
        .then(async (result) => {
          busy.set(false);
          statusIsError.set(false);
          const message = tf("projects.deploySuccess", { count: result.count });
          statusMessage.set(message);
          // defer_recovery_refresh(reload=true)：重载后状态行保住结论。
          await refresh();
          statusMessage.set(message);
          statusIsError.set(false);
        })
        .catch(async (error: unknown) => {
          busy.set(false);
          statusIsError.set(true);
          statusMessage.set(failureMessage("projects.deployFailed", error));
          refreshCopyStatus();
        });
    },

    retryCopies(): void {
      if (busy() || dirty()) return;
      const project = selected();
      if (project === null) return;
      beginMutation();
      void bridge
        .retryCopies(project.name)
        .then((records) => {
          busy.set(false);
          copyEpoch += 1;
          copyLoading.set(false);
          copyError.set(null);
          copyRecords.set(records);
          const pending = records.some((record) => record.state !== "current");
          statusIsError.set(false);
          statusMessage.set(
            tf(pending ? "projects.copiesNeedAttention" : "projects.copiesCurrent", {})
          );
          // defer_recovery_refresh(reload=false)：只重新检查，不重置编辑。
          refreshCopyStatus();
        })
        .catch((error: unknown) => {
          busy.set(false);
          statusIsError.set(true);
          statusMessage.set(failureMessage("projects.retryFailed", error));
          refreshCopyStatus();
        });
    },

    forgetCopy(projectName: string, rel: string, skill: string): void {
      if (busy() || dirty() || selected()?.name !== projectName) return;
      beginMutation();
      void bridge
        .forgetCopy(projectName, rel, skill)
        .then(async () => {
          busy.set(false);
          statusIsError.set(false);
          const message = tf("projects.copyForgotten", {});
          statusMessage.set(message);
          await refresh();
          statusMessage.set(message);
          statusIsError.set(false);
        })
        .catch((error: unknown) => {
          busy.set(false);
          statusIsError.set(true);
          const message = failureMessage("projects.forgetFailed", error);
          statusMessage.set(message);
          publishToast({ tone: "error", message, pinned: true });
          refreshCopyStatus();
        });
    },

    async commitRegister(path: string) {
      const result = await runDialogMutation("projects.registerFailed", () =>
        bridge.register(path)
      );
      if (!result.ok) return result;
      const entry = result.value;
      await refresh();
      await activate(entry, true);
      return { ok: true, entry };
    },

    async commitUpdatePath(name: string, path: string) {
      const result = await runDialogMutation("projects.pathFailed", () =>
        bridge.updatePath(name, path)
      );
      if (!result.ok) return result;
      await refresh();
      return { ok: true };
    },

    async commitRemove(name: string) {
      const result = await runDialogMutation("projects.removeFailed", () => bridge.remove(name));
      if (!result.ok) return result;
      // dialogs.rs open_remove 的成功回调：选中项若被移除则先清空再刷新。
      if (selected()?.name === name) selected.set(null);
      await refresh();
      return { ok: true };
    },

    visibleAgents(): AgentProfileRecord[] {
      return profiles().filter((profile) => profile.hasProjectSkills && profile.enabled);
    },

    filteredProjects(): ProjectEntry[] {
      const query = searchQuery().trim().toLowerCase();
      if (query === "") return projects();
      return projects().filter(
        (project) =>
          project.name.toLowerCase().includes(query) ||
          project.path.toLowerCase().includes(query)
      );
    },

    pendingCopyCount(): number {
      return copyRecords().filter((record) => record.state !== "current").length;
    },

    needsDeploy(): boolean {
      return copyRecords().some(
        (record) => record.state === "missing" || record.state === "broken_link"
      );
    },

    canApply(): boolean {
      return dirty() || state.needsDeploy();
    },

    mutationBlocked(): boolean {
      return busy() || dirty();
    },
  };

  return state;
}

/** upkeep.rs state_key：状态标签 → copyState* 文案键。 */
export function copyStateKey(state: CopyStateTag): string {
  return `projects.copyState${state.charAt(0).toUpperCase()}${state.slice(1)}`.replace(
    /_([a-z])/g,
    (_, c: string) => c.toUpperCase()
  );
}
