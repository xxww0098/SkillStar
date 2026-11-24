// 项目域桥 —— native/src/facades/projects.rs（Projects 模块）的 TS 半边。
// 页面与测试装置只面对 ProjectsBridge：真实现是薄薄一层 JSON→类型收窄，
// 测试装置注入假桥（accounts/bridge.ts 同款纪律）。方法切面与 GPUI
// spawn_domain 的调用一一对应：注册表/技能清单是便宜的本地读，副本看护、
// 保存部署与登记写回走 blocking 池（Promise）。

import { Projects } from "skillstar-gui/native";

/** projects.json 的一行（types.rs ProjectEntry）。 */
export interface ProjectEntry {
  name: string;
  path: string;
  createdAt: string;
}

/** agents::AgentProfile 的项目页投影（detail.rs:190 只消费这几列）。 */
export interface AgentProfileRecord {
  id: string;
  displayName: string;
  icon?: string;
  projectSkillsRel: string;
  hasProjectSkills: boolean;
  enabled: boolean;
}

/** 一条持久化部署记录（types.rs ProjectCopyRecord）。 */
export interface CopyRecord {
  projectName: string;
  relativePath: string;
  skillName: string;
  /** snake_case 状态标签，映射 projects.copyState* 文案。 */
  state: CopyStateTag;
  detail: string | null;
}

export type CopyStateTag =
  | "current"
  | "stale"
  | "project_missing"
  | "missing"
  | "modified"
  | "unmanaged"
  | "source_missing"
  | "read_failed"
  | "broken_link";

export type DeployMode = "symlink" | "copy";

export type PlanAction = "create" | "already";
export type PlanApprovalSource = "skillstar" | "elicitation";

export interface PendingPlanOperation {
  name: string;
  action: PlanAction;
  skillPath: string;
}

export interface PendingProjectSkillPlan {
  planId: string;
  planHash: string;
  canonicalRoot: string;
  isRegistered: boolean;
  owner: string;
  affectedAgents: string[];
  operations: PendingPlanOperation[];
  expiresAt: string;
  approved: boolean;
  approvalSource: PlanApprovalSource | null;
}

export interface ProjectPlanApprovalRecord {
  planId: string;
  planHash: string;
  source: PlanApprovalSource;
  recordedAt: string;
}

/** mod.rs refresh 载荷：注册表 + agent 名册 + Hub 技能名。 */
export interface RegistrySnapshot {
  projects: ProjectEntry[];
  profiles: AgentProfileRecord[];
  hubSkills: string[];
}

/** upkeep.rs ProjectCopySnapshot：同一项目锁下观测到的配置与部署事实。 */
export interface CopyStatusSnapshot {
  project: ProjectEntry;
  agents: Record<string, string[]>;
  deployModes: Record<string, DeployMode>;
  copies: CopyRecord[];
  rules: string[];
}

export interface ProjectsBridge {
  /** mod.rs refresh → list_projects + list_profiles + installed_names。 */
  registry(): Promise<RegistrySnapshot>;
  /** 仅重读 agent 名册（设置页切换启用集时避免重读项目清单与 Hub 技能）。 */
  profiles?(): Promise<AgentProfileRecord[]>;
  /** mod.rs set_active_project → load_skills_list（缺失降级为空表）。 */
  skillsList(project: string): Promise<{
    agents: Record<string, string[]>;
    deployModes: Record<string, DeployMode>;
  }>;
  /** upkeep.rs refresh_copy_status → inspect_project_copy_status。 */
  copyStatus(project: string): Promise<CopyStatusSnapshot>;
  /** actions.rs save_and_sync_current → save_and_sync（count 进页脚文案）。 */
  saveAndSync(
    path: string,
    agents: Record<string, string[]>,
    deployModes: Record<string, DeployMode>
  ): Promise<{ count: number }>;
  /** actions.rs retry_copies → retry_project_copies。 */
  retryCopies(project: string): Promise<CopyRecord[]>;
  /** actions.rs forget_copy → forget_project_copy（只清记录不动文件）。 */
  forgetCopy(project: string, rel: string, skill: string): Promise<null>;
  /** dialogs.rs PathAction::Register → register_project。 */
  register(path: string): Promise<ProjectEntry>;
  /** dialogs.rs PathAction::Change → update_project_path。 */
  updatePath(name: string, path: string): Promise<null>;
  /** dialogs.rs open_remove → remove_project。 */
  remove(name: string): Promise<null>;
  /** dialogs.rs 路径弹窗「选择目录」按钮 → Projects.pick_folder。 */
  pickFolder?(prompt: string): Promise<string | null>;
  /** 检验项目根目录是否存在未过期且未应用的 MCP 计划。 */
  pendingProjectSkillPlan?(projectRoot: string): Promise<PendingProjectSkillPlan | null>;
  /** 仅写入 SkillStar 桌面批准（不部署、不修改项目文件）。 */
  approveProjectSkillPlan?(planId: string, planHash: string): Promise<ProjectPlanApprovalRecord>;
}

// ---- JSON 收窄（桥 crate 的事实源） ------------------------------------------

function asString(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function asRecord(value: unknown): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  if (value !== null && typeof value === "object") {
    for (const [key, list] of Object.entries(value as Record<string, unknown>)) {
      out[key] = asStringArray(list);
    }
  }
  return out;
}

function asModes(value: unknown): Record<string, DeployMode> {
  const out: Record<string, DeployMode> = {};
  if (value !== null && typeof value === "object") {
    for (const [key, mode] of Object.entries(value as Record<string, unknown>)) {
      out[key] = mode === "copy" ? "copy" : "symlink";
    }
  }
  return out;
}

function asEntry(value: unknown): ProjectEntry {
  const rec = (value ?? {}) as Record<string, unknown>;
  return {
    name: asString(rec.name),
    path: asString(rec.path),
    createdAt: asString(rec.createdAt),
  };
}

function asCopyRecord(value: unknown): CopyRecord {
  const rec = (value ?? {}) as Record<string, unknown>;
  return {
    projectName: asString(rec.projectName),
    relativePath: asString(rec.relativePath),
    skillName: asString(rec.skillName),
    state: (typeof rec.state === "string" ? rec.state : "current") as CopyStateTag,
    detail: typeof rec.detail === "string" ? rec.detail : null,
  };
}

function asProfiles(raw: unknown): AgentProfileRecord[] {
  return Array.isArray(raw)
    ? raw.map((entry) => {
        const rec = (entry ?? {}) as Record<string, unknown>;
        return {
          id: asString(rec.id),
          displayName: asString(rec.displayName),
          icon: typeof rec.icon === "string" ? rec.icon : undefined,
          projectSkillsRel: asString(rec.projectSkillsRel),
          hasProjectSkills: rec.hasProjectSkills === true,
          enabled: rec.enabled === true,
        };
      })
    : [];
}

/** 真桥：native facade 直调（页面默认使用）。 */
export const realProjectsBridge: ProjectsBridge = {
  async registry(): Promise<RegistrySnapshot> {
    const raw = (await Projects.registry()) as Record<string, unknown>;
    return {
      projects: Array.isArray(raw.projects) ? raw.projects.map(asEntry) : [],
      profiles: asProfiles(raw.profiles),
      hubSkills: asStringArray(raw.hubSkills),
    };
  },
  async profiles(): Promise<AgentProfileRecord[]> {
    return asProfiles(await Projects.profiles());
  },
  async skillsList(project) {
    const raw = (await Projects.skillsList(project)) as Record<string, unknown>;
    return { agents: asRecord(raw.agents), deployModes: asModes(raw.deployModes) };
  },
  async copyStatus(project) {
    const raw = (await Projects.copyStatus(project)) as Record<string, unknown>;
    return {
      project: asEntry(raw.project),
      agents: asRecord(raw.agents),
      deployModes: asModes(raw.deployModes),
      copies: Array.isArray(raw.copies) ? raw.copies.map(asCopyRecord) : [],
      rules: asStringArray(raw.rules),
    };
  },
  async saveAndSync(path, agents, deployModes) {
    const raw = (await Projects.saveAndSync(path, agents, deployModes)) as Record<
      string,
      unknown
    >;
    return { count: typeof raw.count === "number" ? raw.count : 0 };
  },
  async retryCopies(project) {
    const raw = await Projects.retryCopies(project);
    return Array.isArray(raw) ? raw.map(asCopyRecord) : [];
  },
  async forgetCopy(project, rel, skill) {
    await Projects.forgetCopy(project, rel, skill);
    return null;
  },
  async register(path) {
    return asEntry(await Projects.register(path));
  },
  async updatePath(name, path) {
    await Projects.updatePath(name, path);
    return null;
  },
  async remove(name) {
    await Projects.remove(name);
    return null;
  },
  async pickFolder(prompt) {
    const picked = await Projects.pickFolder(prompt);
    return typeof picked === "string" && picked.length > 0 ? picked : null;
  },
  async pendingProjectSkillPlan(projectRoot) {
    const raw = await Projects.pendingProjectSkillPlan(projectRoot);
    return asPendingPlan(raw);
  },
  async approveProjectSkillPlan(planId, planHash) {
    const raw = (await Projects.approveProjectSkillPlan(planId, planHash)) as Record<
      string,
      unknown
    >;
    return {
      planId: asString(raw.planId),
      planHash: asString(raw.planHash),
      source: raw.source === "elicitation" ? "elicitation" : "skillstar",
      recordedAt: asString(raw.recordedAt),
    };
  },
};

function asPendingPlan(value: unknown): PendingProjectSkillPlan | null {
  if (value === null || value === undefined || typeof value !== "object") return null;
  const rec = value as Record<string, unknown>;
  const planId = asString(rec.planId);
  if (planId === "") return null;
  const opsRaw = Array.isArray(rec.operations) ? rec.operations : [];
  const operations: PendingPlanOperation[] = opsRaw.map((item) => {
    const op = (item ?? {}) as Record<string, unknown>;
    return {
      name: asString(op.name),
      action: op.action === "already" ? "already" : "create",
      skillPath: asString(op.skillPath),
    };
  });
  return {
    planId,
    planHash: asString(rec.planHash),
    canonicalRoot: asString(rec.canonicalRoot),
    isRegistered: rec.isRegistered === true,
    owner: asString(rec.owner),
    affectedAgents: asStringArray(rec.affectedAgents),
    operations,
    expiresAt: asString(rec.expiresAt),
    approved: rec.approved === true,
    approvalSource:
      rec.approvalSource === "elicitation" || rec.approvalSource === "skillstar"
        ? rec.approvalSource
        : null,
  };
}
