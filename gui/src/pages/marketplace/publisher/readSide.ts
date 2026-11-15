import { signal, type Signal } from "nsh";
import {
  parseSkillsPayload,
  type MarketSkill,
  type PublisherRecord,
  type SnapshotStatusTag,
} from "../types";
import type { MarketNotice, MarketSkillId } from "../readSide";

// 发布者页读侧状态机 — marketplace/publisher/mod.rs 对位，本切片核心。
//
// 本地优先 + epoch 丢弃：每次换发布者（open）、开仓库（openRepo）、返回
// （goUp）、同步完成重载（syncScope 的 reload）都 bump loadEpoch；迟到的
// repos/skills/reread Promise 落地前先对 epoch 与身份（activeRepo），不一致
// 直接丢——验收红线：快速换仓库/换发布者绝不出现旧数据（基座 readSide 同
// 款纪律，GPUI 侧靠 view 生命周期等效）。
//
// 描述回填（mod.rs watch_repo_descriptions + reread_repo_descriptions +
// fill_empty_descriptions）：仓库技能读取发现空描述时轮询域内描述轮的水位
// （repo_description_state，100ms 一次），水位推进或轮结束后重读一次，把新
// 描述只填进仍空白的行——不翻回骨架、不覆盖已有文字、不等整批结束。同一
// 时刻只允许一个重读在飞（desc_read/desc_pending 对位），后来者排队。
//
// 依赖全部注入（reposLoader/skillsLoader/rereader/descriptionState/syncer/
// installer/uninstaller/schedule/notify/confirmUninstall/leaveMarket），页
// 面用真实桥实现，nsh test 用可控 Promise + 手动时钟直接断言状态机
// （marketplace/readSide.ts 同款约定）。文案不进状态机：notify 用结构化标
// 记，页面映射 locale。

/** 仓库行（PublisherRepo 的桥形状 + 宽容解析）。 */
export interface PublisherRepoRow {
  repo: string;
  source: string;
  skillCount: number;
  installsLabel: string;
  installs: number;
}

function parseRepo(row: unknown): PublisherRepoRow {
  const record = (row ?? {}) as Record<string, unknown>;
  return {
    repo: typeof record.repo === "string" ? record.repo : "",
    source: typeof record.source === "string" ? record.source : "",
    skillCount: typeof record.skillCount === "number" ? record.skillCount : 0,
    installsLabel: typeof record.installsLabel === "string" ? record.installsLabel : "",
    installs: typeof record.installs === "number" ? record.installs : 0,
  };
}

function parseReposPayload(payload: unknown): {
  data: PublisherRepoRow[];
  snapshotStatus: SnapshotStatusTag;
  snapshotError: string | null;
} {
  const parsed = (payload ?? {}) as Record<string, unknown>;
  const rows = Array.isArray(parsed.data) ? parsed.data : [];
  const status = parsed.snapshotStatus;
  return {
    data: rows.map(parseRepo),
    snapshotStatus:
      status === "stale" ||
      status === "seeding" ||
      status === "miss" ||
      status === "error_fallback" ||
      status === "remote_error"
        ? status
        : "fresh",
    snapshotError: typeof parsed.error === "string" && parsed.error.length > 0 ? parsed.error : null,
  };
}

export interface PublisherSideDeps {
  /** 仓库清单（MarketplacePublisher.publisherRepos）。 */
  reposLoader(name: string): Promise<unknown>;
  /** 仓库技能网格（MarketplacePublisher.repoSkills）。 */
  skillsLoader(source: string): Promise<unknown>;
  /** 描述回填重读（MarketplacePublisher.rereadRepoSkills）。 */
  rereader(source: string): Promise<unknown>;
  /** 描述轮水位（MarketplacePublisher.repoDescriptionState，同步纯查询）。 */
  descriptionState(source: string): { progress: number; running: boolean };
  /** 域同步（sync_marketplace_scope；scope 文法 publisher_repos:<name> /
   * repo_skills:<source>）。 */
  syncer(scope: string): Promise<void>;
  installer(url: string, name: string): Promise<string>;
  uninstaller(name: string): Promise<string>;
  /** 延时调度，返回取消函数（真实现 setTimeout/clearTimeout）。 */
  schedule(fn: () => void, ms: number): () => void;
  /** 操作结果通知带（页面映射成 NoticeBand toast）。 */
  notify(notice: MarketNotice): void;
  /** 卸载确认（chrome::open_uninstall_confirm）：确认后调 commit。 */
  confirmUninstall(name: string, commit: () => void): void;
  /** go_up 离开发布者（SelectPage(Marketplace)）。 */
  leaveMarket(): void;
}

/** mod.rs 描述轮询间隔（watch_repo_descriptions 的 100ms timer）。 */
export const DESCRIPTION_POLL_MS = 100;

export interface PublisherSide {
  // 状态信号（组件只在 JSX 绑定里读）。
  publisherName: Signal<string | null>;
  repos: Signal<PublisherRepoRow[]>;
  skills: Signal<MarketSkill[]>;
  activeRepo: Signal<string | null>;
  query: Signal<string>;
  loadingRepos: Signal<boolean>;
  loadingSkills: Signal<boolean>;
  refreshing: Signal<boolean>;
  busy: Signal<string | null>;
  snapshotStatus: Signal<SnapshotStatusTag>;
  snapshotError: Signal<string | null>;
  /** 同步成功的状态文案标记（UI 映射 marketplace.snapshotRefreshed）。 */
  refreshedFlash: Signal<boolean>;
  selected: Signal<MarketSkillId | null>;
  // 动作。
  open(publisher: { name: string } | null): void;
  onSearchInput(text: string): void;
  openRepo(source: string): void;
  goUp(): void;
  syncSnapshot(): void;
  openSkill(skill: MarketSkill): void;
  clearSelection(): void;
  toggleInstalled(skill: MarketSkill): void;
}

export function createPublisherSide(deps: PublisherSideDeps): PublisherSide {
  const publisherName = signal<string | null>(null);
  const repos = signal<PublisherRepoRow[]>([]);
  const skills = signal<MarketSkill[]>([]);
  const activeRepo = signal<string | null>(null);
  const query = signal("");
  const loadingRepos = signal(false);
  const loadingSkills = signal(false);
  const refreshing = signal(false);
  const busy = signal<string | null>(null);
  const snapshotStatus = signal<SnapshotStatusTag>("fresh");
  const snapshotError = signal<string | null>(null);
  const refreshedFlash = signal(false);
  const selected = signal<MarketSkillId | null>(null);

  // 每次读取周期都 bump；迟到的响应按它丢弃（基座 readSide 的 loadEpoch
  // 同款；GPUI 侧靠 view 生命周期保证，nsh 侧显式化）。
  let loadEpoch = 0;
  // 描述重读的在飞/排队（mod.rs desc_read / desc_pending）。
  let rereadInFlight = false;
  let rereadPending = false;

  /** 详情列选择只在技能还在列表里时保留（mod.rs detail.retain）。 */
  function retainSelection(): void {
    const current = selected();
    if (current === null) return;
    if (!skills().some((skill) => skill.name === current.name)) {
      selected.set(null);
    }
  }

  /** set_publisher（mod.rs:94-110）：换发布者清一切并重读仓库清单。 */
  function open(publisher: { name: string } | null): void {
    loadEpoch += 1;
    publisherName.set(publisher ? publisher.name : null);
    repos.set([]);
    skills.set([]);
    selected.set(null);
    activeRepo.set(null);
    query.set("");
    snapshotError.set(null);
    refreshedFlash.set(false);
    rereadPending = false;
    if (publisher === null) {
      loadingRepos.set(false);
      loadingSkills.set(false);
      return;
    }
    refreshRepos();
  }

  /** refresh_repos（mod.rs:182-211）：仓库清单读取，落地前对 epoch。 */
  function refreshRepos(): void {
    const name = publisherName();
    if (name === null) return;
    loadEpoch += 1;
    const epoch = loadEpoch;
    loadingRepos.set(true);
    deps
      .reposLoader(name)
      .then((payload) => {
        // 红线：迟到的旧响应直接丢（换发布者/开仓库期间回来的旧清单）。
        if (epoch !== loadEpoch) return;
        loadingRepos.set(false);
        const parsed = parseReposPayload(payload);
        repos.set(parsed.data);
        snapshotStatus.set(parsed.snapshotStatus);
        snapshotError.set(parsed.snapshotError);
      })
      .catch((err: unknown) => {
        if (epoch !== loadEpoch) return;
        loadingRepos.set(false);
        snapshotError.set(String(err));
      });
  }

  /** load_repo_skills_with（mod.rs:213-270）：query 无条件清；详情只在选
   * 中来源与目标仓库不同时清（同仓库重载保选择）。 */
  function openRepo(source: string): void {
    loadEpoch += 1;
    const epoch = loadEpoch;
    // A different repo opens at the top with a clean slate（mod.rs 221-226）。
    if (activeRepo() !== source) {
      selected.set(null);
    }
    activeRepo.set(source);
    query.set("");
    loadingSkills.set(true);
    deps
      .skillsLoader(source)
      .then((payload) => {
        // 红线：迟到响应按 epoch+身份双检（换仓库/返回/换发布者都丢弃）。
        if (epoch !== loadEpoch || activeRepo() !== source) return;
        loadingSkills.set(false);
        const parsed = parseSkillsPayload(payload);
        skills.set(parsed.data);
        retainSelection();
        snapshotStatus.set(parsed.snapshotStatus);
        if (snapshotError() === null) snapshotError.set(parsed.error);
        // The read above started one description round（mod.rs 257-266）：
        // 有空描述且仓库未换时开始盯水位。
        if (
          parsed.data.some((skill) => skill.description.trim().length === 0) &&
          activeRepo() === source
        ) {
          watchDescriptions(source, epoch);
        }
      })
      .catch((err: unknown) => {
        if (epoch !== loadEpoch || activeRepo() !== source) return;
        loadingSkills.set(false);
        snapshotError.set(String(err));
      });
  }

  /** watch_repo_descriptions（mod.rs:275-301）：100ms 盯水位，推进或收工
   * 就重读一次；零起步（轮开始前写下的行也要上屏）。 */
  function watchDescriptions(source: string, watchEpoch: number): void {
    let seen = 0;
    let stopped = false;
    const tick = () => {
      if (stopped) return;
      if (watchEpoch !== loadEpoch || activeRepo() !== source) {
        stopped = true;
        return;
      }
      const state = deps.descriptionState(source);
      if (state.progress !== seen || !state.running) {
        seen = state.progress;
        rereadDescriptions(source, watchEpoch);
      }
      if (!state.running) {
        stopped = true;
        return;
      }
      deps.schedule(tick, DESCRIPTION_POLL_MS);
    };
    tick();
  }

  /** reread_repo_descriptions（mod.rs:305-345）：拉已写下的描述，只填空白
   * 行；一次一个，后来者排队（desc_read/desc_pending）。 */
  function rereadDescriptions(source: string, watchEpoch: number): void {
    if (rereadInFlight) {
      rereadPending = true;
      return;
    }
    rereadInFlight = true;
    deps
      .rereader(source)
      .then((payload) => {
        rereadInFlight = false;
        if (watchEpoch !== loadEpoch || activeRepo() !== source) return;
        const parsed = parseSkillsPayload(payload);
        const merged = fillEmptyDescriptions(skills(), parsed.data);
        if (merged !== null) {
          skills.set(merged);
          retainSelection();
        }
        if (rereadPending) {
          rereadPending = false;
          rereadDescriptions(source, watchEpoch);
        }
      })
      .catch(() => {
        rereadInFlight = false;
      });
  }

  /** sync_current_scope（mod.rs:347-387）：refreshing 守卫不重入；scope 按
   * 当前层选；成功后状态文案 + 重读当前层。 */
  function syncSnapshot(): void {
    const name = publisherName();
    if (name === null) return;
    if (refreshing()) return;
    refreshing.set(true);
    const repo = activeRepo();
    const scope = repo !== null ? `repo_skills:${repo}` : `publisher_repos:${name.toLowerCase()}`;
    deps
      .syncer(scope)
      .then(() => {
        refreshing.set(false);
        refreshedFlash.set(true);
        if (activeRepo() !== null) {
          // 重读当前仓库（openRepo 全量重载，含安装态覆盖）。
          const active = activeRepo();
          if (active !== null) reloadRepo(active);
        } else {
          refreshRepos();
        }
      })
      .catch((err: unknown) => {
        refreshing.set(false);
        snapshotError.set(String(err));
      });
  }

  /** openRepo 的重载形态（同步成功后 load_repo_skills(同 source)）：同仓
   * 库保留详情选择，openRepo 内部自会 bump epoch。 */
  function reloadRepo(source: string): void {
    openRepo(source);
  }

  /** go_up（mod.rs:156-164）：仓库内退回仓库清单，仓库清单退回市场。 */
  function goUp(): void {
    if (activeRepo() !== null) {
      loadEpoch += 1;
      activeRepo.set(null);
      skills.set([]);
      selected.set(null);
      query.set("");
      loadingSkills.set(false);
      return;
    }
    deps.leaveMarket();
  }

  function onSearchInput(text: string): void {
    query.set(text);
  }

  /** set_installed 的提交主体（mod.rs:389-442）：busy 钉住，结果走通知带，
   * 成功后就地翻 installed。 */
  function commitInstall(skill: MarketSkill, install: boolean): void {
    busy.set(skill.name);
    const job = install
      ? deps.installer(skill.gitUrl, skill.name)
      : deps.uninstaller(skill.name);
    job
      .then((name) => {
        busy.set(null);
        deps.notify(install ? { kind: "installed", name } : { kind: "uninstalled", name });
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

  /** mod.rs set_installed 成功分支：列表行 + 详情选择就地翻。 */
  function noteInstalled(name: string, installed: boolean): void {
    skills.set(skills().map((row) => (row.name === name ? { ...row, installed } : row)));
  }

  return {
    publisherName,
    repos,
    skills,
    activeRepo,
    query,
    loadingRepos,
    loadingSkills,
    refreshing,
    busy,
    snapshotStatus,
    snapshotError,
    refreshedFlash,
    selected,

    open,

    onSearchInput,

    openRepo,

    goUp,

    syncSnapshot,

    /** open_market_skill（mod.rs:635-639）：toggle 语义——再点同一张卡收起。 */
    openSkill(skill: MarketSkill): void {
      const current = selected();
      const identity: MarketSkillId = {
        source: skill.source?.trim() || null,
        name: skill.name,
      };
      selected.set(
        current && current.name === identity.name && current.source === identity.source
          ? null
          : identity
      );
    },

    clearSelection(): void {
      selected.set(null);
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
  };
}

/** fill_empty_descriptions（mod.rs:791-820）：把 fresh 里非空的描述按
 * (source, name) 填进 current 里仍空白的行；已有文字不覆盖、没有可填行
 * 时返回 null（调用方不写回）。纯函数，测试直接断言。 */
export function fillEmptyDescriptions(
  current: MarketSkill[],
  fresh: MarketSkill[]
): MarketSkill[] | null {
  const incoming = new Map<string, string>();
  for (const row of fresh) {
    const text = row.description.trim();
    if (text.length === 0) continue;
    const source = row.source?.trim();
    if (!source) continue;
    incoming.set(`${source}\u{0}/${row.name}`, text);
  }
  let changed = false;
  const next = current.map((skill) => {
    if (skill.description.trim().length > 0) return skill;
    const source = skill.source?.trim();
    if (!source) return skill;
    const text = incoming.get(`${source}\u{0}/${skill.name}`);
    if (text === undefined) return skill;
    changed = true;
    return { ...skill, description: text };
  });
  return changed ? next : null;
}

/** hero 计数（hero.rs:16-21）：仓库非空用行内和，空表回退发布者行计数。 */
export function heroCounts(
  publisher: Pick<PublisherRecord, "repoCount" | "skillCount"> | null,
  repos: PublisherRepoRow[]
): { repos: number; skills: number; installs: number } {
  if (repos.length === 0) {
    return {
      repos: publisher?.repoCount ?? 0,
      skills: publisher?.skillCount ?? 0,
      installs: 0,
    };
  }
  return {
    repos: repos.length,
    skills: repos.reduce((sum, repo) => sum + repo.skillCount, 0),
    installs: repos.reduce((sum, repo) => sum + repo.installs, 0),
  };
}
