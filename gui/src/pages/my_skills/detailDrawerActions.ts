// 详情抽屉动作编排 — commands.rs 的 UI 半（dispatch_card_command ToggleAgent /
// set_skill_agent_links / uninstall_skill）+ 乐观翻链（flip/restore_agent_link）
// 的 TS 对位。渲染留在 detailDrawerSlot.tsx，这里只做数据流，纯信号与
// Promise 可无头测试（deck 页 detailDrawerActions.ts 同款分工）。
//
// pending 键 `skill:::agent`（commands.rs pending_agents）：挡住重复点击；
// 不进 state.ts——选择状态模型归基座，pending 是抽屉的域簿记。

import { effect, signal, untracked } from "nsh";
import { MySkills, MySkillsDrawer, SettingsStorage } from "skillstar-gui/native";
import { beginSave, failSave, finishSave } from "../../chrome/saveStatus";
import { t, tf } from "../../services/translation";
import { publishToast } from "../../shell/events";
import { et, etf } from "./extrasStrings";
import {
  busy,
  openDetail,
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

export interface UnmanagedAgentEntryDto {
  skillName: string;
  agentId: string;
  agentDisplayName: string;
  targetDir: string;
  isSymlink: boolean;
  canAdoptToHub: boolean;
}

export interface DrawerExtrasDto {
  called: string | null;
  besideSkills: DrawerSiblingSkill[];
  modifiedCopies: ModifiedAgentCopyDto[];
  unmanagedEntries?: UnmanagedAgentEntryDto[];
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
  takeOverUnmanagedAgentEntry?(
    skill: string,
    agentId: string,
  ): Promise<{ backupId: string | null; path: string | null }>;
  adoptUnmanagedAgentEntry?(skill: string, agentId: string): Promise<void>;
  revealPath?(path: string): void;
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
  takeOverUnmanagedAgentEntry: (skill, agentId) =>
    MySkillsDrawer.takeOverUnmanagedAgentEntry(skill, agentId) as Promise<{
      backupId: string | null;
      path: string | null;
    }>,
  adoptUnmanagedAgentEntry: (skill, agentId) =>
    MySkillsDrawer.adoptUnmanagedAgentEntry(skill, agentId) as Promise<void>,
  revealPath: (path) => {
    SettingsStorage.revealPath(path);
  },
  refresh: () => refresh(),
};

// ---- 抽屉局部状态（页面 keep-alive 单实体） -----------------------------------

/** 进行中的 agent 链接变更（commands.rs pending_agents：`skill:::agent`）。 */
export const pendingAgents = signal<ReadonlySet<string>>(new Set());

/** 本地改动的基线缺失补读（upstreamNote 异步落地的 per-skill 缓存）。 */
export const baselineMissing = signal<Record<string, boolean>>({});

/** 冲突/扩展面板版本戳：链接切换或恢复动作落地时递增，触发已打开的抽屉重查。 */
export const extrasVersion = signal(0);

export function bumpExtrasVersion(): void {
  extrasVersion.set(untracked(() => extrasVersion()) + 1);
}

// 打开的技能变化时补读上游详情（换卡重读；迟到响应按当前打开丢弃，
// mod.rs epoch 纪律）。快照已带 baselineMissing 时不再全表重扫。
effect(() => {
  const name = selectedSkill();
  if (name === null) return;
  const current = untracked(skills).find((record) => record.name === name);
  if (
    current !== undefined &&
    (current.upstreamChange?.kind !== "local_changes" ||
      current.upstreamChange.baselineMissing !== undefined)
  ) {
    return;
  }
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

// ---- 未受管冲突接管与开关编排 ------------------------------------------------

const TOGGLE_SKIP_REPLACE_KEY = "skill-toggle-skip";

/** 未受管冲突跳过项的接管按钮文案；不属于未受管目录/外来链接时返回 null。 */
export function unmanagedSkipActionLabel(code: string): string | null {
  if (code === "unmanaged_real_directory") {
    return t("skillToggle.backupAndTakeOver");
  }
  if (code === "foreign_link") {
    return t("skillToggle.takeOverLink");
  }
  return null;
}

/** 备份并接管未受管同名目录/链接（可从通知带按钮或抽屉冲突卡片触发）。 */
export function runTakeOverUnmanaged(
  ops: DrawerOps,
  name: string,
  agentId: string,
  toastReplaceKey?: string,
): void {
  const fn = ops.takeOverUnmanagedAgentEntry;
  if (fn === undefined) return;
  const key = `${name}::${agentId}`;
  if (pendingAgents().has(key)) return;
  const displayName =
    profiles().find((profile) => profile.id === agentId)?.displayName ?? agentId;
  pendingAgents.set(new Set([...pendingAgents(), key]));
  const wasLinked = flipAgentLink(name, displayName, true);
  const saveId = `link:${name}:${agentId}`;
  beginSave(saveId);
  fn(name, agentId)
    .then((res) => {
      finishSave(saveId);
      const message =
        typeof res.backupId === "string" && res.backupId.length > 0
          ? etf("mySkills.takeOverDoneWithBackup", {
              id: res.backupId,
              agent: displayName,
            })
          : etf("mySkills.takeOverDone", { agent: displayName });
      publishToast({
        tone: "success",
        message,
        ...(toastReplaceKey !== undefined ? { replaceKey: toastReplaceKey } : {}),
      });
    })
    .catch((err: unknown) => {
      restoreAgentLink(name, displayName, wasLinked);
      const message = `${et("mySkills.takeOverFailed")}: ${String(err)}`;
      publishToast({
        tone: "error",
        message,
        pinned: true,
        ...(toastReplaceKey !== undefined ? { replaceKey: toastReplaceKey } : {}),
      });
      failSave(saveId, message, () =>
        runTakeOverUnmanaged(ops, name, agentId, toastReplaceKey),
      );
    })
    .finally(() => {
      dropPending([key]);
      bumpExtrasVersion();
      void ops.refresh();
    });
}

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
  const saveId = `link:${name}:${agentId}`;
  beginSave(saveId);
  const retry = () => runToggle(ops, name, agentId, enable);
  ops
    .toggleAgent(name, agentId, enable)
    .then((outcome) => {
      if (outcome.kind === "skipped") {
        restoreAgentLink(name, displayName, wasLinked);
        const message = toggleSkipMessage(name, outcome);
        const takeOverLabel = unmanagedSkipActionLabel(outcome.code);
        if (
          takeOverLabel !== null &&
          ops.takeOverUnmanagedAgentEntry !== undefined
        ) {
          publishToast({
            tone: "warning",
            message,
            pinned: true,
            replaceKey: TOGGLE_SKIP_REPLACE_KEY,
            action: {
              label: takeOverLabel,
              run: () =>
                runTakeOverUnmanaged(ops, name, agentId, TOGGLE_SKIP_REPLACE_KEY),
            },
          });
        } else if (outcome.code === "modified_agent_copy") {
          publishToast({
            tone: "warning",
            message,
            pinned: true,
            replaceKey: TOGGLE_SKIP_REPLACE_KEY,
            action: {
              label: t("skillToggle.resolveInDrawer"),
              run: () => {
                if (selectedSkill() !== name) openDetail(name);
              },
            },
          });
        } else {
          publishToast({ tone: "warning", message });
        }
        failSave(saveId, message, retry);
        return;
      }
      finishSave(saveId);
    })
    .catch((err: unknown) => {
      restoreAgentLink(name, displayName, wasLinked);
      const message = `${t("common.failed")}: ${String(err)}`;
      publishToast({ tone: "error", message });
      failSave(saveId, message, retry);
    })
    .finally(() => {
      dropPending([key]);
      bumpExtrasVersion();
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
      bumpExtrasVersion();
      void ops.refresh();
    });
}

function currentLinks(name: string): string[] {
  return skills().find((record) => record.name === name)?.agentLinks ?? [];
}

/** 卸载确认的提交（uninstall_skill）：busy=`uninstall:名字`，和单卡更新
 * 的名字 token 分开，更新图标不会跟着转。清批量选择里的名字，失败走
 * 通知带，尾部 refresh。抽屉关闭由确认回调先做。不从列表里提前删掉。 */
export function runUninstall(ops: DrawerOps, name: string): void {
  busy.set(`uninstall:${name}`);
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
