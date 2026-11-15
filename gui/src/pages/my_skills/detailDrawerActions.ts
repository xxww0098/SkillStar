// 详情抽屉动作编排 — commands.rs 的 UI 半（dispatch_card_command ToggleAgent /
// set_skill_agent_links / uninstall_skill）+ 乐观翻链（flip/restore_agent_link）
// 的 TS 对位。渲染留在 detailDrawerSlot.tsx，这里只做数据流，纯信号与
// Promise 可无头测试（deck 页 detailDrawerActions.ts 同款分工）。
//
// pending 键 `skill:::agent`（commands.rs pending_agents）：挡住重复点击；
// 不进 state.ts——选择状态模型归基座，pending 是抽屉的域簿记。

import { effect, signal } from "nsh";
import { MySkills, MySkillsDrawer } from "skillstar-gui/native";
import { t, tf } from "../../services/translation";
import { publishToast } from "../../shell/events";
import {
  busy,
  profiles,
  refresh,
  selectedBatch,
  selectedSkill,
  skills,
} from "./state";
import type { AgentProfileRecord } from "./filters";
import {
  batchLinkNotice,
  masterSwitchAgentIds,
  masterSwitchEnable,
  profileIsLinked,
  toggleSkipMessage,
  type AgentLinkReport,
  type ToggleOutcome,
} from "./detailFacts";

/** MySkillsDrawer.upstreamNote 的桥形状（快照丢的 baselineMissing 在此）。 */
export type UpstreamDetail =
  | { kind: "removed" }
  | { kind: "identity_changed"; upstreamName: string }
  | { kind: "local_changes"; baselineMissing: boolean }
  | null;

/** 抽屉自己的域命令（commands.rs ToggleAgent / set_skill_agent_links +
 * 卸载 + 本地改动基线读 + 每条命令尾部的页面重载）。 */
export interface DrawerSiblingSkill {
  name: string;
  source: string;
  folderPath: string;
}

export interface ModifiedAgentCopyDto {
  skillName: string;
  agentId: string;
  agentDisplayName: string;
  targetDir: string;
}

export interface DrawerExtrasDto {
  called: string | null;
  besideSkills: DrawerSiblingSkill[];
  modifiedCopies: ModifiedAgentCopyDto[];
}

export interface DrawerOps {
  toggleAgent(skill: string, agentId: string, enable: boolean): Promise<ToggleOutcome>;
  setAgentLinks(
    skill: string,
    agentIds: string[],
    enable: boolean
  ): Promise<AgentLinkReport>;
  uninstallSkill(name: string): Promise<void>;
  /** 单技能上游详情（快照丢字段，基线缺失在此补；未安装返回 null）。 */
  upstreamNote(name: string): Promise<UpstreamDetail>;
  inspectDrawerExtras?(skill: string): Promise<DrawerExtrasDto>;
  takeAgentCopyEdits?(skill: string, agentId: string): Promise<void>;
  overwriteAgentCopyWithBackup?(
    skill: string,
    agentId: string,
  ): Promise<{ backupId: string; path: string }>;
  /** 命令尾部的 refresh（GPUI spawn_domain 回调的 this.refresh(cx)）；
   * 测试注入假件以免真实重载冲掉装置夹具。 */
  refresh(): Promise<void>;
}

/** 默认绑定：MySkillsDrawer facade + MySkills.uninstallSkill + 页面
 * refresh。生成的声明类型是 JsonValue；facade 的形状即这些接口（桥的信
 * 任边，deck 同款）。 */
export const drawerOps: DrawerOps = {
  toggleAgent: (skill, agentId, enable) =>
    MySkillsDrawer.toggleAgent(skill, agentId, enable) as Promise<ToggleOutcome>,
  setAgentLinks: (skill, agentIds, enable) =>
    MySkillsDrawer.setAgentLinks(skill, agentIds, enable) as Promise<AgentLinkReport>,
  uninstallSkill: (name) => MySkills.uninstallSkill(name),
  upstreamNote: (name) => MySkillsDrawer.upstreamNote(name) as Promise<UpstreamDetail>,
  inspectDrawerExtras: (skill) =>
    MySkillsDrawer.inspectDrawerExtras(skill) as Promise<DrawerExtrasDto>,
  takeAgentCopyEdits: (skill, agentId) =>
    MySkillsDrawer.takeAgentCopyEdits(skill, agentId) as Promise<void>,
  overwriteAgentCopyWithBackup: (skill, agentId) =>
    MySkillsDrawer.overwriteAgentCopyWithBackup(skill, agentId) as Promise<{
      backupId: string;
      path: string;
    }>,
  refresh: () => refresh(),
};

// ---- 抽屉局部状态（页面 keep-alive 单实体） -----------------------------------

/** 进行中的 agent 链接变更（commands.rs pending_agents：`skill:::agent`）。 */
export const pendingAgents = signal<ReadonlySet<string>>(new Set());

/** 本地改动的基线缺失补读（upstreamNote 异步落地的 per-skill 缓存）。 */
export const baselineMissing = signal<Record<string, boolean>>({});

// 打开的技能变化时补读上游详情（换卡重读；迟到响应按当前打开丢弃，
// mod.rs epoch 纪律）。
effect(() => {
  const name = selectedSkill();
  if (name === null) return;
  void drawerOps
    .upstreamNote(name)
    .then((note) => {
      if (selectedSkill() !== name) return;
      if (note !== null && note.kind === "local_changes") {
        baselineMissing.set({ ...baselineMissing(), [name]: note.baselineMissing });
      }
    })
    .catch(() => {
      // 读不到就按已记基线显示（detail_facts 默认分支）。
    });
});

// ---- 乐观翻链（commands.rs flip_agent_link / restore_agent_link） ------------

function flipAgentLink(name: string, displayName: string, enable: boolean): boolean {
  const list = skills();
  const index = list.findIndex((record) => record.name === name);
  if (index < 0) return false;
  const record = list[index];
  const links = record.agentLinks;
  const wasLinked = links.some((link) => link === displayName);
  let next = links;
  if (enable && !wasLinked) next = [...links, displayName];
  else if (!enable) next = links.filter((link) => link !== displayName);
  const copy = list.slice();
  copy[index] = { ...record, agentLinks: next };
  skills.set(copy);
  return wasLinked;
}

function restoreAgentLink(name: string, displayName: string, wasLinked: boolean): void {
  const list = skills();
  const index = list.findIndex((record) => record.name === name);
  if (index < 0) return;
  const record = list[index];
  let links = record.agentLinks.filter((link) => link !== displayName);
  if (wasLinked) links = [...links, displayName];
  const copy = list.slice();
  copy[index] = { ...record, agentLinks: links };
  skills.set(copy);
}

function dropPending(keys: readonly string[]): void {
  const next = new Set(pendingAgents());
  for (const key of keys) next.delete(key);
  pendingAgents.set(next);
}

// ---- 三条命令 -----------------------------------------------------------------

/** 单行开关（dispatch_card_command ToggleAgent）：pending 挡重复，乐观翻
 * 链，skip/失败回滚 + 通知带，尾部 refresh。 */
export function runToggle(
  ops: DrawerOps,
  name: string,
  agentId: string,
  enable: boolean
): void {
  const key = `${name}::${agentId}`;
  if (pendingAgents().has(key)) return;
  const displayName = profiles().find((profile) => profile.id === agentId)?.displayName;
  if (displayName === undefined) return;
  pendingAgents.set(new Set([...pendingAgents(), key]));
  const wasLinked = flipAgentLink(name, displayName, enable);
  ops
    .toggleAgent(name, agentId, enable)
    .then((outcome) => {
      if (outcome.kind === "skipped") {
        restoreAgentLink(name, displayName, wasLinked);
        publishToast({
          tone: "warning",
          message: toggleSkipMessage(name, outcome),
        });
      }
    })
    .catch((err: unknown) => {
      restoreAgentLink(name, displayName, wasLinked);
      publishToast({
        tone: "error",
        message: `${t("common.failed")}: ${String(err)}`,
      });
    })
    .finally(() => {
      dropPending([key]);
      void ops.refresh();
    });
}

/** master 开关（set_skill_agent_links）：整表一事务，只动未到位的行。 */
export function runMasterToggle(
  ops: DrawerOps,
  name: string,
  agents: readonly AgentProfileRecord[]
): void {
  const pairs: [string, boolean][] = agents.map((agent) => [
    agent.id,
    profileIsLinked(agent, currentLinks(name)),
  ]);
  const enable = masterSwitchEnable(pairs);
  if (enable === null) return;
  const ids = masterSwitchAgentIds(pairs, enable);
  if (ids.length === 0) return;
  const prefix = `${name}::`;
  if ([...pendingAgents()].some((key) => key.startsWith(prefix))) return;
  const snaps: { displayName: string; wasLinked: boolean }[] = [];
  for (const id of ids) {
    const displayName = agents.find((agent) => agent.id === id)?.displayName;
    if (displayName === undefined) continue;
    snaps.push({ displayName, wasLinked: flipAgentLink(name, displayName, enable) });
  }
  pendingAgents.set(new Set([...pendingAgents(), ...ids.map((id) => `${name}::${id}`)]));
  ops
    .setAgentLinks(name, ids, enable)
    .then((report) => {
      const notice = batchLinkNotice(report);
      if (notice !== null) publishToast({ tone: "warning", message: notice });
    })
    .catch((err: unknown) => {
      for (const snap of snaps) {
        restoreAgentLink(name, snap.displayName, snap.wasLinked);
      }
      publishToast({
        tone: "warning",
        message: `${t("common.failed")}: ${String(err)}`,
      });
    })
    .finally(() => {
      dropPending(ids.map((id) => `${name}::${id}`));
      void ops.refresh();
    });
}

function currentLinks(name: string): string[] {
  return skills().find((record) => record.name === name)?.agentLinks ?? [];
}

/** 卸载确认的提交（uninstall_skill）：busy=名字（按钮转「卸载中」）、清批
 * 量选择里的名字，失败走通知带，尾部 refresh。抽屉关闭由确认回调先做
 * （GPUI select_detail(None) 顺序）。 */
export function runUninstall(ops: DrawerOps, name: string): void {
  busy.set(name);
  const batch = selectedBatch();
  if (batch.has(name)) {
    const kept = new Set(batch);
    kept.delete(name);
    selectedBatch.set(kept);
  }
  ops
    .uninstallSkill(name)
    .catch((err: unknown) => {
      publishToast({
        tone: "error",
        message: tf("mySkills.uninstallFailedReason", { err: String(err) }),
      });
    })
    .finally(() => {
      busy.set(null);
      void ops.refresh();
    });
}
