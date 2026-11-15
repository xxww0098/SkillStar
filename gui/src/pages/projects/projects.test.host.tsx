// 项目页的无头测试装置（githubSignIn.test.host.tsx 样板）：假桥注入
// ProjectsWorkbench。域模型复刻 GPUI ProjectSandbox 的语义（test_support.rs
// + recovery_tests.rs）：
// - seed：项目 alpha 以 copy 模式给 codex 部署 demo（记录 current）；
// - hub-v2：Hub 内容升级 → 记录 stale；
// - move-project：项目根迁移 → 记录 project_missing（重试报错带名字/路
//   径；清理记录不动两端的文件）；
// - delete-copy：副本被删 → 记录 missing（重试不恢复，仅显式保存恢复）。
// 装置按钮翻转模型、readout 暴露 save/retry/forget 计数、最近一次保存载
// 荷、目标内容与 Hub 内容；DialogHost 挂上（路径弹窗/确认走共享
// ConfirmCard）。不触文件系统、不打真网——真域路径由 native crate 的
// cargo 测试在 SKILLSTAR_DATA_DIR 沙箱覆盖（facades/projects.rs tests）。

import { Button } from "nsh/base";
import { Text, View, effect, mount, signal, type JSX } from "nsh";
import { DialogHost } from "../../chrome/dialog";
import type {
  CopyRecord,
  CopyStatusSnapshot,
  DeployMode,
  ProjectEntry,
  ProjectsBridge,
  RegistrySnapshot,
} from "./bridge";
import { createProjectsState } from "./state";
import { ProjectsWorkbench } from "./index";

// ---- 假域模型 -------------------------------------------------------------------

interface FakeProject {
  entry: ProjectEntry;
  rootExists: boolean;
  agents: Record<string, string[]>;
  deployModes: Record<string, DeployMode>;
  /** rel/skill → 部署副本内容（null = 副本已不存在）。 */
  copies: Record<string, string | null>;
  rules: string[];
}

const PROFILES: RegistrySnapshot["profiles"] = [
  {
    id: "codex",
    displayName: "Codex",
    projectSkillsRel: ".agents/skills",
    hasProjectSkills: true,
    enabled: true,
  },
  {
    id: "cursor",
    displayName: "Cursor",
    projectSkillsRel: ".cursor/skills",
    hasProjectSkills: true,
    enabled: true,
  },
  {
    id: "global-only",
    displayName: "GlobalOnly",
    projectSkillsRel: "",
    hasProjectSkills: false,
    enabled: true,
  },
];

let hubContent = "v1";
let nextSeq = 1;

function makeProject(name: string, path: string): FakeProject {
  return {
    entry: { name, path, createdAt: "2026-01-01T00:00:00Z" },
    rootExists: true,
    agents: { codex: ["demo"] },
    deployModes: { ".agents/skills": "copy" },
    copies: { ".agents/skills/demo": hubContent },
    rules: ["CLAUDE.md"],
  };
}

const projects: FakeProject[] = [makeProject("alpha", "/tmp/proj/alpha")];

function recordState(project: FakeProject, key: string): CopyRecord["state"] {
  if (!project.rootExists) return "project_missing";
  const content = project.copies[key];
  if (content === undefined) return "current";
  if (content === null) return "missing";
  return content === hubContent ? "current" : "stale";
}

function recordsOf(project: FakeProject): CopyRecord[] {
  return Object.keys(project.copies).map((key) => {
    // key 形如 ".agents/skills/demo"：最后一个 "/" 前是 rel，其后是技能名。
    const cut = key.lastIndexOf("/");
    return {
      projectName: project.entry.name,
      relativePath: key.slice(0, cut),
      skillName: key.slice(cut + 1),
      state: recordState(project, key),
      detail: null,
    };
  });
}

// ---- 假桥 -----------------------------------------------------------------------

const calls = { save: 0, retry: 0, forget: 0, register: 0 };

const pulse = signal(0);
const readoutText = signal("");
let lastSave: { path: string; agents: string; modes: string } | null = null;

function bump(): void {
  pulse.set(pulse() + 1);
}

function findProject(name: string): FakeProject | undefined {
  return projects.find((project) => project.entry.name === name);
}

function statusOf(name: string): CopyStatusSnapshot {
  const project = findProject(name) ?? projects[0];
  return {
    project: { ...project.entry },
    agents: JSON.parse(JSON.stringify(project.agents)),
    deployModes: JSON.parse(JSON.stringify(project.deployModes)),
    copies: recordsOf(project),
    rules: [...project.rules],
  };
}

const fakeBridge: ProjectsBridge = {
  async registry(): Promise<RegistrySnapshot> {
    bump();
    return {
      projects: projects.map((project) => ({ ...project.entry })),
      profiles: PROFILES,
      hubSkills: ["demo"],
    };
  },
  async skillsList(name) {
    const project = findProject(name);
    if (project === undefined) return { agents: {}, deployModes: {} };
    bump();
    return {
      agents: JSON.parse(JSON.stringify(project.agents)),
      deployModes: JSON.parse(JSON.stringify(project.deployModes)),
    };
  },
  async copyStatus(name) {
    bump();
    return statusOf(name);
  },
  async saveAndSync(path, agents, deployModes) {
    calls.save += 1;
    lastSave = {
      path,
      agents: JSON.stringify(Object.fromEntries(Object.entries(agents).sort())),
      modes: JSON.stringify(deployModes),
    };
    bump();
    const project = projects.find((entry) => entry.entry.path === path);
    if (project === undefined) throw new Error(`unknown project path: ${path}`);
    if (!project.rootExists) {
      // sync.rs ensure_project_root_exists：路径不存在即失败，文件不动。
      throw new Error(`Project path not found or not a directory: ${path}\n请在「Projects」页面点击「更改路径」重新关联该项目目录。`);
    }
    project.agents = JSON.parse(JSON.stringify(agents));
    project.deployModes = JSON.parse(JSON.stringify(deployModes));
    // 部署语义（sync.rs）：清单里每个 (agent → skill) 落到该 agent 的
    // project_skills_rel；这里直接以 Hub 当前内容重建副本。
    const relOf: Record<string, string> = {
      codex: ".agents/skills",
      cursor: ".cursor/skills",
    };
    const deployed = new Set<string>();
    for (const [agent, skills] of Object.entries(agents)) {
      const rel = relOf[agent] ?? ".agents/skills";
      for (const skill of skills) deployed.add(`${rel}/${skill}`);
    }
    for (const key of Object.keys(project.copies)) {
      if (!deployed.has(key) && project.copies[key] !== null) {
        delete project.copies[key]; // 清单里移除的技能：部署撤下、记录消失
      }
    }
    for (const key of deployed) {
      project.copies[key] = hubContent;
    }
    const count = Object.values(agents).reduce((sum, list) => sum + list.length, 0);
    bump();
    return { count };
  },
  async retryCopies(name) {
    calls.retry += 1;
    bump();
    const project = findProject(name);
    if (project === undefined) throw new Error(`unknown project: ${name}`);
    if (!project.rootExists) {
      // retry_project_copies 对缺失根：错误带项目名与路径（recovery_tests
      // 断言 error.contains(name) && error.contains(path)）。
      throw new Error(`Project ${project.entry.name} not found at ${project.entry.path}`);
    }
    for (const key of Object.keys(project.copies)) {
      if (project.copies[key] === null) continue; // 副本已删：重试不重建
      project.copies[key] = hubContent; // stale → 按当前 Hub 重建
    }
    bump();
    return recordsOf(project);
  },
  async forgetCopy(name, rel, skill) {
    calls.forget += 1;
    bump();
    const project = findProject(name);
    if (project !== undefined) {
      delete project.copies[`${rel}/${skill}`];
      // 元数据清理：从共用该目录的全部 agent 清单移除（upkeep.rs 语义）。
      for (const agent of Object.keys(project.agents)) {
        project.agents[agent] = project.agents[agent].filter((entry) => entry !== skill);
      }
    }
    bump();
    return null;
  },
  async register(path) {
    calls.register += 1;
    bump();
    const trimmed = path.trim();
    if (trimmed === "") throw new Error("empty path");
    if (projects.some((project) => project.entry.path === trimmed)) {
      throw new Error("already registered");
    }
    const name = trimmed.split("/").filter(Boolean).pop() ?? "project";
    const project = makeProject(name, trimmed);
    project.agents = {};
    project.deployModes = {};
    project.copies = {};
    project.rules = [];
    projects.push(project);
    bump();
    return { ...project.entry };
  },
  async updatePath(name, path) {
    const project = findProject(name);
    if (project === undefined) throw new Error(`unknown project: ${name}`);
    project.entry.path = path;
    bump();
    return null;
  },
  async remove(name) {
    const index = projects.findIndex((project) => project.entry.name === name);
    if (index >= 0) projects.splice(index, 1);
    bump();
    return null;
  },
};

// ---- 装置 -----------------------------------------------------------------------

const state = createProjectsState(fakeBridge);

effect(() => {
  void pulse();
  const target = projects[0]?.copies[".agents/skills/demo"];
  const missingKeys = Object.keys(projects[0]?.copies ?? {}).filter(
    (key) => projects[0].copies[key] === null
  );
  readoutText.set(
    `save:${calls.save} retry:${calls.retry} forget:${calls.forget} reg:${calls.register} ` +
      `hub:${hubContent} target:${target === undefined ? "none" : target === null ? "gone" : target} ` +
      `missingKeys:${missingKeys.length} ` +
      `last:${lastSave === null ? "none" : `${lastSave.agents}|${lastSave.modes}`}`
  );
});

function Readout(): JSX.Element {
  return (
    <Text testID="host-readout" style={{ fontSize: 10 }}>
      {readoutText()}
    </Text>
  );
}

function App(): JSX.Element {
  return (
    <View
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        height: "100%",
      }}
    >
      <View
        style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap", padding: 4 }}
      >
        <Button
          testID="host-hub-v2"
          onPress={() => {
            hubContent = "v2";
            bump();
          }}
        >
          <Text>hub-v2</Text>
        </Button>
        <Button
          testID="host-move-project"
          onPress={() => {
            const project = projects[0];
            project.rootExists = false;
            bump();
          }}
        >
          <Text>move-project</Text>
        </Button>
        <Button
          testID="host-delete-copy"
          onPress={() => {
            projects[0].copies[".agents/skills/demo"] = null;
            bump();
          }}
        >
          <Text>delete-copy</Text>
        </Button>
        <Button
          testID="host-refresh"
          onPress={() => {
            void state.refresh();
          }}
        >
          <Text>refresh</Text>
        </Button>
      </View>
      <Readout />
      <View style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <ProjectsWorkbench state={state} />
      </View>
      {/* 路径弹窗/清理确认走共享 ConfirmCard：DialogHost 必须在场。 */}
      <DialogHost />
    </View>
  );
}

export default App;

mount(App);
