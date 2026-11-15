import { signal, type Signal } from "nsh";
import {
  leaderboardCategory,
  parsePublishersPayload,
  parseSkillsPayload,
  type MarketSkill,
  type MarketTabId,
  type PublisherRecord,
  type SnapshotStatusTag,
} from "./types";

// 市场页读侧状态机 — marketplace/mod.rs 对位，本波核心。
//
// 本地优先：所有读取走快照（leaderboard/search/publishers 四个读者），
// epoch 丢弃过期响应——每次导航（loadNow）或击键（loadDebounced）都
// bump loadEpoch；迟到的 Promise 落地前先对 epoch，不一致直接丢。这是
// 验收红线：连点切 tab、快速输入绝不出现旧数据。
//
// 依赖全部注入（loader/syncer/installer/uninstaller/schedule/notify/
// confirmUninstall），页面用真实桥实现，nsh test 用可手动推进的假时钟
// 与可控 Promise 直接断言状态机（noticeBoard.ts 同款约定：调度类纯逻辑
// 抽模块单测）。文案不进状态机：notify/状态字段用结构化标记，页面再映
// 射成 locale。
//
// 每个实例自持 signal（createReadSide 工厂），页面模块级单例即页面实
// 体（keep-alive 只挂载一次）；测试各建各的，互不串扰。

/** 读取载荷：official 是发布者，其余是技能（mod.rs MarketplaceData）。 */
export type LoadPayload =
  | { kind: "skills"; payload: unknown }
  | { kind: "publishers"; payload: unknown };

/** 工具栏排序（mod.rs MarketSort）。 */
export type MarketSort = "stars" | "updated";

/** 状态机发给 UI 层的结构化通知（页面映射成 NoticeBand toast）。 */
export type MarketNotice =
  | { kind: "installed"; name: string }
  | { kind: "uninstalled"; name: string }
  | { kind: "installFailed"; name: string; error: string }
  | { kind: "uninstallFailed"; name: string; error: string };

export interface DirectSourceMatch {
  source: string;
  raw: string;
  skills: string[];
  wasCommand: boolean;
}

export interface ReadSideDeps {
  /** 一次读：非空 query 恒为 search；否则 official 为发布者，其余按
   * tab 类目读 leaderboard（mod.rs refresh 的路由）。 */
  loader(tab: MarketTabId, query: string): Promise<LoadPayload>;
  /** 域同步（sync_marketplace_scope）。 */
  syncer(scope: string): Promise<void>;
  installer(url: string, name: string): Promise<string>;
  uninstaller(name: string): Promise<string>;
  /** 延时调度，返回取消函数（真实现 setTimeout/clearTimeout）。 */
  schedule(fn: () => void, ms: number): () => void;
  /** 操作结果通知带（成功/失败 toast）。 */
  notify(notice: MarketNotice): void;
  /** 卸载确认（chrome::open_uninstall_confirm）：确认后调 commit。安装
   * 不弹（install_toggle.rs）。 */
  confirmUninstall(name: string, commit: () => void): void;
  /** 从搜索输入识别直接仓库/安装命令（可选，便于旧桩兼容）。 */
  parseDirectSource?(query: string): DirectSourceMatch | null;
  /** <5 条稀疏搜索且 >10m 冷却时触发种子检索；返回 true 时原位重拉。 */
  maybeSeedSearch?(query: string): Promise<boolean>;
}

/** 打开中的市场技能标识（detail_drawer MarketSkillId）。 */
export interface MarketSkillId {
  source: string | null;
  name: string;
}

/** mod.rs SEARCH_DEBOUNCE。 */
export const SEARCH_DEBOUNCE_MS = 200;
/** mod.rs SYNC_BACKFILL_RELOAD：一次同步还会在后台补描述，8 秒后回填
 * 重读一次，无需用户动作。 */
export const SYNC_BACKFILL_RELOAD_MS = 8_000;

export interface ReadSide {
  // 状态信号（组件只在 JSX 绑定里读）。
  tab: Signal<MarketTabId>;
  query: Signal<string>;
  skills: Signal<MarketSkill[]>;
  publishers: Signal<PublisherRecord[]>;
  sort: Signal<MarketSort>;
  viewList: Signal<boolean>;
  loading: Signal<boolean>;
  refreshing: Signal<boolean>;
  busy: Signal<string | null>;
  snapshotStatus: Signal<SnapshotStatusTag>;
  snapshotUpdatedAt: Signal<string | null>;
  snapshotError: Signal<string | null>;
  /** 同步成功的状态文案标记（UI 映射 marketplace.snapshotSynced）。 */
  syncedFlash: Signal<boolean>;
  selected: Signal<MarketSkillId | null>;
  directSource: Signal<DirectSourceMatch | null>;
  // 动作。
  start(): void;
  selectTab(tab: MarketTabId): void;
  onSearchInput(text: string): void;
  clearSearch(): void;
  spotlightPick(name: string): void;
  setSort(sort: MarketSort): void;
  setViewList(list: boolean): void;
  syncSnapshot(): void;
  toggleInstalled(skill: MarketSkill): void;
  openSkill(skill: MarketSkill): void;
  clearSelection(): void;
  /** 详情列切片按名字同步安装态（mod.rs detail.note_installed）。 */
  noteInstalled(name: string, installed: boolean): void;
}

export function createReadSide(deps: ReadSideDeps): ReadSide {
  const tab = signal<MarketTabId>("all");
  const query = signal("");
  const loadedSkills = signal<MarketSkill[]>([]);
  const skills = signal<MarketSkill[]>([]);
  const publishers = signal<PublisherRecord[]>([]);
  const sort = signal<MarketSort>("stars");
  const viewList = signal(false);
  const loading = signal(true);
  const refreshing = signal(false);
  const busy = signal<string | null>(null);
  const snapshotStatus = signal<SnapshotStatusTag>("fresh");
  const snapshotUpdatedAt = signal<string | null>(null);
  const snapshotError = signal<string | null>(null);
  const syncedFlash = signal(false);
  const selected = signal<MarketSkillId | null>(null);
  const directSource = signal<DirectSourceMatch | null>(null);

  // 每次载入都 bump；迟到的响应按它丢弃（mod.rs load_epoch）。
  let loadEpoch = 0;
  // 新击键取消旧防抖计时器（GPUI 侧靠 epoch 检查等效，这里双保险）。
  let cancelDebounce: (() => void) | null = null;
  let started = false;
  const seededQueries = new Set<string>();

  /** apply_sort（mod.rs:432-459）：Updated 按更新时间倒序；Stars 搜索中
   * 按星标重排并重编名次，非搜索保持榜序并回填名次。 */
  function applySort(): void {
    const searching = query().trim().length > 0;
    let rows = loadedSkills().slice();
    if (sort() === "updated") {
      rows.sort(
        (a, b) => b.lastUpdated.localeCompare(a.lastUpdated) || a.name.localeCompare(b.name)
      );
    } else if (searching) {
      rows.sort((a, b) => b.stars - a.stars || a.name.localeCompare(b.name));
    }
    if (sort() === "stars") {
      rows = rows.map((row, index) => ({
        ...row,
        rank: searching ? index + 1 : (row.rank ?? index + 1),
      }));
    }
    skills.set(rows);
    retainSelection();
  }

  /** 详情列选择只在技能还在列表里时保留（mod.rs detail.retain）。 */
  function retainSelection(): void {
    const current = selected();
    if (current === null) return;
    if (!skills().some((skill) => skill.name === current.name)) {
      selected.set(null);
    }
  }

  /** refresh（mod.rs:259-314）：按 tab+query 路由读取，落地前对 epoch。 */
  function fetch(tabAt: MarketTabId, queryAt: string, epoch: number): void {
    deps
      .loader(tabAt, queryAt)
      .then((result) => {
        // 红线：迟到的旧响应直接丢。
        if (epoch !== loadEpoch) return;
        loading.set(false);
        if (result.kind === "skills") {
          const parsed = parseSkillsPayload(result.payload);
          loadedSkills.set(parsed.data);
          publishers.set([]);
          applySort();
          snapshotStatus.set(parsed.snapshotStatus);
          snapshotUpdatedAt.set(parsed.snapshotUpdatedAt);
          // LocalFirst 的 error 字段展示在快照横幅（mod.rs 同款）。
          snapshotError.set(parsed.error);
          const trimmed = queryAt.trim();
          if (
            trimmed.length > 0 &&
            parsed.data.length < 5 &&
            directSource() === null &&
            deps.maybeSeedSearch !== undefined &&
            !seededQueries.has(trimmed)
          ) {
            seededQueries.add(trimmed);
            void deps
              .maybeSeedSearch(trimmed)
              .then((didSeed) => {
                if (didSeed && query().trim() === trimmed) {
                  loadNow(false);
                }
              })
              .catch(() => {
                /* best-effort sparse search fallback */
              });
          }
        } else {
          const parsed = parsePublishersPayload(result.payload);
          publishers.set(parsed.data);
          loadedSkills.set([]);
          skills.set([]);
          selected.set(null);
          snapshotStatus.set(parsed.snapshotStatus);
          snapshotUpdatedAt.set(parsed.snapshotUpdatedAt);
          snapshotError.set(parsed.error);
        }
      })
      .catch((err: unknown) => {
        if (epoch !== loadEpoch) return;
        loading.set(false);
        snapshotError.set(String(err));
      });
  }

  /** load_now（mod.rs:218-234）：用户导航。skeleton 档（切 tab、清空搜
   * 索）打开骨架屏并回顶；原位档（同步完成、spotlight 选中）保位。 */
  function loadNow(skeleton: boolean): void {
    loadEpoch += 1;
    if (skeleton) {
      loading.set(true);
    }
    fetch(tab(), query().trim(), loadEpoch);
  }

  /** load_debounced（mod.rs:238-257）：击键 bump epoch，暂停 200ms 后仍
   * 是最新 epoch 才发读取；两列表皆空时先亮骨架。 */
  function loadDebounced(): void {
    loadEpoch += 1;
    const epoch = loadEpoch;
    if (cancelDebounce) cancelDebounce();
    cancelDebounce = deps.schedule(() => {
      cancelDebounce = null;
      if (epoch !== loadEpoch) return;
      if (skills().length === 0 && publishers().length === 0) {
        loading.set(true);
      }
      fetch(tab(), query().trim(), epoch);
    }, SEARCH_DEBOUNCE_MS);
  }

  /** set_installed 的提交主体（mod.rs:368-430）：busy 钉住卡面动作，
   * 结果走通知带，成功后就地翻 installed 标志。 */
  function commitInstall(skill: MarketSkill, install: boolean): void {
    busy.set(skill.name);
    const job = install
      ? deps.installer(skill.gitUrl, skill.name)
      : deps.uninstaller(skill.name);
    job
      .then((name) => {
        busy.set(null);
        deps.notify(
          install ? { kind: "installed", name } : { kind: "uninstalled", name }
        );
        noteInstalled(name, install);
      })
      .catch((err: unknown) => {
        busy.set(null);
        deps.notify(
          install
            ? { kind: "installFailed", name: skill.name, error: String(err) }
            : { kind: "uninstallFailed", name: skill.name, error: String(err) }
        );
      });
  }

  function noteInstalled(name: string, installed: boolean): void {
    loadedSkills.set(
      loadedSkills().map((row) => (row.name === name ? { ...row, installed } : row))
    );
    skills.set(skills().map((row) => (row.name === name ? { ...row, installed } : row)));
  }

  return {
    tab,
    query,
    skills,
    publishers,
    sort,
    viewList,
    loading,
    refreshing,
    busy,
    snapshotStatus,
    snapshotUpdatedAt,
    snapshotError,
    syncedFlash,
    selected,
    directSource,

    start(): void {
      if (started) return;
      started = true;
      loadNow(true);
    },

    /** 切 tab（view.rs render_tabs 的点击）：清 query、清错误、official
     * 清详情选择，骨架档重载。 */
    selectTab(next: MarketTabId): void {
      if (tab() === next) return;
      tab.set(next);
      query.set("");
      directSource.set(null);
      snapshotError.set(null);
      if (next === "official") selected.set(null);
      loadNow(true);
    },

    onSearchInput(text: string): void {
      if (text === query()) return;
      query.set(text);
      directSource.set(deps.parseDirectSource ? deps.parseDirectSource(text) : null);
      loadDebounced();
    },

    clearSearch(): void {
      query.set("");
      directSource.set(null);
      loadNow(true);
    },

    /** spotlight 选中一行：query 设为该名并立即原位重载
     * （spotlight.rs on_click，load_now(false)）。 */
    spotlightPick(name: string): void {
      query.set(name);
      directSource.set(deps.parseDirectSource ? deps.parseDirectSource(name) : null);
      loadNow(false);
    },

    setSort(next: MarketSort): void {
      if (sort() === next) return;
      sort.set(next);
      applySort();
    },

    setViewList(list: boolean): void {
      viewList.set(list);
    },

    /** sync_snapshot（mod.rs:316-366）：域内刷新中不重入；成功后状态文
     * 案 + 原位重载，并排 8s 回填重读（epoch 变了就不读）。 */
    syncSnapshot(): void {
      if (refreshing()) return;
      refreshing.set(true);
      const scopeQuery = query().trim();
      const scopeTab = tab();
      const scope = scopeQuery
        ? `search_seed:${scopeQuery.toLowerCase()}`
        : scopeTab === "official"
          ? "official_publishers"
          : `leaderboard_${leaderboardCategory(scopeTab)}`;
      deps
        .syncer(scope)
        .then(() => {
          refreshing.set(false);
          syncedFlash.set(true);
          loadNow(false);
          const epoch = loadEpoch;
          deps.schedule(() => {
            if (epoch !== loadEpoch) return;
            loadNow(false);
          }, SYNC_BACKFILL_RELOAD_MS);
        })
        .catch((err: unknown) => {
          refreshing.set(false);
          snapshotError.set(String(err));
        });
    },

    /** install_toggle.rs：安装立即提交；卸载先走确认弹窗。 */
    toggleInstalled(skill: MarketSkill): void {
      const install = !skill.installed;
      if (!install) {
        deps.confirmUninstall(skill.name, () => commitInstall(skill, false));
        return;
      }
      commitInstall(skill, true);
    },

    openSkill(skill: MarketSkill): void {
      const current = selected();
      const identity: MarketSkillId = {
        source: skill.source?.trim() || null,
        name: skill.name,
      };
      // 再点同一张卡收起（mod.rs open_market_skill 的 toggle 语义）。
      selected.set(
        current && current.name === identity.name && current.source === identity.source
          ? null
          : identity
      );
    },

    clearSelection(): void {
      selected.set(null);
    },

    noteInstalled,
  };
}
