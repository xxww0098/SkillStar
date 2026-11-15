// 我的技能页状态机 — my_skills/mod.rs（+ updates.rs 的批次门/节流 +
// repo_source.rs 的仓内重装/移除）对位。
//
// keep-alive：Shell 每页只挂载一次，模块级 signal 即页面实体（skill_cards
// 页同款）。组件体只执行一次，一切派生读取落在 JSX 表达式里。
//
// 选择状态模型归本页持有（波 3 基座约定）：
// - selectedSkill：详情抽屉的开关与身份（再点同卡收起；列表刷新后名字
//   消失即收起；卸载该技能时收起）；
// - selectedBatch / linkMenuOpen：批量选择与选择条的 agent 菜单开合；
// - busy / updatingBatch：一次只跑一个变更；卡面更新省略号跟点击时
//   的名字快照（updates.rs skill_update_in_flight），不跟后来的选择变化。
// 选择条本体（selection_bar.rs）是并行切片，经 selectionBarSlot 挂点
// 渲染；详情抽屉经 detailDrawerSlot；SKILL.md 阅读器经 skillReaderSlot。

import { computed, signal, effect } from "nsh";
import { MySkills } from "skillstar-gui/native";
import { publishToast, toast } from "../../shell/events";
import { agentsChanged, groupsChanged, translationPrefsChanged } from "../../shell/events";
import { openUninstallConfirm, uninstallTitleCount } from "../../chrome/dialog";
import { t, tf } from "../../services/translation";
import {
  type AgentProfileRecord,
  type SkillRecord,
  type SortOption,
  type SourceFilter,
  type MySkillsScope,
  filteredSkills,
  scopedSkills,
  repoSources,
  needsAttention,
} from "./filters";
import {
  canArmBatch,
  reportNotification,
  shouldBackgroundCheck,
  type UpdateReport,
} from "./updates";

// ---- snapshot 解析（容错：桥层形状漂移降级为空，不砸页面） ----------------

function parseSkill(raw: unknown): SkillRecord {
  const rec = (raw ?? {}) as Record<string, unknown>;
  const change = (rec.upstreamChange ?? null) as Record<string, unknown> | null;
  return {
    name: typeof rec.name === "string" ? rec.name : "",
    description: typeof rec.description === "string" ? rec.description : "",
    localizedDescription:
      typeof rec.localizedDescription === "string" ? rec.localizedDescription : undefined,
    skillType: rec.skillType === "local" ? "local" : "hub",
    stars: typeof rec.stars === "number" ? rec.stars : 0,
    installed: rec.installed !== false,
    updateAvailable: rec.updateAvailable === true,
    upstreamChange:
      change === null
        ? null
        : change.kind === "removed"
          ? { kind: "removed" }
          : change.kind === "identity_changed"
            ? {
                kind: "identity_changed",
                upstreamName: typeof change.upstreamName === "string" ? change.upstreamName : "",
              }
            : { kind: "local_changes" },
    lastUpdated: typeof rec.lastUpdated === "string" ? rec.lastUpdated : "",
    gitUrl: typeof rec.gitUrl === "string" ? rec.gitUrl : "",
    author: typeof rec.author === "string" ? rec.author : undefined,
    agentLinks: Array.isArray(rec.agentLinks)
      ? rec.agentLinks.filter((name): name is string => typeof name === "string")
      : [],
    rank: typeof rec.rank === "number" ? rec.rank : undefined,
    source: typeof rec.source === "string" ? rec.source : undefined,
    called:
      typeof rec.called === "string" && rec.called.trim().length > 0
        ? rec.called.trim()
        : undefined,
  };
}

function parseAgents(raw: unknown): AgentProfileRecord[] {
  if (!Array.isArray(raw)) return [];
  return raw.map((entry) => {
    const rec = (entry ?? {}) as Record<string, unknown>;
    return {
      id: typeof rec.id === "string" ? rec.id : "",
      displayName: typeof rec.displayName === "string" ? rec.displayName : "",
      icon: typeof rec.icon === "string" ? rec.icon : undefined,
      enabled: rec.enabled === true,
      targetable: rec.targetable === true,
    };
  });
}

// ---- 页面实体信号 ----------------------------------------------------------

export const skills = signal<SkillRecord[]>([]);
export const profiles = signal<AgentProfileRecord[]>([]);
export const scope = signal<MySkillsScope>("local");
export const sortBy = signal<SortOption>("smart");
export const sourceFilter = signal<SourceFilter>("all");
export const repoFilter = signal<string | null>(null);
export const agentFilter = signal<string | null>(null);
export const onlyUpdates = signal(false);
export const viewList = signal(false);

/** 视图切换（mod.rs 视图 TabBar on_click）：换模式即换数据集开顶
 * （reset_list_scroll 语义由画布对 viewList 的 effect 执行）。 */
export function setViewList(list: boolean): void {
  viewList.set(list);
}

export const searchQuery = signal("");
export const loading = signal(true);
export const error = signal<string | null>(null);

export const favoriteRepos = signal<ReadonlySet<string>>(new Set());
export const favoritesLocked = signal(false);
/** Bumped on each favorite click so a slower list load cannot wipe it
 * (mod.rs favorites_epoch)。 */
let favoritesEpoch = 0;

/** 仓内重装进行中的来源；其他行的重装按钮锁住（repo_source.rs）。 */
export const reinstallingRepo = signal<string | null>(null);

/** 选择状态模型（本页持有，抽屉/选择条切片经槽位消费）。 */
export const selectedSkill = signal<string | null>(null);
export const selectedBatch = signal<ReadonlySet<string>>(new Set());
export const linkMenuOpen = signal(false);

export const busy = signal<string | null>(null);
export const updatingBatch = signal<ReadonlySet<string>>(new Set());
export const checkingUpdates = signal(false);
/** Start of the last upstream check; throttles the background check
 * (updates.rs last_update_check)。 */
let lastUpdateCheckAt: number | null = null;

/** GitHub 会话（频道空状态的三态门）；null = 尚未读到。 */
export const github = signal<{ state: string; login: string | null } | null>(null);

/** Per-skill description-translation choice（mod.rs description_choices；
 * 选择条的翻译动作与抽屉的语言按钮共用，Settings 开关从不写）。 */
export const descriptionChoices = signal<Record<string, boolean>>({});

// ---- 派生缓存（computed 仅在依赖 signal 变化时重算） -------------------------

export function queryState() {
  return {
    query: searchQuery(),
    sourceFilter: sourceFilter(),
    repoFilter: repoFilter(),
    agentFilterId: agentFilter(),
    onlyUpdates: onlyUpdates(),
    sortBy: sortBy(),
  };
}

const filteredMemo = computed<SkillRecord[]>(() =>
  filteredSkills(skills(), profiles(), queryState()),
);

const scopedMemo = computed<SkillRecord[]>(() =>
  scopedSkills(skills(), profiles(), queryState()),
);

const attentionCountMemo = computed<number>(
  () => skills().filter(needsAttention).length,
);

const pendingUpdateMemo = computed<number>(
  () => skills().filter((skill) => skill.updateAvailable).length,
);

const localCountMemo = computed<number>(
  () => skills().filter((skill) => skill.skillType === "local").length,
);

const reposMemo = computed<string[]>(() => repoSources(skills()));

export const targetableProfiles = computed<AgentProfileRecord[]>(() =>
  profiles().filter((profile) => profile.enabled && profile.targetable),
);

export const enabledProfiles = computed<AgentProfileRecord[]>(() =>
  profiles().filter((profile) => profile.enabled),
);

export function currentFiltered(): SkillRecord[] {
  return filteredMemo();
}

export function currentScoped(): SkillRecord[] {
  return scopedMemo();
}

export function allAttentionCount(): number {
  return attentionCountMemo();
}

export function pendingUpdateCount(): number {
  return pendingUpdateMemo();
}

export function localSkillCount(): number {
  return localCountMemo();
}

export function currentRepos(): string[] {
  return reposMemo();
}

// ---- 装载与刷新 ------------------------------------------------------------

let loadEpoch = 0;

/** refresh (mod.rs refresh_from)：skills + profiles + favorites 一次换齐。
 * 收藏点击先于本装载落地时（favoritesEpoch 变化）保留点击结果。 */
export async function refresh(): Promise<void> {
  const epoch = ++loadEpoch;
  try {
    const snapshot = (await MySkills.snapshot()) as {
      skills: unknown[];
      agents: unknown;
      favorites: string[];
      favoritesLocked: boolean;
    };
    if (epoch !== loadEpoch) return;
    const loadedSkills = (snapshot.skills as unknown[]).map(parseSkill);
    // A Settings toggle may have landed while this listing was in flight:
    // keep the newer snapshot (mod.rs profiles_epoch discipline) — the nsh
    // side receives profile reloads through the agentsChanged channel, whose
    // own refresh supersedes this one by epoch.
    skills.set(loadedSkills);
    profiles.set(parseAgents(snapshot.agents));
    const clicked = favoritesEpoch !== 0;
    if (snapshot.favoritesLocked === true) {
      favoritesLocked.set(true);
      if (!clicked) favoriteRepos.set(new Set());
    } else {
      favoritesLocked.set(false);
      if (!clicked) favoriteRepos.set(new Set(snapshot.favorites as string[]));
    }
    error.set(null);
    loading.set(false);
    // 名字消失的技能不得留在选中态（mod.rs 侧选择保留语义：保留仍存在
    // 的选择；卸载路径在自身动作里清理）。
    retainExisting(loadedSkills);
    try {
      const choices = (await MySkills.descriptionChoices()) as { choices?: Record<string, boolean> };
      if (epoch === loadEpoch) {
        descriptionChoices.set(choices.choices ?? {});
      }
    } catch {
      // 读取失败保持现状：卡面按 localizedDescription 兜底。
    }
    maybeCheckUpdates();
  } catch (err) {
    if (epoch !== loadEpoch) return;
    loading.set(false);
    error.set(String(err));
  }
}

function retainExisting(loaded: SkillRecord[]): void {
  const names = new Set(loaded.map((skill) => skill.name));
  const batch = selectedBatch();
  const keptBatch = new Set([...batch].filter((name) => names.has(name)));
  if (keptBatch.size !== batch.size) selectedBatch.set(keptBatch);
  const selected = selectedSkill();
  if (selected !== null && !names.has(selected)) selectedSkill.set(null);
}

// ---- 更新检查（updates.rs） --------------------------------------------------

function maybeCheckUpdates(): void {
  if (
    shouldBackgroundCheck(skills(), lastUpdateCheckAt, Date.now(), busy()) &&
    !checkingUpdates()
  ) {
    void checkUpdates(false);
  }
}

export async function checkUpdates(manual: boolean): Promise<void> {
  if (checkingUpdates()) return;
  checkingUpdates.set(true);
  lastUpdateCheckAt = Date.now();
  try {
    const result = (await MySkills.checkUpdates()) as { pending: number };
    const pending = typeof result.pending === "number" ? result.pending : 0;
    if (manual) {
      toast(
        pending === 0
          ? t("mySkills.checkUpdatesNone")
          : tf("mySkills.checkUpdatesFound", { count: pending }),
        pending === 0 ? "success" : "info"
      );
    }
  } catch (err) {
    if (manual) {
      toast(`${t("mySkills.checkUpdatesFailed")}: ${String(err)}`, "error");
    }
    // 静默后台检查失败只留 debug（updates.rs 同款）。
  } finally {
    checkingUpdates.set(false);
    await refresh();
  }
}

/** 工具栏刷新：手动检查 → toast → 重载（toolbar.rs bar_refresh_button）。 */
export function refreshAndCheck(): void {
  if (checkingUpdates() || loading()) return;
  loading.set(true);
  void checkUpdates(true);
}

// ---- 批量更新（updates.rs run_batch_update） --------------------------------

export function updateAllPending(): void {
  const names = skills()
    .filter((skill) => skill.updateAvailable)
    .map((skill) => skill.name);
  void runBatchUpdate(names, "update_all");
}

export function batchUpdateSelected(): void {
  const batch = selectedBatch();
  const names = skills()
    .filter((skill) => batch.has(skill.name) && skill.updateAvailable)
    .map((skill) => skill.name);
  void runBatchUpdate(names, "batch_update");
}

export function updateSkill(name: string): void {
  void runBatchUpdate([name], name);
}

async function runBatchUpdate(names: string[], token: string): Promise<void> {
  if (!canArmBatch(names, busy())) return;
  updatingBatch.set(new Set(names));
  busy.set(token);
  try {
    const report = (await MySkills.updateSkills(names)) as unknown as UpdateReport;
    const notice = reportNotification(report, { t, tf });
    if (notice) {
      publishToast({
        tone: notice.tone,
        message: notice.message,
        replaceKey: "skill-update",
        pinned: notice.pinned || undefined,
      });
    }
  } catch (err) {
    toast(`${t("mySkills.updateFailed")}: ${String(err)}`, "error");
  } finally {
    busy.set(null);
    updatingBatch.set(new Set());
    await refresh();
  }
}

// ---- 收藏（mod.rs toggle_repo_favorite） ------------------------------------

export async function toggleFavoriteRepo(source: string): Promise<void> {
  if (favoritesLocked() || source.length === 0) return;
  const previous = favoriteRepos();
  const next = new Set(previous);
  if (!next.delete(source)) next.add(source);
  favoritesEpoch += 1;
  favoriteRepos.set(next);
  try {
    const result = (await MySkills.toggleFavoriteRepo(source)) as {
      favorites: string[];
      locked: boolean;
    };
    favoritesEpoch += 1;
    if (result.locked === true) {
      favoritesLocked.set(true);
      favoriteRepos.set(new Set(result.favorites as string[]));
      toast(t("toolbar.favoriteSourceLocked"), "error");
      return;
    }
    favoriteRepos.set(new Set(result.favorites as string[]));
  } catch {
    favoritesEpoch += 1;
    favoriteRepos.set(previous);
    toast(t("toolbar.favoriteSourceFailed"), "error");
  }
}

// ---- 来源菜单的仓内动作（repo_source.rs） -----------------------------------

export async function reinstallRepoSource(source: string): Promise<void> {
  if (reinstallingRepo() !== null) return;
  const donor = skills().find(
    (skill) =>
      skill.source === source && skill.skillType === "hub" && skill.gitUrl.length > 0
  );
  if (!donor) {
    toast(tf("mySkills.reinstallRepoSourceMissing", { source }), "error");
    return;
  }
  reinstallingRepo.set(source);
  try {
    const result = (await MySkills.reinstallRepoSource(donor.gitUrl)) as number;
    const count = typeof result === "number" ? result : 0;
    toast(tf("mySkills.reinstallRepoSuccess", { count }), "success");
  } catch (err) {
    toast(`${tf("mySkills.reinstallRepoFailed", { source })}\n${String(err)}`, "error");
  } finally {
    reinstallingRepo.set(null);
    await refresh();
  }
}

export function uninstallRepoSource(source: string): void {
  const names = skills()
    .filter((skill) => skill.source === source)
    .map((skill) => skill.name);
  if (names.length === 0) return;
  if (repoFilter() === source) repoFilter.set(null);
  openUninstallConfirm(uninstallTitleCount(names.length), () => {
    void runRepoUninstall(source, names);
    return true;
  });
}

async function runRepoUninstall(source: string, names: string[]): Promise<void> {
  // repo_source.rs：先清选择/抽屉（这些名字即将消失），busy 带来源 token。
  const batch = selectedBatch();
  const kept = new Set([...batch].filter((name) => !names.includes(name)));
  selectedBatch.set(kept);
  if (selectedSkill() !== null && names.includes(selectedSkill() as string)) {
    selectedSkill.set(null);
  }
  busy.set(`repo-remove:${source}`);
  try {
    await MySkills.uninstallRepoSkills(names);
  } catch (err) {
    toast(String(err), "error");
  } finally {
    busy.set(null);
    await refresh();
  }
}

// ---- 选择与筛选状态操作（commands.rs on_skill_card 的 Open/Select 半） -------

/** 卡面主点击：开抽屉；再点同卡收起（on_skill_card Open）。 */
export function openDetail(name: string): void {
  selectedSkill.set(selectedSkill() === name ? null : name);
}

/** 头像选择点击：进出批量选择（on_skill_card Select）。 */
export function toggleBatchMember(name: string): void {
  const next = new Set(selectedBatch());
  if (!next.delete(name)) next.add(name);
  selectedBatch.set(next);
}

export function clearBatch(): void {
  selectedBatch.set(new Set());
  linkMenuOpen.set(false);
}

/** clear_filters (mod.rs)：清 query 与全部筛选，不动排序与视图。 */
export function clearFilters(): void {
  searchQuery.set("");
  sourceFilter.set("all");
  repoFilter.set(null);
  agentFilter.set(null);
  onlyUpdates.set(false);
}

// ---- keep-alive 频道 ---------------------------------------------------------

// 首挂载装载一次（页面实体构造即 refresh）。
effect(() => {
  void refresh();
  void readGithubStatus();
});
effect(() => agentsChanged.subscribe(() => void refresh()));
effect(() => groupsChanged.subscribe(() => void refresh()));
effect(() => translationPrefsChanged.subscribe(() => void refresh()));

/** GitHub 会话：失败/未读到保持 Unknown（mod.rs 侧 None 分支）。 */
async function readGithubStatus(): Promise<void> {
  try {
    const status = (await MySkills.githubStatus()) as { state: string; login: string | null };
    github.set({ state: String(status.state ?? "unknown"), login: status.login ?? null });
  } catch {
    github.set(null);
  }
}

/** 卡面本地徽章点击：打开技能目录（skill_card OpenFolder）。 */
export function openSkillFolder(name: string): void {
  void MySkills.openSkillFolder(name).catch((err: unknown) => {
    toast(`${t("common.failed")}: ${String(err)}`, "error");
  });
}
