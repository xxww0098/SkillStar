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

import { computed, effect, signal, untracked } from "nsh";
import { MySkills } from "skillstar-gui/native";
import {
  agentsChanged,
  groupsChanged,
  publishToast,
  requestChannelsScope,
  selectPage,
  toast,
  translationPrefsChanged,
} from "../../shell/events";
import { currentPage } from "../../shell/nav";
import { SLOW_MS } from "../../chrome/delayedFlag";
import { openUninstallConfirm, uninstallTitleCount } from "../../chrome/dialog";
import { beginSave, failSave, finishSave, saveMark } from "../../chrome/saveStatus";
import { endJob, putJob } from "../../shell/jobBoard";
import { t, tf } from "../../services/translation";
import { rt } from "./readerStrings";
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
            : {
                kind: "local_changes",
                baselineMissing:
                  typeof change.baselineMissing === "boolean" ? change.baselineMissing : undefined,
              },
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

/** 切到共享频道 / 切回本地工作台；切回本地且存在脏标记时立即补刷。 */
export function setScope(next: MySkillsScope): void {
  scope.set(next);
  if (next === "local" && refreshDirty && untracked(pageActive)) {
    refreshDirty = false;
    void refresh();
  }
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
/** 只有手动「刷新并检查」为真。后台检查不转工具栏图标。 */
export const manualChecking = signal(false);
/** Start of the last upstream check; throttles the background check
 * (updates.rs last_update_check)。 */
let lastUpdateCheckAt: number | null = null;

/** GitHub 会话（频道空状态的三态门）；null = 尚未读到。 */
export const github = signal<{ state: string; login: string | null } | null>(null);

/** Per-skill description-translation choice（mod.rs description_choices；
 * 选择条的翻译动作与抽屉的语言按钮共用，Settings 开关从不写）。 */
export const descriptionChoices = signal<Record<string, boolean>>({});

// ---- 派生缓存（computed 仅在依赖 signal 变化且页面活跃时重算） ----------------

export function pageActive(): boolean {
  return currentPage() === "my_skills";
}

function sameElements<T>(prev: readonly T[], next: readonly T[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

function gated<T>(
  read: () => T,
  fallback: T,
  equals?: (prev: T, next: T) => boolean,
) {
  return computed<T>(
    (prev) => (!pageActive() ? (prev !== undefined ? prev : fallback) : read()),
    equals ? { equals } : undefined,
  );
}

function stringArraysEqual(a: readonly string[], b: readonly string[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function upstreamChangeEqual(
  a: SkillRecord["upstreamChange"],
  b: SkillRecord["upstreamChange"],
): boolean {
  if (a === b) return true;
  if (a === null || b === null || a.kind !== b.kind) return false;
  if (a.kind === "identity_changed" && b.kind === "identity_changed") {
    return a.upstreamName === b.upstreamName;
  }
  if (a.kind === "local_changes" && b.kind === "local_changes") {
    return a.baselineMissing === b.baselineMissing;
  }
  return true;
}

function skillRecordEqual(a: SkillRecord, b: SkillRecord): boolean {
  return (
    a === b ||
    (a.name === b.name &&
      a.description === b.description &&
      a.localizedDescription === b.localizedDescription &&
      a.skillType === b.skillType &&
      a.stars === b.stars &&
      a.installed === b.installed &&
      a.updateAvailable === b.updateAvailable &&
      upstreamChangeEqual(a.upstreamChange, b.upstreamChange) &&
      a.lastUpdated === b.lastUpdated &&
      a.gitUrl === b.gitUrl &&
      a.author === b.author &&
      a.rank === b.rank &&
      a.source === b.source &&
      a.called === b.called &&
      stringArraysEqual(a.agentLinks, b.agentLinks))
  );
}

function mergeSkills(prev: readonly SkillRecord[], next: SkillRecord[]): SkillRecord[] {
  if (prev.length === 0) return next;
  const byName = new Map<string, SkillRecord>();
  for (const item of prev) byName.set(item.name, item);
  return next.map((item) => {
    const existing = byName.get(item.name);
    return existing !== undefined && skillRecordEqual(existing, item) ? existing : item;
  });
}

function mergeProfiles(
  prev: readonly AgentProfileRecord[],
  next: AgentProfileRecord[],
): AgentProfileRecord[] {
  if (prev.length === 0) return next;
  const byId = new Map<string, AgentProfileRecord>();
  for (const item of prev) byId.set(item.id, item);
  return next.map((item) => {
    const existing = byId.get(item.id);
    return existing !== undefined &&
      existing.displayName === item.displayName &&
      existing.icon === item.icon &&
      existing.enabled === item.enabled &&
      existing.targetable === item.targetable
      ? existing
      : item;
  });
}

function setsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a === b) return true;
  if (a.size !== b.size) return false;
  for (const item of a) {
    if (!b.has(item)) return false;
  }
  return true;
}

function choicesEqual(a: Record<string, boolean>, b: Record<string, boolean>): boolean {
  if (a === b) return true;
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}

function scopedQueryState() {
  return {
    query: searchQuery(),
    sourceFilter: sourceFilter(),
    repoFilter: repoFilter(),
    agentFilterId: agentFilter(),
  };
}

export function queryState() {
  return {
    ...scopedQueryState(),
    onlyUpdates: onlyUpdates(),
    sortBy: sortBy(),
  };
}

const filteredMemo = gated<SkillRecord[]>(
  () => filteredSkills(skills(), profiles(), queryState()),
  [],
  sameElements,
);

const scopedMemo = gated<SkillRecord[]>(
  () => scopedSkills(skills(), profiles(), scopedQueryState()),
  [],
  sameElements,
);

const attentionCountMemo = gated<number>(
  () => skills().filter(needsAttention).length,
  0,
);

const pendingUpdateMemo = gated<number>(
  () => skills().filter((skill) => skill.updateAvailable).length,
  0,
);

const localCountMemo = gated<number>(
  () => skills().filter((skill) => skill.skillType === "local").length,
  0,
);

const reposMemo = gated<string[]>(() => repoSources(skills()), [], sameElements);

export const targetableProfiles = gated<AgentProfileRecord[]>(
  () => profiles().filter((profile) => profile.enabled && profile.targetable),
  [],
  sameElements,
);

export const enabledProfiles = gated<AgentProfileRecord[]>(
  () => profiles().filter((profile) => profile.enabled),
  [],
  sameElements,
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
let snapshotLoader: () => Promise<unknown> = () => MySkills.snapshot();

/** 测试注入位：替换快照读取，避免无头单测读写真实 $HOME。 */
export function setSnapshotLoaderForTest(loader?: () => Promise<unknown>): void {
  snapshotLoader = loader ?? (() => MySkills.snapshot());
}

/** refresh (mod.rs refresh_from)：skills + profiles + favorites 一次换齐。
 * 收藏点击先于本装载落地时（favoritesEpoch 变化）保留点击结果。 */
export async function refresh(): Promise<void> {
  const epoch = ++loadEpoch;
  try {
    const snapshot = (await snapshotLoader()) as {
      skills: unknown[];
      agents: unknown;
      favorites: string[];
      favoritesLocked: boolean;
      descriptionChoices?: Record<string, boolean>;
    };
    if (epoch !== loadEpoch) return;
    const loadedSkills = (snapshot.skills as unknown[]).map(parseSkill);
    const mergedSkills = mergeSkills(untracked(skills), loadedSkills);
    if (!sameElements(untracked(skills), mergedSkills)) {
      skills.set(mergedSkills);
    }
    const mergedProfiles = mergeProfiles(untracked(profiles), parseAgents(snapshot.agents));
    if (!sameElements(untracked(profiles), mergedProfiles)) {
      profiles.set(mergedProfiles);
    }
    const clicked = favoritesEpoch !== 0;
    if (snapshot.favoritesLocked === true) {
      favoritesLocked.set(true);
      if (!clicked && untracked(favoriteRepos).size > 0) favoriteRepos.set(new Set());
    } else {
      favoritesLocked.set(false);
      if (!clicked) {
        const nextFavs = new Set(snapshot.favorites as string[]);
        if (!setsEqual(untracked(favoriteRepos), nextFavs)) {
          favoriteRepos.set(nextFavs);
        }
      }
    }
    error.set(null);
    loading.set(false);
    // 名字消失的技能不得留在选中态（mod.rs 侧选择保留语义：保留仍存在
    // 的选择；卸载路径在自身动作里清理）。
    retainExisting(mergedSkills);
    if (snapshot.descriptionChoices !== undefined) {
      if (!choicesEqual(untracked(descriptionChoices), snapshot.descriptionChoices)) {
        descriptionChoices.set(snapshot.descriptionChoices);
      }
    } else {
      try {
        const choices = (await MySkills.descriptionChoices()) as { choices?: Record<string, boolean> };
        if (epoch === loadEpoch) {
          const nextChoices = choices.choices ?? {};
          if (!choicesEqual(untracked(descriptionChoices), nextChoices)) {
            descriptionChoices.set(nextChoices);
          }
        }
      } catch {
        // 读取失败保持现状：卡面按 localizedDescription 兜底。
      }
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
  if (manual) manualChecking.set(true);
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
    try {
      await refresh();
    } finally {
      if (manual) manualChecking.set(false);
    }
  }
}

/** 工具栏刷新：只手动检查转图标，列表留在原地（不亮骨架）。 */
export function refreshAndCheck(): void {
  if (checkingUpdates() || loading()) return;
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
  const batch = token === "update_all" || token === "batch_update";
  const single = !batch && names.length === 1 ? names[0] : null;
  updatingBatch.set(new Set(names));
  busy.set(token);
  // 一批一次 updateSkills，没有逐条进度，也不编完成数。
  // 更新只在原地转图标（工具栏的更新全部、选择条、卡面上的更新图标），
  // 不出右上角任务卡片：结果由这一次请求的 toast 说明。
  try {
    const report = (await MySkills.updateSkills(names)) as unknown as UpdateReport;
    const notice = reportNotification(report, { t, tf });
    if (notice) {
      publishToast({
        tone: notice.tone,
        message: notice.message,
        replaceKey: "skill-update",
        pinned: notice.pinned || undefined,
        action: notice.actionLabelKey
          ? { label: rt(notice.actionLabelKey), run: () => selectPage.publish("projects") }
          : undefined,
      });
    }
    if (single) {
      const failed = (report.failed ?? []).find((row) => row.name === single);
      const mark = `update:${single}`;
      if (failed) failSave(mark, failed.error, () => updateSkill(single));
      else if (saveMark(mark)) finishSave(mark);
    }
  } catch (err) {
    const reason = `${t("mySkills.updateFailed")}: ${String(err)}`;
    if (single) failSave(`update:${single}`, reason, () => updateSkill(single));
    toast(reason, "error");
  } finally {
    busy.set(null);
    updatingBatch.set(new Set());
    await refresh();
  }
}

// ---- 收藏（mod.rs toggle_repo_favorite） ------------------------------------

export async function toggleFavoriteRepo(source: string): Promise<void> {
  if (favoritesLocked() || source.length === 0) return;
  const id = `fav:${source}`;
  const previous = favoriteRepos();
  const next = new Set(previous);
  if (!next.delete(source)) next.add(source);
  favoritesEpoch += 1;
  favoriteRepos.set(next);
  beginSave(id);
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
      failSave(id, t("toolbar.favoriteSourceLocked"));
      return;
    }
    favoriteRepos.set(new Set(result.favorites as string[]));
    finishSave(id);
  } catch {
    favoritesEpoch += 1;
    favoriteRepos.set(previous);
    toast(t("toolbar.favoriteSourceFailed"), "error");
    failSave(id, t("toolbar.favoriteSourceFailed"), () => {
      void toggleFavoriteRepo(source);
    });
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
  const jobId = `reinstall:${source}`;
  const slowTimer = setTimeout(() => {
    putJob({ id: jobId, title: tf("wait.reinstalling", { source }) });
  }, SLOW_MS);
  try {
    const result = (await MySkills.reinstallRepoSource(donor.gitUrl)) as number;
    const count = typeof result === "number" ? result : 0;
    toast(tf("mySkills.reinstallRepoSuccess", { count }), "success");
    if (saveMark(jobId)) finishSave(jobId);
  } catch (err) {
    const reason = `${tf("mySkills.reinstallRepoFailed", { source })}\n${String(err)}`;
    toast(reason, "error");
    failSave(jobId, reason, () => {
      void reinstallRepoSource(source);
    });
  } finally {
    clearTimeout(slowTimer);
    endJob(jobId);
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
  // 确认后才卸。名单留到 refresh，不在请求前从列表或选择里抠掉。
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

let initialLoaded = false;
let refreshDirty = false;

export function isRefreshDirtyForTest(): boolean {
  return refreshDirty;
}

export function scheduleRefresh(): void {
  if (!untracked(pageActive) || untracked(scope) !== "local") {
    refreshDirty = true;
    return;
  }
  refreshDirty = false;
  void refresh();
}

// 首挂载装载一次；切回本页或切回本地 scope 时若后台有失效信号则补刷一次。
effect(() => {
  if (!pageActive()) return;
  const activeScope = scope();
  if (!initialLoaded) {
    initialLoaded = true;
    refreshDirty = false;
    untracked(() => {
      void refresh();
      void readGithubStatus();
    });
    return;
  }
  if (refreshDirty && activeScope === "local") {
    refreshDirty = false;
    untracked(() => void refresh());
  }
});
effect(() => agentsChanged.subscribe(scheduleRefresh));
effect(() => groupsChanged.subscribe(scheduleRefresh));
effect(() => translationPrefsChanged.subscribe(scheduleRefresh));
effect(() =>
  requestChannelsScope.subscribe(() => {
    setScope("channels");
    if (untracked(github) === null) void readGithubStatus();
  }),
);

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
