import { signal, type Signal } from "nsh";
import { SLOW_MS } from "../../chrome/delayedFlag";
import { endJob, putJob } from "../../shell/jobBoard";
import { tf } from "../../services/translation";
import {
  leaderboardCategory,
  parsePublishersPayload,
  parseSkillsPayload,
  parseSnapshotError,
  type MarketSkill,
  type MarketTabId,
  type PublisherRecord,
  type SnapshotErrorInfo,
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

/** 页内一屏缓存。键是 `${tab}|${query.trim()}`，成功落地才写入。 */
interface PageCacheEntry {
  kind: "skills" | "publishers";
  skills: MarketSkill[];
  publishers: PublisherRecord[];
  snapshotStatus: SnapshotStatusTag;
  snapshotUpdatedAt: string | null;
  snapshotError: SnapshotErrorInfo | null;
}

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
  /** 卸载市场行指的那一个已安装技能，返回真正移除的落盘名。安装器遇到
   * 重名会并存改名为 `<name>-<owner>`，落盘目录不一定是 name，桥按
   * source + name 找到它；没有 source 时只按 name。 */
  uninstaller(name: string, source?: string | null): Promise<string>;
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
  /** 可选活跃态判定：离页期间延后 8s 回填重读，回到页面时再 flush。 */
  isActive?(): boolean;
}

/** 打开中的市场技能标识（detail_drawer MarketSkillId）。 */
export interface MarketSkillId {
  source: string | null;
  name: string;
}

/** mod.rs SEARCH_DEBOUNCE。 */
export const SEARCH_DEBOUNCE_MS = 200;
/** mod.rs SYNC_BACKFILL_RELOAD：一次同步还会在后台补描述，8 秒后回填
 * 重读一次，无需用户动作。切 tab 仍 bump epoch，到点的回填自弃。 */
export const SYNC_BACKFILL_RELOAD_MS = 8_000;
/** 每个 scope 在 stale 状态下的最大静默自动重试额度。 */
export const MAX_AUTO_REFRESH_RETRIES = 3;
/** stale 后台重试失败后的退避间隔（ms）。 */
export const STALE_RETRY_DELAY_MS = 5_000;

/** 没有在飞的安装/卸载时的忙碌集。信号按引用比较，空态共用这一个。 */
export const NO_BUSY: ReadonlySet<string> = new Set();

/** 安装超过约 10 秒才挂右上角任务。一个 promise，不能取消；结束时调用返回值。 */
export function armInstallJob(name: string, schedule: ReadSideDeps["schedule"]): () => void {
  const id = `market-install-${name}`;
  let ended = false;
  const cancelTimer = schedule(() => {
    if (ended) return;
    putJob({ id, title: tf("wait.installing", { name }) });
  }, SLOW_MS);
  return () => {
    if (ended) return;
    ended = true;
    cancelTimer();
    endJob(id);
  };
}

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
  /** 在飞的安装/卸载，键是技能名。各技能各记一笔，可以多个同时在飞。 */
  busy: Signal<ReadonlySet<string>>;
  snapshotStatus: Signal<SnapshotStatusTag>;
  snapshotUpdatedAt: Signal<string | null>;
  snapshotError: Signal<SnapshotErrorInfo | null>;
  /** 同步成功的状态文案标记（UI 映射 marketplace.snapshotSynced）。 */
  syncedFlash: Signal<boolean>;
  selected: Signal<MarketSkillId | null>;
  directSource: Signal<DirectSourceMatch | null>;
  // 动作。
  start(): void;
  selectTab(tab: MarketTabId): void;
  onSearchInput(text: string): void;
  clearSearch(): void;
  setSort(sort: MarketSort): void;
  setViewList(list: boolean): void;
  syncSnapshot(): void;
  toggleInstalled(skill: MarketSkill): void;
  openSkill(skill: MarketSkill): void;
  clearSelection(): void;
  /** 详情列切片按名字同步安装态（mod.rs detail.note_installed）。 */
  noteInstalled(name: string, installed: boolean): void;
  /** 回到市场页时补发因离页而延后的 8s 回填重读。 */
  flushBackfill(): void;
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
  const busy = signal<ReadonlySet<string>>(NO_BUSY);
  const snapshotStatus = signal<SnapshotStatusTag>("unknown");
  const snapshotUpdatedAt = signal<string | null>(null);
  const snapshotError = signal<SnapshotErrorInfo | null>(null);
  const syncedFlash = signal(false);
  const selected = signal<MarketSkillId | null>(null);
  const directSource = signal<DirectSourceMatch | null>(null);

  // 每次载入都 bump；迟到的响应按它丢弃（mod.rs load_epoch）。
  // 缓存命中也 bump：测试按 loader 次数核对 epoch，旧响应仍然作废。
  let loadEpoch = 0;
  // 页内缓存，键是捕获的 tab|query，不用实时 tab()。
  const pageCache = new Map<string, PageCacheEntry>();
  // 新击键取消旧防抖计时器（GPUI 侧靠 epoch 检查等效，这里双保险）。
  let cancelDebounce: (() => void) | null = null;
  let started = false;
  let pendingBackfillEpoch: number | null = null;
  const seededQueries = new Set<string>();
  const autoRefreshAttempts = new Map<string, number>();
  const autoRefreshingScopes = new Set<string>();
  const cancelAutoRetryTimers = new Map<string, () => void>();

  function scopeFor(tabAt: MarketTabId, queryAt: string): string {
    const trimmed = queryAt.trim();
    if (trimmed.length > 0) return `search_seed:${trimmed.toLowerCase()}`;
    return tabAt === "official"
      ? "official_publishers"
      : `leaderboard_${leaderboardCategory(tabAt)}`;
  }

  function clearStaleRetryTimer(scope: string): void {
    const cancel = cancelAutoRetryTimers.get(scope);
    if (cancel) {
      cancel();
      cancelAutoRetryTimers.delete(scope);
    }
  }

  function handleSnapshotStatus(scope: string, status: SnapshotStatusTag): void {
    if (status === "fresh") {
      autoRefreshAttempts.delete(scope);
      clearStaleRetryTimer(scope);
      return;
    }
    if (status === "stale") {
      maybeAutoRefreshStale(scope);
    }
  }

  function maybeAutoRefreshStale(scope: string): void {
    if (autoRefreshingScopes.has(scope)) return;
    const attempts = autoRefreshAttempts.get(scope) ?? 0;
    if (attempts >= MAX_AUTO_REFRESH_RETRIES) return;
    clearStaleRetryTimer(scope);
    autoRefreshAttempts.set(scope, attempts + 1);
    autoRefreshingScopes.add(scope);
    deps
      .syncer(scope)
      .then(() => {
        autoRefreshingScopes.delete(scope);
        if (scopeFor(tab(), query()) === scope) {
          loadNow(false);
        }
      })
      .catch(() => {
        autoRefreshingScopes.delete(scope);
        const used = autoRefreshAttempts.get(scope) ?? MAX_AUTO_REFRESH_RETRIES;
        if (used < MAX_AUTO_REFRESH_RETRIES && scopeFor(tab(), query()) === scope) {
          const cancel = deps.schedule(() => {
            cancelAutoRetryTimers.delete(scope);
            if (scopeFor(tab(), query()) === scope && snapshotStatus() === "stale") {
              maybeAutoRefreshStale(scope);
            }
          }, STALE_RETRY_DELAY_MS);
          cancelAutoRetryTimers.set(scope, cancel);
        }
      });
  }

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

  function cacheKey(tabAt: MarketTabId, queryAt: string): string {
    return `${tabAt}|${queryAt.trim()}`;
  }

  /** 命中先填技能/发布者/快照，并关掉骨架档。 */
  function applyCache(entry: PageCacheEntry): void {
    if (entry.kind === "skills") {
      loadedSkills.set(entry.skills.map((row) => ({ ...row })));
      publishers.set([]);
      applySort();
    } else {
      publishers.set(entry.publishers.map((row) => ({ ...row })));
      loadedSkills.set([]);
      skills.set([]);
      selected.set(null);
    }
    snapshotStatus.set(entry.snapshotStatus);
    snapshotUpdatedAt.set(entry.snapshotUpdatedAt);
    snapshotError.set(entry.snapshotError);
    loading.set(false);
  }

  /** 只在 epoch 匹配的成功分支调用。键用捕获的 tabAt/queryAt。 */
  function remember(tabAt: MarketTabId, queryAt: string, entry: PageCacheEntry): void {
    pageCache.set(cacheKey(tabAt, queryAt), entry);
  }

  /** 未命中且调用方要骨架、且两边列表都空，才打开骨架档。 */
  function prime(tabAt: MarketTabId, queryAt: string, skeleton: boolean): void {
    const hit = pageCache.get(cacheKey(tabAt, queryAt));
    if (hit) {
      applyCache(hit);
      return;
    }
    if (skeleton && skills().length === 0 && publishers().length === 0) {
      loading.set(true);
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
        const scope = scopeFor(tabAt, queryAt);
        if (result.kind === "skills") {
          const parsed = parseSkillsPayload(result.payload);
          const errInfo =
            parsed.error && !parsed.error.scope ? { ...parsed.error, scope } : parsed.error;
          loadedSkills.set(parsed.data);
          publishers.set([]);
          applySort();
          snapshotStatus.set(parsed.snapshotStatus);
          snapshotUpdatedAt.set(parsed.snapshotUpdatedAt);
          // LocalFirst 的 error 字段展示在快照横幅（mod.rs 同款）。
          snapshotError.set(errInfo);
          remember(tabAt, queryAt, {
            kind: "skills",
            skills: parsed.data,
            publishers: [],
            snapshotStatus: parsed.snapshotStatus,
            snapshotUpdatedAt: parsed.snapshotUpdatedAt,
            snapshotError: errInfo,
          });
          handleSnapshotStatus(scope, parsed.snapshotStatus);
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
          const errInfo =
            parsed.error && !parsed.error.scope ? { ...parsed.error, scope } : parsed.error;
          publishers.set(parsed.data);
          loadedSkills.set([]);
          skills.set([]);
          selected.set(null);
          snapshotStatus.set(parsed.snapshotStatus);
          snapshotUpdatedAt.set(parsed.snapshotUpdatedAt);
          snapshotError.set(errInfo);
          remember(tabAt, queryAt, {
            kind: "publishers",
            skills: [],
            publishers: parsed.data,
            snapshotStatus: parsed.snapshotStatus,
            snapshotUpdatedAt: parsed.snapshotUpdatedAt,
            snapshotError: errInfo,
          });
          handleSnapshotStatus(scope, parsed.snapshotStatus);
        }
      })
      .catch((err: unknown) => {
        if (epoch !== loadEpoch) return;
        loading.set(false);
        snapshotError.set(
          parseSnapshotError(String(err), "remote_error", scopeFor(tabAt, queryAt))
        );
      });
  }

  /** load_now（mod.rs:218-234）：用户导航。有缓存先填再请求；没缓存且
   * 两边都空才开骨架。命中同样 bump，迟到响应仍作废。 */
  function loadNow(skeleton: boolean): void {
    const tabAt = tab();
    const queryAt = query().trim();
    prime(tabAt, queryAt, skeleton);
    loadEpoch += 1;
    fetch(tabAt, queryAt, loadEpoch);
  }

  /** load_debounced（mod.rs:238-257）：击键 bump epoch，暂停 200ms 后仍
   * 是最新 epoch 才发读取。搜索只有两边列表都空才允许骨架。 */
  function loadDebounced(): void {
    loadEpoch += 1;
    const epoch = loadEpoch;
    const tabAt = tab();
    const queryAt = query().trim();
    if (cancelDebounce) cancelDebounce();
    cancelDebounce = deps.schedule(() => {
      cancelDebounce = null;
      if (epoch !== loadEpoch) return;
      prime(tabAt, queryAt, true);
      fetch(tabAt, queryAt, epoch);
    }, SEARCH_DEBOUNCE_MS);
  }

  /** 忙碌按技能名各记一笔：后点的技能不打断先点的，一个结束只放开它
   * 自己。信号按引用比较，每次整组替换。 */
  function setBusy(name: string, on: boolean): void {
    const next = new Set(busy());
    if (on) next.add(name);
    else next.delete(name);
    busy.set(next.size === 0 ? NO_BUSY : next);
  }

  /** set_installed 的提交主体（mod.rs:368-430）：busy 钉住该技能自己的
   * 卡面动作，结果走通知带，成功后就地翻 installed 标志。 */
  function commitInstall(skill: MarketSkill, install: boolean): void {
    // 在飞的技能不重复提交。卡片和详情列靠同一份记录钉住按钮，这里兜底。
    if (busy().has(skill.name)) return;
    setBusy(skill.name, true);
    const finishJob = install ? armInstallJob(skill.name, deps.schedule) : () => {};
    const job = install
      ? deps.installer(skill.gitUrl, skill.name)
      : deps.uninstaller(skill.name, skill.source);
    const finish = (): void => {
      finishJob();
      setBusy(skill.name, false);
    };
    job
      .then((name) => {
        finish();
        deps.notify(
          install ? { kind: "installed", name } : { kind: "uninstalled", name }
        );
        // 翻的是用户操作的那一行：并存改名时桥返回的落盘名不是行名，
        // 只按它匹配会让这张卡停在旧状态。落盘名对应的行也一并翻。
        noteInstalled(skill.name, install);
        if (name !== skill.name) noteInstalled(name, install);
      })
      .catch((err: unknown) => {
        finish();
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

    /** 切 tab（view.rs render_tabs 的点击）：清 query、清错误。进 official
     * 清技能行与详情；离开则清发布者。有缓存先填再请求；两边都空才开骨架。 */
    selectTab(next: MarketTabId): void {
      if (tab() === next) return;
      tab.set(next);
      query.set("");
      directSource.set(null);
      snapshotError.set(null);
      if (next === "official") {
        selected.set(null);
        loadedSkills.set([]);
        skills.set([]);
      } else {
        publishers.set([]);
      }
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
      // 先落下再升起，第二次成功才会再次通知。
      syncedFlash.set(false);
      const scope = scopeFor(tab(), query());
      autoRefreshAttempts.delete(scope);
      clearStaleRetryTimer(scope);
      deps
        .syncer(scope)
        .then(() => {
          refreshing.set(false);
          syncedFlash.set(true);
          loadNow(false);
          const epoch = loadEpoch;
          deps.schedule(() => {
            if (epoch !== loadEpoch) return;
            if (deps.isActive && !deps.isActive()) {
              pendingBackfillEpoch = epoch;
              return;
            }
            pendingBackfillEpoch = null;
            loadNow(false);
          }, SYNC_BACKFILL_RELOAD_MS);
        })
        .catch((err: unknown) => {
          refreshing.set(false);
          snapshotError.set(parseSnapshotError(String(err), "remote_error", scope));
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

    flushBackfill(): void {
      if (pendingBackfillEpoch === null) return;
      const epoch = pendingBackfillEpoch;
      pendingBackfillEpoch = null;
      if (epoch === loadEpoch) {
        loadNow(false);
      }
    },
  };
}
