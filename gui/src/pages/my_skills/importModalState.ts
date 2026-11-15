import { signal, untracked, effect } from "nsh";
import type { DomainJob } from "skillstar-gui/native";
import { MySkillsImport, SkillCardsCreateGroup } from "skillstar-gui/native";
import { startJob, type JobRun } from "../../services/domainJob";
import { groupsChanged, requestImportSource } from "../../shell/events";
import { refresh } from "./state";
import { t, tf } from "../../services/translation";

// 导入对话框状态机 — my_skills/import_modal.rs（ImportDialog 实体）对位。
// 纯状态与域动作（无 JSX）：渲染器在 importModalPhases/Select/Share/Pack，
// 宿主与卡片在 importModalSlot。
//
// 【桥规矩】扫描/安装走 startJob（120ms 节流的进度事件在 Rust 桥层折叠，
// 这里只接「最新值」映射文案键）；分享码安装/收编/导包无进度流但同持
// DomainJob（关闭即 cancel + epoch 门，过期结算不落屏）。换扫描/换安装
// 前先 stop 旧 run（GPUI 的 facade 单槽对位）。
//
// 【nsh 纪律】组件体只执行一次：signal 的读取全部落在 JSX 表达式或回调
// 里；模块级 effect 是页面实体（state.ts 同款）；回调里不读再写同一信号。

// ---- 域载荷视图类型（facade JsObject 的 TS 侧收窄） --------------------------

/** GitOperationProgress 的原始事件（progress_json：phase/stage 原样）。 */
export interface GitProgressEvent {
  phase: string;
  stage?: string | null;
  skill?: string | null;
  repository?: string;
}

/** DiscoveredSkillInfo（选择列表一行）。 */
export interface SkillRow {
  id: string;
  folderPath: string;
  description: string;
  alreadyInstalled: boolean;
  installable: boolean;
  internal: boolean;
  frontmatterIssues: string[];
}

/** ScanInfo（一次扫描的结果）。 */
export interface ScanView {
  short: string;
  repoUrl: string;
  skills: SkillRow[];
  plugin: boolean;
  cacheHit: boolean;
  cachedAt?: string | null;
  preselectedSkills?: string[];
}

/** SharePreviewInfo（ags-/agd- 解码载荷）。 */
export interface ShareView {
  kind: string;
  name: string;
  description: string;
  icon: string;
  skills: { n: string; u: string; c?: string | null; p?: boolean | null }[];
}

/** ShareOutcomeInfo（分享码安装汇总）。 */
export interface ShareOutcome {
  requested: number;
  existing: string[];
  installed: string[];
  skipped: { name: string; reason: string }[];
  deckCreated: boolean;
}

/** 一条合作方安全审计（select 阶段的提示井）。 */
export interface AuditRow {
  skill: string;
  audits: { name: string; result: string }[];
}

// ---- 域桥（skillReaderSlot 的 ReaderBridge 同款注入位） ----------------------

export interface ImportBridge {
  looksLikeShareCode(text: string): boolean;
  parseShareCode(text: string): Promise<ShareView>;
  installedNames(): Promise<string[]>;
  repoHistory(): Promise<string[]>;
  scanRepo(
    input: string,
    fullDepth: boolean,
    refreshUpstream: boolean,
    job: DomainJob,
    onEvent: (event: GitProgressEvent) => void,
  ): Promise<ScanView>;
  installSkills(
    short: string,
    targets: { id: string; folderPath: string }[],
    job: DomainJob,
    onEvent: (event: GitProgressEvent) => void,
  ): Promise<{ installed: string[]; short: string }>;
  installShare(preview: ShareView, job: DomainJob): Promise<ShareOutcome>;
  adoptFolder(path: string, job: DomainJob): Promise<{ names: string[] }>;
  importBundle(
    path: string,
    fallbackName: string,
    countLabel: string,
    job: DomainJob,
  ): Promise<{ installed: string[]; deckCreated: boolean }>;
  installAudits(repoUrl: string, skillNames: string[]): Promise<AuditRow[]>;
  /** 系统目录选择框（pick_folder；null = 取消，对话框原地不动）。 */
  pickFolder(prompt: string): Promise<string | null>;
  /** 系统 .ags/.agd 文件选择框（pick_bundle；null 同上）。 */
  pickBundleFile(prompt: string): Promise<string | null>;
}

/** Quick Pack 的卡组提交（SkillCardsCreateGroup 域 facade——跨页动作走域，
 * 不 import 卡组页模块）。 */
export interface PackBridge {
  commitDeck(name: string, members: string[]): Promise<void>;
}

/** 真桥：MySkillsImport facade。同步 Result 在 Rust 侧 Err 即 TS 侧 throw
 * （parseShareCode 的失败分支因此裹 try/catch 转 reject）。 */
export const facadeBridge: ImportBridge = {
  looksLikeShareCode: (text) => MySkillsImport.looksLikeShareCode(text),
  parseShareCode: (text) => {
    try {
      return Promise.resolve(MySkillsImport.parseShareCode(text) as unknown as ShareView);
    } catch (err) {
      return Promise.reject(err);
    }
  },
  installedNames: () => MySkillsImport.installedNames() as Promise<string[]>,
  repoHistory: () => MySkillsImport.repoHistory() as Promise<string[]>,
  scanRepo: (input, fullDepth, refreshUpstream, job, onEvent) =>
    MySkillsImport.scanRepo(
      input,
      fullDepth,
      refreshUpstream,
      job,
      onEvent as unknown as (event: unknown) => void,
    ) as Promise<ScanView>,
  installSkills: (short, targets, job, onEvent) =>
    MySkillsImport.installSkills(
      short,
      targets,
      job,
      onEvent as unknown as (event: unknown) => void,
    ) as Promise<{ installed: string[]; short: string }>,
  installShare: (preview, job) =>
    MySkillsImport.installShare(preview, job) as Promise<ShareOutcome>,
  adoptFolder: (path, job) => MySkillsImport.adoptFolder(path, job) as Promise<{ names: string[] }>,
  importBundle: (path, fallbackName, countLabel, job) =>
    MySkillsImport.importBundle(path, fallbackName, countLabel, job) as Promise<{
      installed: string[];
      deckCreated: boolean;
    }>,
  installAudits: (repoUrl, skillNames) =>
    MySkillsImport.installAudits(repoUrl, skillNames) as Promise<AuditRow[]>,
  pickFolder: (prompt) => MySkillsImport.pickFolder(prompt) as Promise<string | null>,
  pickBundleFile: (prompt) => MySkillsImport.pickBundleFile(prompt) as Promise<string | null>,
};

const packFacadeBridge: PackBridge = {
  commitDeck: (name, members) => SkillCardsCreateGroup.commitDeck(null, name, "", "💻", members),
};

/** 当前生效的桥（Host 挂载时安装一次；测试注入假桥）。 */
export let importBridge: ImportBridge = facadeBridge;
export let packBridge: PackBridge = packFacadeBridge;

/** Host 挂载时换桥（无头测试注入假桥用；真桥默认）。 */
export function installBridges(next?: ImportBridge, nextPack?: PackBridge): void {
  importBridge = next ?? facadeBridge;
  packBridge = nextPack ?? packFacadeBridge;
}

// ---- 纯助手（helpers.rs / progress.rs 的对位） -------------------------------

/** progress_key（progress.rs:23-36）：Preparing/Cancelled 先行，Running 里
 * 只有 Discovering 与 Materializing/Deploying 有专属文案，其余 gitRunning。 */
export function progressKey(phase: string, stage?: string | null): string {
  if (phase === "preparing") return "githubImportModal.gitPreparing";
  if (phase === "cancelled") return "githubImportModal.gitCancelled";
  if (phase === "running") {
    if (stage === "discovering") return "githubImportModal.discovering";
    if (stage === "materializing" || stage === "deploying") {
      return "githubImportModal.materializing";
    }
  }
  return "githubImportModal.gitRunning";
}

/** git_error_message（helpers.rs:20-41）：已知传输码换可行动文案，其余
 * 原样透传原始细节。 */
export function gitErrorMessage(raw: string): string {
  const table: [string, string][] = [
    ["token_expired:", "mySkills.gitTokenExpired"],
    ["not_authenticated:", "mySkills.gitNotAuthenticated"],
    ["credential_unavailable:", "mySkills.gitCredentialUnavailable"],
    ["unauthorized:", "mySkills.gitUnauthorized"],
    ["app_not_installed:", "mySkills.gitAppNotInstalled"],
    ["network:", "mySkills.gitNetwork"],
    ["cancelled:", "mySkills.gitCancelled"],
    ["unsafe_remote:", "mySkills.gitUnsafeRemote"],
  ];
  for (const [marker, key] of table) {
    if (raw.includes(marker)) return t(key);
  }
  return raw;
}

/** deck_name_from_source（helpers.rs:7-16）：去尾斜杠与 .git 后的最后一段。 */
export function deckNameFromSource(source: string): string {
  return source
    .trim()
    .replace(/\/+$/, "")
    .replace(/\.git$/, "")
    .split("/")
    .pop()!;
}

/** share_remote（share_install.rs share_remote 的展示半）：仅 https/ssh/
 * scp-like 远端可标；标签 host/owner-repo，解析不了返回 null（不支持源）。 */
export function shareRemoteLabel(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) return null;
  const lower = trimmed.toLowerCase();
  const scpLike = !lower.includes("://") && trimmed.includes("@") && trimmed.includes(":");
  if (!lower.startsWith("https://") && !lower.startsWith("ssh://") && !scpLike) return null;
  let authority: string;
  let rest: string;
  if (scpLike) {
    const at = trimmed.indexOf("@");
    const colon = trimmed.indexOf(":", at + 1);
    authority = trimmed.slice(0, colon);
    rest = trimmed.slice(colon + 1);
  } else {
    const after = trimmed.slice(trimmed.indexOf("://") + 3);
    const cut = after.search(/[/?#]/);
    authority = cut === -1 ? after : after.slice(0, cut);
    rest = cut === -1 ? "" : after.slice(cut);
  }
  const hostNoUser = authority.includes("@") ? authority.slice(authority.lastIndexOf("@") + 1) : authority;
  const host = hostNoUser.includes(":") && /^\d+$/.test(hostNoUser.slice(hostNoUser.lastIndexOf(":") + 1))
    ? hostNoUser.slice(0, hostNoUser.lastIndexOf(":"))
    : hostNoUser;
  if (!host) return null;
  const segments = rest
    .split(/[/?#]/)
    .filter((part) => part.length > 0)
    .map((part) => part.replace(/\.git$/, ""));
  if (segments.length < 2) return null;
  return `${host.toLowerCase()}/${segments[segments.length - 2]}/${segments[segments.length - 1]}`;
}

// ---- 页面实体信号 ------------------------------------------------------------

export type Phase =
  | "input"
  | "scanning"
  | "select"
  | "installing"
  | "completed"
  | "failed"
  | "sharePreview"
  | "shareInstalling"
  | "pack";

export const phase = signal<Phase>("input");
export const urlValue = signal("");
/** 输入框当前文本看起来是分享码（实时 banner 的数据面）。 */
export const shareDetected = signal(false);
export const filterQuery = signal("");
export const history = signal<string[]>([]);
export const scan = signal<ScanView | null>(null);
export const selected = signal<ReadonlySet<string>>(new Set());
export const audits = signal<AuditRow[]>([]);
export const auditPending = signal(false);
export const auditSource = signal<string | null>(null);
export const fullDepth = signal(false);
/** Loading 相的进度文案（progress_key 已解析，GPUI 存 SharedString 同款）。 */
export const progressLabel = signal("");
export const error = signal<string | null>(null);
export const installedCount = signal(0);
export const share = signal<ShareView | null>(null);
export const shareExisting = signal<string[]>([]);
export const summary = signal<ShareOutcome | null>(null);
/** Pack 相的种子（安装完成回调只暂存，渲染时消费——open_pack 对位）。 */
export const packSeed = signal<{ name: string; members: string[] } | null>(null);
export const packName = signal("");
export const packError = signal<string | null>(null);
export const packBusy = signal(false);

/** 在跑的桥任务（扫描/安装/分享/收编/导包共用一个槽——GPUI 的 facade 单槽）。 */
export const activeRun = signal<JobRun<GitProgressEvent> | null>(null);
/** 上一次扫描的输入（deep scan / refresh upstream 复用，scan_input 对位）。 */
let scanInput = "";

/** 进度事件 → Loading 相文案（progress.rs watch 的 still 门对位：只在
 * 扫描/安装相刷新；stop 后的迟到事件被 startJob 的 epoch 门挡在 latest
 * 之外）。单一模块级 effect（dialog.tsx topEntry 同款），不随任务累积。 */
effect(() => {
  const run = activeRun();
  if (run === null) return;
  const event = run.latest();
  if (event === null) return;
  const at = phase();
  if (at !== "scanning" && at !== "installing") return;
  progressLabel.set(t(progressKey(event.phase, event.stage ?? null)));
});

/** 打开信号由 importModalSlot 持有；本模块提供开/关两侧的域动作。 */
export const importModalOpen = signal<string | null>(null);

let pendingImportLaunch: { input: string; autoScan: boolean } | null = null;

function applyPendingImportLaunch(): void {
  const launch = pendingImportLaunch;
  pendingImportLaunch = null;
  if (launch === null) return;
  reset();
  urlValue.set(launch.input);
  if (launch.autoScan) {
    scanWith(launch.input, false, false);
  }
}

/** 对外打开入口（toolbar open_import 对位；接线在 toolbar.tsx）。 */
export function openImportModal(): void {
  pendingImportLaunch = null;
  importModalOpen.set("import");
}

/** 预填来源（如 `npx skills add ...` 或同源推荐技能）打开导入弹窗并可选自动扫描。 */
export function openImportModalWithInput(rawInput: string, autoScan = true): void {
  const trimmed = rawInput.trim();
  if (!trimmed) {
    openImportModal();
    return;
  }
  pendingImportLaunch = { input: trimmed, autoScan };
  if (importModalOpen() !== null) {
    applyPendingImportLaunch();
  } else {
    importModalOpen.set("import");
  }
}

/** 关闭：X/Escape/遮罩共用（ImportDialog::close → cancel_active）。 */
export function closeImportModal(): void {
  pendingImportLaunch = null;
  importModalOpen.set(null);
}

// ---- 生命周期（new/reset/Drop 对位） -----------------------------------------

/** 打开信号驱动：开 = 复位 + 装历史；关 = stop 在途任务（Drop→cancel 兜底，
 * epoch 门保证迟到事件不落屏）。 */
effect(() => {
  const open = importModalOpen();
  if (open === null) {
    untracked(() => {
      activeRun()?.stop();
      activeRun.set(null);
    });
    return;
  }
  untracked(() => {
    reset();
    applyPendingImportLaunch();
    void loadHistory();
  });
});

effect(() =>
  requestImportSource.subscribe((req) => {
    openImportModalWithInput(req.input, req.autoScan);
  }),
);

async function loadHistory(): Promise<void> {
  const sources = await importBridge.repoHistory();
  // 关闭后迟到的历史不落屏（epoch：打开身份仍在才写）。
  if (importModalOpen() === null) return;
  history.set(sources);
}

/** reset（import_modal.rs:639-658）：回输入相，一切清空。受控 set 不触发
 * onChange，与 GPUI set_value 压 Change 同款（无分享码重解析循环）。 */
function reset(): void {
  phase.set("input");
  scan.set(null);
  selected.set(new Set());
  audits.set([]);
  auditPending.set(false);
  auditSource.set(null);
  fullDepth.set(false);
  progressLabel.set("");
  error.set(null);
  installedCount.set(0);
  share.set(null);
  shareExisting.set([]);
  summary.set(null);
  shareDetected.set(false);
  filterQuery.set("");
  packSeed.set(null);
  packName.set("");
  packError.set(null);
  urlValue.set("");
}

// ---- 域动作 -------------------------------------------------------------------

/** 扫描（scan_with_refresh :201-260）：换扫描先停旧任务；进度事件映射
 * progress_key；完成预选「可批量且未安装」并装载审计。 */
export function scanWith(input: string, depth: boolean, refreshUpstream: boolean): void {
  const trimmed = input.trim();
  if (!trimmed) return;
  untracked(() => activeRun()?.stop());
  fullDepth.set(depth);
  scanInput = trimmed;
  audits.set([]);
  auditPending.set(false);
  auditSource.set(null);
  phase.set("scanning");
  progressLabel.set(t("githubImportModal.cloning"));
  error.set(null);

  const run = startJob<GitProgressEvent>("import-scan", (job, onEvent) =>
    importBridge.scanRepo(trimmed, depth, refreshUpstream, job, onEvent),
  );
  activeRun.set(run);

  run.done.then(
    (raw) => {
      if (activeRun() !== run) return; // 过期结算（已取消/已换扫描）不落屏
      activeRun.set(null);
      const next = (raw ?? null) as ScanView | null;
      if (next === null || !Array.isArray(next.skills)) {
        error.set(t("githubImportModal.somethingWrong"));
        phase.set("failed");
        return;
      }
      const explicit = Array.isArray(next.preselectedSkills)
        ? new Set(next.preselectedSkills.map((item) => item.trim().toLowerCase()).filter(Boolean))
        : null;
      const preset = new Set(
        next.skills
          .filter((skill) => {
            if (!skill.installable) return false;
            if (explicit !== null && explicit.size > 0) {
              return (
                explicit.has(skill.id.toLowerCase()) ||
                explicit.has(skill.folderPath.toLowerCase())
              );
            }
            return !skill.internal && !skill.alreadyInstalled;
          })
          .map((skill) => skill.id),
      );
      selected.set(preset);
      scan.set(next);
      phase.set("select");
      startAudits(next);
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      error.set(gitErrorMessage(String(err)));
      phase.set("failed");
    },
  );
}

/** 深度重扫（deep_scan :263-270）。 */
export function deepScan(): void {
  if (scan() === null) return;
  scanWith(scanInput, true, false);
}

/** 上游刷新（refreshUpstream：同输入同深度 refresh=true）。 */
export function refreshUpstream(): void {
  scanWith(scanInput, fullDepth(), true);
}

/** 审计装载（start_install_audits :597-636）：非公开 github 不查；结果按
 * 源守卫落屏（换扫描即弃旧）。 */
function startAudits(next: ScanView): void {
  audits.set([]);
  auditPending.set(false);
  const repoUrl = next.repoUrl;
  const names = next.skills.filter((skill) => skill.installable).map((skill) => skill.id);
  if (names.length === 0) {
    auditSource.set(null);
    return;
  }
  auditSource.set(repoUrl);
  auditPending.set(true);
  void importBridge.installAudits(repoUrl, names).then((rows) => {
    if (auditSource() !== repoUrl) return; // 换了扫描：旧审计不落屏
    auditPending.set(false);
    audits.set(rows);
  });
}

/** URL 输入驱动（on_url_event :158-178）：实时分享码识别 + 解析进预览。 */
export function onUrlChange(text: string): void {
  urlValue.set(text);
  const looks = importBridge.looksLikeShareCode(text);
  shareDetected.set(looks);
  if (looks) parseShare(text);
}

/** Enter 即扫描（PressEnter 分支）。 */
export function onUrlSubmit(): void {
  scanWith(urlValue(), fullDepth(), false);
}

/** 历史行点击：回填并直接扫描（phases.rs 列表行 on_click）。 */
export function pickHistory(source: string): void {
  urlValue.set(source);
  scanWith(source, fullDepth(), false);
}

/** 分享码解析（parse_share :360-400）：失败也进预览相（错误块），成功后
 * 异步标记已安装成员。 */
export function parseShare(text: string): void {
  summary.set(null);
  importBridge
    .parseShareCode(text)
    .then((preview) => {
      share.set(preview);
      shareExisting.set([]);
      error.set(null);
      phase.set("sharePreview");
      const names = preview.skills.map((skill) => skill.n);
      void importBridge.installedNames().then((installed) => {
        if (share() !== preview) return; // 换了载荷：旧差集不落屏
        const lower = new Set(installed.map((name) => name.trim().toLowerCase()));
        const seen = new Set<string>();
        shareExisting.set(
          names
            .map((name) => name.trim())
            .filter(
              (name) =>
                name.length > 0 && !seen.has(name.toLowerCase()) && lower.has(name.toLowerCase()),
            ),
        );
      });
    })
    .catch((err: unknown) => {
      share.set(null);
      shareExisting.set([]);
      error.set(String(err));
      phase.set("sharePreview");
    });
}

/** 行勾选/整行点击的 toggle（select.rs skill_row on_click / checkbox）。 */
export function toggleSkill(id: string): void {
  const next = new Set(selected());
  if (next.has(id)) next.delete(id);
  else next.add(id);
  selected.set(next);
}

/** 全选/取消全选（fresh 优先，全装完时回退可安装集，select.rs:145-160）。 */
export function toggleSelectAll(): void {
  const current = scan();
  if (current === null) return;
  const query = filterQuery().toLowerCase();
  const filtered = current.skills.filter(
    (skill) =>
      query.length === 0 ||
      skill.id.toLowerCase().includes(query) ||
      skill.description.toLowerCase().includes(query),
  );
  const fresh = filtered.filter((skill) => skill.installable && !skill.internal && !skill.alreadyInstalled);
  const selectable = fresh.length > 0 ? fresh : filtered.filter((skill) => skill.installable && !skill.internal);
  if (selectable.length === 0) return;
  const allSelected = selectable.every((skill) => selected().has(skill.id));
  const next = new Set(selected());
  for (const skill of selectable) {
    if (allSelected) next.delete(skill.id);
    else next.add(skill.id);
  }
  selected.set(next);
}

/** 全部重装（reinstall_all：选中集 = 全部可批量 id，select.rs:498-506）。 */
export function selectAllInstallable(): void {
  const current = scan();
  if (current === null) return;
  selected.set(
    new Set(
      current.skills
        .filter((skill) => skill.installable && !skill.internal)
        .map((skill) => skill.id),
    ),
  );
}

/** 安装勾选项（install_selected :274-337）；pack=true 走 Quick Pack 续接。 */
export function installSelected(pack: boolean): void {
  const current = scan();
  if (current === null) return;
  const picked = current.skills.filter(
    (skill) => skill.installable && selected().has(skill.id),
  );
  if (picked.length === 0) return;
  untracked(() => activeRun()?.stop());
  summary.set(null);
  phase.set("installing");
  progressLabel.set(tf("githubImportModal.installing", { count: picked.length }));

  const targets = picked.map((skill) => ({ id: skill.id, folderPath: skill.folderPath }));
  const short = current.short;
  const run = startJob<GitProgressEvent>("import-install", (job, onEvent) =>
    importBridge.installSkills(short, targets, job, onEvent),
  );
  activeRun.set(run);

  run.done.then(
    (raw) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      const outcome = (raw ?? {}) as { installed?: string[] };
      const installed = Array.isArray(outcome.installed) ? outcome.installed : [];
      installedCount.set(installed.length);
      error.set(null);
      void refresh(); // 页面刷新（refresh_page 对位）
      if (pack) {
        packSeed.set({ name: deckNameFromSource(short), members: [...installed] });
        packName.set(deckNameFromSource(short));
        packError.set(null);
        phase.set("pack");
      } else {
        phase.set("completed");
      }
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      error.set(gitErrorMessage(String(err)));
      phase.set("failed");
    },
  );
}

/** 分享码安装（install_share :404-454）。 */
export function installShareNow(): void {
  const preview = share();
  if (preview === null) return;
  untracked(() => activeRun()?.stop());
  phase.set("shareInstalling");
  progressLabel.set(t("shareCodeImport.installing"));

  const run = startJob<GitProgressEvent>("import-share", (job) =>
    importBridge.installShare(preview, job),
  );
  activeRun.set(run);
  run.done.then(
    (raw) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      const outcome = (raw ?? {}) as ShareOutcome;
      error.set(null);
      installedCount.set(outcome.installed.length);
      shareExisting.set([...outcome.existing]);
      summary.set(outcome);
      phase.set("completed");
      void refresh();
      if (outcome.deckCreated) groupsChanged.publish(); // 卡组页重载
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      error.set(String(err));
      phase.set("failed");
    },
  );
}

/** 系统目录选择 → 收编（pick_folder → adopt_folder，import_modal.rs
 * :457-477：取消/未选则对话框原地不动）。 */
export function pickFolderThenAdopt(): void {
  void importBridge.pickFolder(t("importModal.adoptFolderTitle")).then((path) => {
    if (path !== null && path.length > 0) adoptFolder(path);
  });
}

/** 系统文件选择 → 导包（pick_bundle → import_bundle，:518-538 同款）。 */
export function pickBundleThenImport(): void {
  void importBridge.pickBundleFile(t("importBundleModal.pickFile")).then((path) => {
    if (path !== null && path.length > 0) importBundleFile(path);
  });
}

/** 收编本地目录（adopt_folder :479-513）。 */
export function adoptFolder(path: string): void {
  untracked(() => activeRun()?.stop());
  phase.set("installing");
  progressLabel.set(t("importModal.adoptingFolder"));
  const run = startJob<GitProgressEvent>("import-adopt", (job) =>
    importBridge.adoptFolder(path, job),
  );
  activeRun.set(run);
  run.done.then(
    (raw) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      const names = ((raw ?? {}) as { names?: string[] }).names ?? [];
      installedCount.set(names.length);
      phase.set("completed");
      void refresh();
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      error.set(String(err));
      phase.set("failed");
    },
  );
}

/** 导入 .ags/.agd 文件（import_bundle :543-595）。 */
export function importBundleFile(path: string): void {
  untracked(() => activeRun()?.stop());
  phase.set("installing");
  progressLabel.set(t("importBundleModal.importing"));
  const run = startJob<GitProgressEvent>("import-bundle", (job) =>
    importBridge.importBundle(
      path,
      t("importDeckBundleModal.deckBundle"),
      t("importDeckBundleModal.skillsCount"),
      job,
    ),
  );
  activeRun.set(run);
  run.done.then(
    (raw) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      const outcome = (raw ?? {}) as { installed?: string[]; deckCreated?: boolean };
      installedCount.set((outcome.installed ?? []).length);
      phase.set("completed");
      void refresh();
      if (outcome.deckCreated === true) groupsChanged.publish();
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      error.set(String(err));
      phase.set("failed");
    },
  );
}

/** Loading 相的取消按钮（cancel_active :147-151）：stop 即 epoch 门 + Rust
 * 侧 facade.cancel()，任务可能仍在跑但结果不落屏。 */
export function cancelActive(): void {
  untracked(() => {
    activeRun()?.stop();
  });
}

/** 失败相的重试（render_failed on_click → reset）。 */
export function resetToInput(): void {
  reset();
}

/** 分享预览的返回（share.rs back 按钮：回输入相；无载荷分支只清错误）。 */
export function shareBack(clearShare: boolean): void {
  phase.set("input");
  if (clearShare) share.set(null);
  error.set(null);
}

/** Quick Pack 提交（Phase::Pack 的创建按钮）：空名/重名由域层 reject，表单
 * 显示错误并保留输入（CreateGroupDialog 的失败分支对位）。成功 → 关窗 +
 * 卡组页重载（GroupsChanged）。 */
export function commitPack(): void {
  const seed = packSeed();
  if (seed === null || packBusy()) return;
  const name = packName().trim();
  packBusy.set(true);
  packError.set(null);
  packBridge
    .commitDeck(name, seed.members)
    .then(() => {
      packBusy.set(false);
      groupsChanged.publish();
      closeImportModal();
    })
    .catch((err: unknown) => {
      packBusy.set(false);
      packError.set(String(err));
    });
}
