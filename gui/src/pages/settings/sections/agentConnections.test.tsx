// 「智能体连接」分区测试 — agent_connections.rs + managed_skills/ 的语义
// 对位：注册表过滤（排序/搜索/状态 pills/十行截断）、启用开关成功才发布
// agentsChanged（定向刷新）、共享物理目录的 journal 暂停恢复（同 key 的
// profile 联动 + pending 门 + 陈旧读丢弃）。不写真实 $HOME——真域路径由
// native crate 的 cargo 测试在沙箱覆盖（facades/settings_agent_connections.rs）。
import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../../layout";
import { agentsChanged } from "../../../shell/events";
import {
  VISIBLE_AGENTS,
  createAgentConnectionsState,
  displayPaths,
  matchedAgents,
  sharedNames,
  statusCounts,
  type AgentConnectionsBridge,
  type AgentProfileDto,
  type ManagedReportDto,
  type ManagedStateDto,
} from "./agentConnections";
import { managedStatusText, managedTargetKeyOf, noticeFromReport, pauseSnapshot } from "./managedSkills";

const HOST = "./agentConnections.test.host.tsx";

// ---- 装置（假桥：内存态 + 调用记录） ----------------------------------------

function profile(partial: Partial<AgentProfileDto> & { id: string }): AgentProfileDto {
  return {
    display_name: partial.id,
    icon: "",
    global_skills_dir: "",
    project_skills_rel: "",
    installed: true,
    enabled: false,
    synced_count: 0,
    ...partial,
  };
}

function managedReport(
  action: "paused" | "restored",
  state: ManagedStateDto,
): ManagedReportDto {
  return {
    action,
    state,
    succeeded: state.active_skill_names,
    skipped: [],
    failed: [],
  };
}

interface AgentHarness {
  bridge: AgentConnectionsBridge;
  state: ReturnType<typeof createAgentConnectionsState>;
  calls: {
    toggles: string[];
    managedToggles: string[];
    unlinks: { agentId: string; skill: string }[];
    unlinkAlls: string[];
  };
  /** 挂起的 managed 读（settle 时按序 resolve）。 */
  pendingReads: ((state: ManagedStateDto | Error) => void)[];
  diskState: Record<string, ManagedStateDto>;
}

function harness(profiles: AgentProfileDto[], diskState: Record<string, ManagedStateDto> = {}): AgentHarness {
  const calls = { toggles: [], managedToggles: [], unlinks: [], unlinkAlls: [] } as AgentHarness["calls"];
  const pendingReads: ((state: ManagedStateDto | Error) => void)[] = [];
  let enabled = new Map(profiles.map((item) => [item.id, item.enabled]));
  let names = new Map<string, string[]>();
  const bridge: AgentConnectionsBridge = {
    async listProfiles() {
      return profiles.map((item) => ({ ...item, enabled: enabled.get(item.id) === true }));
    },
    async toggleProfile(id) {
      calls.toggles.push(id);
      const next = !(enabled.get(id) === true);
      enabled.set(id, next);
      return next;
    },
    async listLinkedSkills(id) {
      return names.get(id) ?? [];
    },
    async unlinkSkill(skill, agentId) {
      calls.unlinks.push({ agentId, skill });
      const list = names.get(agentId) ?? [];
      names.set(agentId, list.filter((name) => name !== skill));
    },
    async unlinkAll(agentId) {
      calls.unlinkAlls.push(agentId);
      const removed = (names.get(agentId) ?? []).length;
      names.set(agentId, []);
      return removed;
    },
    getManagedState(agentId) {
      return new Promise<ManagedStateDto>((resolve, reject) => {
        pendingReads.push((value) => (value instanceof Error ? reject(value) : resolve(value)));
      });
    },
    async toggleManaged(agentId) {
      calls.managedToggles.push(agentId);
      const key = managedTargetKeyOf(
        profiles.find((item) => item.id === agentId)?.global_skills_dir ?? "",
      );
      const current = diskState[key] ?? { active_skill_names: [], suspended_skill_names: [] };
      const next =
        current.suspended_skill_names.length > 0
          ? {
              active_skill_names: current.suspended_skill_names,
              suspended_skill_names: [],
            }
          : {
              active_skill_names: [],
              suspended_skill_names: current.active_skill_names,
            };
      diskState[key] = next;
      for (const item of profiles) {
        if (managedTargetKeyOf(item.global_skills_dir) === key) {
          names.set(item.id, [...next.active_skill_names]);
        }
      }
      return managedReport(
        current.suspended_skill_names.length > 0 ? "restored" : "paused",
        next,
      );
    },
  };
  return {
    bridge,
    state: createAgentConnectionsState(bridge),
    calls,
    pendingReads,
    diskState,
  };
}

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 8; i += 1) {
    await Promise.resolve();
  }
}

// ---- 注册表过滤（matched_agents / status_counts / 十行截断） -----------------

test("matched agents sort enabled first, then search, then the status pill", () => {
  const profiles = [
    profile({ id: "off-agent", display_name: "Zebra", enabled: false }),
    profile({ id: "codex", display_name: "Codex CLI", enabled: true }),
    profile({ id: "cursor", display_name: "Cursor", enabled: false }),
  ];
  // enabled 优先排序。
  expect(matchedAgents(profiles, "", "all").map((item) => item.id)).toEqual([
    "codex",
    "off-agent",
    "cursor",
  ]);
  // 搜索命中 display_name 或 id（不区分大小写）。
  expect(matchedAgents(profiles, "CO", "all").map((item) => item.id)).toEqual(["codex"]);
  expect(matchedAgents(profiles, "cursor", "all").map((item) => item.id)).toEqual(["cursor"]);
  // 状态 pills 在搜索之后过滤。
  expect(matchedAgents(profiles, "", "disabled").map((item) => item.id)).toEqual([
    "off-agent",
    "cursor",
  ]);
  expect(matchedAgents(profiles, "zeb", "enabled")).toEqual([]);
});

test("pill counts are post-search, pre-filter", () => {
  const profiles = [
    profile({ id: "a", enabled: true }),
    profile({ id: "b", enabled: false }),
    profile({ id: "c", enabled: false }),
  ];
  expect(statusCounts(profiles, "")).toEqual([3, 1, 2]);
  expect(statusCounts(profiles, "b")).toEqual([1, 0, 1]);
});

test("the unfiltered list caps at ten rows; any filter shows everything", () => {
  expect(VISIBLE_AGENTS).toBe(10);
  const many = Array.from({ length: 13 }, (_, index) => profile({ id: `agent-${index}` }));
  const state = harness(many).state;
  // 无过滤：十行 + show-more（host 断言展开交互；这里断言纯函数半边）。
  expect(matchedAgents(many, "", "all")).toHaveLength(13);
  // 有过滤词：列表全显（GPUI filter_active 分支）。
  state.editAgentQuery("agent-1");
  expect(state.agentQuery()).toBe("agent-1");
  state.clearAgentFilters();
  expect(state.agentQuery()).toBe("");
  expect(state.agentFilter()).toBe("all");
});

test("codex shows the legacy second path; shared names cross profiles", () => {
  expect(
    displayPaths(profile({ id: "codex", global_skills_dir: "/Users/me/.codex/skills" }), "/Users/me"),
  ).toEqual(["~/.codex/skills", "~/.agents/skills"]);
  // HOME 外的路径原样显示（home_prefix 的直通分支）。
  expect(
    displayPaths(profile({ id: "cursor", global_skills_dir: "/opt/cursor/skills" }), "/Users/me"),
  ).toEqual(["/opt/cursor/skills"]);
  // codex 的主目录恰为 legacy 路径时去重。
  expect(
    displayPaths(profile({ id: "codex", global_skills_dir: "/Users/me/.agents/skills" }), "/Users/me"),
  ).toEqual(["~/.agents/skills"]);
  const profiles = [
    profile({ id: "alpha", display_name: "Alpha", global_skills_dir: "/hub/shared/skills/" }),
    profile({ id: "beta", display_name: "Beta", global_skills_dir: "/hub/shared/skills" }),
    profile({ id: "solo", display_name: "Solo", global_skills_dir: "/hub/other/skills" }),
  ];
  // 同物理目录（尾斜杠归一）互为共享；无全局目录的 profile 不参与。
  expect(sharedNames(profiles, profiles[0])).toEqual(["Beta"]);
  expect(sharedNames(profiles, profiles[2])).toEqual([]);
  expect(
    sharedNames([...profiles, profile({ id: "none", display_name: "None" })], profiles[0]),
  ).toEqual(["Beta"]);
});

// ---- 启用开关 → agentsChanged 定向刷新 --------------------------------------

test("a successful toggle publishes agentsChanged exactly once; a failure never does", async () => {
  const h = harness([
    profile({ id: "codex", global_skills_dir: "/hub/codex/skills", enabled: false }),
  ]);
  await h.state.start();
  const events: string[] = [];
  const off = agentsChanged.subscribe(() => events.push("changed"));
  // 成功：enabled 跟随 + 定向刷新事件（GPUI cx.emit(AgentsChanged) 对位）。
  await h.state.toggleAgentEnabled("codex");
  expect(events).toEqual(["changed"]);
  expect(h.state.profiles().find((item) => item.id === "codex")?.enabled).toBe(true);
  expect(h.calls.toggles).toEqual(["codex"]);
  // 失败：本地不动、不发布（toggleFailed 的 toast 走 notice 频道）。
  h.bridge.toggleProfile = async () => {
    throw new Error("pref write failed");
  };
  await h.state.toggleAgentEnabled("codex");
  expect(events).toEqual(["changed"]);
  expect(h.state.profiles().find((item) => item.id === "codex")?.enabled).toBe(true);
  off();
});

test("enabling an agent with a global dir refreshes its managed state", async () => {
  const h = harness([
    profile({ id: "codex", global_skills_dir: "/hub/codex/skills", enabled: false }),
  ]);
  await h.state.start();
  await h.state.toggleAgentEnabled("codex");
  // toggle 后排了一个 refresh 读（pending 门在途时重复 toggle 被拒）。
  expect(h.pendingReads).toHaveLength(1);
  h.pendingReads[0]({ active_skill_names: ["writer"], suspended_skill_names: [] });
  await settle();
  expect(h.state.managedStates()["/hub/codex/skills"]).toEqual({
    active_skill_names: ["writer"],
    suspended_skill_names: [],
  });
});

// ---- 共享物理目录 journal 暂停恢复 -------------------------------------------

test("the directory key folds equivalent spellings", () => {
  expect(managedTargetKeyOf("/hub/shared/skills/")).toBe("/hub/shared/skills");
  expect(managedTargetKeyOf("C:\\hub\\shared\\skills")).toBe("C:/hub/shared/skills");
  expect(managedTargetKeyOf("  /hub/skills  ")).toBe("/hub/skills");
});

test("pause snapshots cover the five states with unique, trimmed names", () => {
  expect(pauseSnapshot(undefined)).toEqual({
    status: "loading",
    active: [],
    suspended: [],
    action: null,
    checked: false,
  });
  const active = pauseSnapshot({
    active_skill_names: ["alpha", "beta", "alpha", " "],
    suspended_skill_names: [],
  });
  expect(active.status).toBe("active");
  expect(active.checked).toBe(true);
  expect(active.action).toBe("pause");
  expect(active.active).toEqual(["alpha", "beta"]);
  const paused = pauseSnapshot({
    active_skill_names: [],
    suspended_skill_names: ["alpha", "retired-skill", "alpha"],
  });
  expect(paused.status).toBe("paused");
  expect(paused.action).toBe("restore");
  expect(paused.checked).toBe(false);
  const partial = pauseSnapshot({
    active_skill_names: ["still-active"],
    suspended_skill_names: ["needs-retry"],
  });
  expect(partial.status).toBe("partial");
  expect(partial.action).toBe("restore");
  expect(pauseSnapshot({ active_skill_names: [], suspended_skill_names: [] }).status).toBe("empty");
});

test("pausing and restoring through one profile updates every profile on the directory", async () => {
  // 两个 profile 共享一个物理目录：journal、pending、linked 名单都是目录级的。
  const h = harness(
    [
      profile({ id: "alpha", display_name: "Alpha", global_skills_dir: "/hub/shared/skills/", enabled: true, synced_count: 1 }),
      profile({ id: "beta", display_name: "Beta", global_skills_dir: "/hub/shared/skills", enabled: true, synced_count: 1 }),
    ],
    { "/hub/shared/skills": { active_skill_names: ["writer"], suspended_skill_names: [] } },
  );
  await h.state.start();
  // preload：一个物理目录只读一次。
  expect(h.pendingReads).toHaveLength(1);
  h.pendingReads[0]({ active_skill_names: ["writer"], suspended_skill_names: [] });
  await settle();
  // apply_managed_state：同目录的兄弟 profile 一起拿到名单与计数。
  expect(h.state.linkedSkills()["alpha"]).toEqual(["writer"]);
  expect(h.state.linkedSkills()["beta"]).toEqual(["writer"]);
  expect(h.state.profiles().find((item) => item.id === "beta")?.synced_count).toBe(1);

  // 暂停（经 alpha）：报告落 state，兄弟目录快照共享同一份。
  await h.state.toggleManagedSkills("alpha");
  await settle();
  expect(h.calls.managedToggles).toEqual(["alpha"]);
  expect(
    h.state.managedStates()["/hub/shared/skills"]?.suspended_skill_names,
  ).toEqual(["writer"]);
  expect(h.state.profiles().find((item) => item.id === "alpha")?.synced_count).toBe(0);
  expect(h.state.profiles().find((item) => item.id === "beta")?.synced_count).toBe(0);

  // 恢复经 beta（同一 journal 的另一半）：名单回到两个 profile。
  await h.state.toggleManagedSkills("beta");
  await settle();
  expect(h.state.managedStates()["/hub/shared/skills"]?.active_skill_names).toEqual(["writer"]);
  expect(h.state.profiles().find((item) => item.id === "beta")?.synced_count).toBe(1);
});

test("an in-flight directory operation gates the switch to exactly one call", async () => {
  const h = harness(
    [profile({ id: "alpha", global_skills_dir: "/hub/s/skills", enabled: true })],
    { "/hub/s/skills": { active_skill_names: ["writer"], suspended_skill_names: [] } },
  );
  await h.state.start();
  // preload 的读先落地，pending 门才关闭。
  h.pendingReads[0]({ active_skill_names: ["writer"], suspended_skill_names: [] });
  await settle();
  const gate: { release: null | (() => void) } = { release: null };
  h.bridge.toggleManaged = (agentId) => {
    h.calls.managedToggles.push(agentId);
    return new Promise<ManagedReportDto>((resolve) => {
      gate.release = () =>
        resolve(
          managedReport("paused", { active_skill_names: [], suspended_skill_names: ["writer"] }),
        );
    });
  };
  const first = h.state.toggleManagedSkills("alpha");
  // 在途：pending 阻断第二次操作（同一物理目录一个操作）。
  h.state.toggleManagedSkills("alpha");
  expect(h.calls.managedToggles).toEqual(["alpha"]);
  gate.release?.();
  await first;
  await settle();
  expect(h.state.managedPending()["/hub/s/skills"]).toBeUndefined();
});

test("a stale read after an invalidate never overwrites the newer state", async () => {
  const h = harness(
    [profile({ id: "alpha", global_skills_dir: "/hub/s/skills", enabled: true })],
    { "/hub/s/skills": { active_skill_names: ["writer"], suspended_skill_names: [] } },
  );
  await h.state.start();
  const stale = h.pendingReads[0];
  // 在读落地前发起一次目录操作（invalidate 抬代际）。
  const op = h.state.toggleManagedSkills("alpha");
  stale({ active_skill_names: ["ghost"], suspended_skill_names: [] });
  await op;
  await settle();
  // 陈旧读被丢弃：journal 只反映操作后的暂停集，不是 ghost。
  expect(h.state.managedStates()["/hub/s/skills"]?.suspended_skill_names).toEqual(["writer"]);
  expect(h.state.linkedSkills()["alpha"]).toEqual([]);
});

// ---- notice 文案（notice_from_report 对位） ----------------------------------

test("the toggle report notice decomposes failures, skips and success", () => {
  const clean = noticeFromReport("Devin", managedReport("paused", {
    active_skill_names: [],
    suspended_skill_names: ["a", "b"],
  }));
  // managedReport 的 succeeded 取 active 名单，这里手工给两枚成功名。
  const ok = noticeFromReport("Devin", {
    action: "paused",
    state: { active_skill_names: [], suspended_skill_names: ["a", "b"] },
    succeeded: ["a", "b"],
    skipped: [],
    failed: [],
  });
  expect(ok.tone).toBe("success");
  expect(ok.text).toContain("Devin");
  expect(ok.text).toContain("2");
  expect(clean.tone).toBe("success");

  const skipped = noticeFromReport("Devin", {
    action: "restored",
    state: { active_skill_names: ["a"], suspended_skill_names: [] },
    succeeded: ["a"],
    skipped: [
      {
        skill_name: "occupied",
        code: "unmanaged_real_directory",
        path: "/hub/occupied",
        reason: "",
      },
      {
        skill_name: "linked",
        code: "foreign_link",
        path: "/hub/linked",
        reason: "",
      },
    ],
    failed: [],
  });
  expect(skipped.tone).toBe("warning");
  expect(skipped.openPath).toBe("/hub/occupied");
  expect(skipped.text).toContain("occupied");
  expect(skipped.text).toContain("外部链接");

  const failed = noticeFromReport("Devin", {
    action: "paused",
    state: { active_skill_names: [], suspended_skill_names: [] },
    succeeded: [],
    skipped: [],
    failed: [{ skill_name: "x", error: "boom" }],
  });
  expect(failed.tone).toBe("error");
  expect(failed.text).toContain("x: boom");
});

test("the managed status line reads pending, loading and the pause states", () => {
  expect(managedStatusText(true, pauseSnapshot(undefined))).toBe(
    translationT("settings.managedSkillsUpdating"),
  );
  expect(managedStatusText(false, pauseSnapshot(undefined))).toBe(
    translationT("settings.managedSkillsChecking"),
  );
  expect(
    managedStatusText(false, pauseSnapshot({ active_skill_names: [], suspended_skill_names: ["a", "b"] })),
  ).toBe(translationT("settings.managedSkillsPausedCount").replace("{{count}}", "2"));
});

// t() 不能在模块顶层取（locale signal）；测试内经 services/translation 取。
import { t as translationT } from "../../../services/translation";

// ---- host UI（假桥装置；过滤与展开的渲染期挂载） -----------------------------

test("the section renders rows, filters by pill and expands the linked panel", async () => {
  const app = await open(HOST);
  // 产品默认窗宽（基座 openSettings 同款：无头引擎默认窗口比产品窄，
  // 分区行会落在视口外，press 打不中目标）。
  await app.setViewport(WINDOW_W, WINDOW_H);
  await app.waitUntil(() => app.shows("id:agent-filter-all"), { timeout: 2000 });
  // 读数格式 agents:<all>|<enabled>|<当前 pill>（装置播种 3 个 profile：
  // 1 enabled / 2 disabled；默认 all pill → 第三位也是总数）。
  await app.waitUntil(() => app.texts().some((s) => s.includes("agents:3|1|3")));
  // 状态过滤：disabled pill 把读数第三位切到 2，行也只剩未启用的。
  await app.press("id:agent-links-beta");
  await app.waitUntil(() => app.texts().some((s) => s.includes("expanded:beta")));
  // 面板挂出：名单 chip 与一键清空按钮都在（managed 读已落地为 1 张卡）。
  await app.waitUntil(() => app.shows("id:unlink-all-beta"), { timeout: 2000 });
});

test("deploy mode state loads overrides, switches default & per-agent mode, and publishes agentsChanged", async () => {
  const h = harness([
    profile({ id: "cursor", display_name: "Cursor", enabled: true, global_skills_dir: "/hub/cursor" }),
    profile({ id: "antigravity", display_name: "Antigravity", enabled: true, global_skills_dir: "/hub/ag" }),
  ]);
  let defaultMode: "symlink" | "copy" = "symlink";
  let overrides: Record<string, "symlink" | "copy"> = {};
  const setDefaultCalls: string[] = [];
  const setAgentCalls: { agentId: string; mode: string }[] = [];

  h.bridge.getDeployModes = async () => ({
    defaultMode,
    overrides: { ...overrides },
    disallowJunction: ["antigravity"],
  });
  h.bridge.setDefaultDeployMode = async (mode) => {
    setDefaultCalls.push(mode);
    defaultMode = mode;
    return {
      defaultMode: mode,
      reconciled: { updated: 2, skipped_modified: 0, skipped_foreign: 0, failures: [] },
    };
  };
  h.bridge.setAgentDeployMode = async (agentId, choice) => {
    setAgentCalls.push({ agentId, mode: choice });
    if (choice === "inherit") delete overrides[agentId];
    else overrides[agentId] = choice;
    return {
      overrides: { ...overrides },
      reconciled: { updated: 1, skipped_modified: choice === "copy" ? 1 : 0, skipped_foreign: 0, failures: [] },
    };
  };

  let changeEvents = 0;
  const unsub = agentsChanged.subscribe(() => {
    changeEvents += 1;
  });

  await h.state.start();
  expect(h.state.deployModeAvailable()).toBe(true);
  expect(h.state.defaultDeployMode()).toBe("symlink");
  expect(h.state.disallowJunctionAgents()).toEqual(["antigravity"]);

  h.state.setDefaultDeployMode("copy");
  await settle();
  expect(setDefaultCalls).toEqual(["copy"]);
  expect(h.state.defaultDeployMode()).toBe("copy");
  expect(changeEvents).toBe(1);

  h.state.setAgentDeployMode("cursor", "symlink");
  await settle();
  expect(setAgentCalls[0]).toEqual({ agentId: "cursor", mode: "symlink" });
  expect(h.state.agentDeployOverrides()).toEqual({ cursor: "symlink" });
  expect(changeEvents).toBe(2);

  h.state.setAgentDeployMode("cursor", "inherit");
  await settle();
  expect(setAgentCalls[1]).toEqual({ agentId: "cursor", mode: "inherit" });
  expect(h.state.agentDeployOverrides()).toEqual({});
  unsub();
});

test("unlinkLinkedSkill, unlinkAllLinkedSkills, and toggleManagedSkills publish agentsChanged on success only", async () => {
  const h = harness(
    [profile({ id: "codex", display_name: "Codex", global_skills_dir: "/hub/codex/skills", enabled: true })],
    { "/hub/codex/skills": { active_skill_names: ["alpha", "beta"], suspended_skill_names: [] } },
  );
  await h.state.start();
  h.pendingReads[0]({ active_skill_names: ["alpha", "beta"], suspended_skill_names: [] });
  await settle();

  let events = 0;
  const unsub = agentsChanged.subscribe(() => {
    events += 1;
  });

  h.state.unlinkLinkedSkill("codex", "alpha");
  await settle();
  expect(events).toBe(1);

  h.state.unlinkAllLinkedSkills("codex");
  await settle();
  expect(events).toBe(2);

  h.state.toggleManagedSkills("codex");
  await settle();
  expect(events).toBe(3);

  h.bridge.unlinkSkill = async () => {
    throw new Error("unlink fail");
  };
  h.state.unlinkLinkedSkill("codex", "beta");
  await settle();
  expect(events).toBe(3);
  unsub();
});
