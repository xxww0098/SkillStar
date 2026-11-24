// 「关于」状态机、桥接与 DTO — 从 ./about.tsx 拆出（保持单文件 < 800 行）。

import { Clipboard, batch, signal, type Signal } from "nsh";
import { SettingsStorage } from "skillstar-gui/native";

// ---- DTO（about_snapshot / run_release_check 的 wire 形状） ------------------

/** gh_manager::GitStatus（serde tag="status"，变体名原样大小写）。 */
export type GitStatusDto =
  | { status: "Installed"; version: string }
  | {
      status: "NotInstalled";
      os: string;
      install_instructions: { label: string; command: string }[];
      download_url: string;
    };

/** release_check::ReleaseCheckOutcome（serde tag="kind"，snake_case）。 */
export type ReleaseOutcomeDto =
  | { kind: "up_to_date" }
  | { kind: "no_published_release" }
  | { kind: "available"; tag: string; url: string }
  | { kind: "failed"; reason: string };

export interface ReleaseRecordDto {
  last_checked_unix: number;
  current_version: string;
  outcome: ReleaseOutcomeDto;
}

export interface AboutSnapshotDto {
  version: string;
  data_root: string;
  gh_installed: boolean;
  /** about.rs gh_install_command：本平台的 gh 安装命令（null=未知平台）。 */
  gh_install_command: string | null;
  git_status: GitStatusDto | null;
  release_check: ReleaseRecordDto | null;
}

export interface AboutBridge {
  /** about.rs 构造函数对位：环境快照一次读。 */
  aboutSnapshot(): Promise<AboutSnapshotDto>;
  /** run_release_check：真网；resolve 新记录（域侧已持久化）。 */
  runReleaseCheck(): Promise<ReleaseRecordDto>;
  /** os_open::open_folder 对位：文件管理器中显示（失败静默）。 */
  openFolder(path: string): void;
}

export const RELEASES_PAGE_URL = "https://github.com/xxww0098/SkillStar/releases/latest";
export const GH_INSTALL_PAGE_URL = "https://cli.github.com/";

function asSnapshot(value: unknown): AboutSnapshotDto {
  const raw = (value ?? {}) as Record<string, unknown>;
  const gitRaw = (raw.git_status ?? null) as Record<string, unknown> | null;
  const releaseRaw = (raw.release_check ?? null) as Record<string, unknown> | null;
  const outcomeRaw = (releaseRaw?.outcome ?? null) as Record<string, unknown> | null;
  const str = (item: unknown): string => (typeof item === "string" ? item : "");
  const gitStatus: GitStatusDto | null =
    gitRaw !== null && gitRaw.status === "Installed"
      ? { status: "Installed", version: str(gitRaw.version) }
      : gitRaw !== null && gitRaw.status === "NotInstalled"
        ? {
            status: "NotInstalled",
            os: str(gitRaw.os),
            download_url: str(gitRaw.download_url),
            install_instructions: Array.isArray(gitRaw.install_instructions)
              ? gitRaw.install_instructions.map((item) => {
                  const row = (item ?? {}) as Record<string, unknown>;
                  return { label: str(row.label), command: str(row.command) };
                })
              : [],
          }
        : null;
  const outcome: ReleaseOutcomeDto | null =
    outcomeRaw === null
      ? null
      : outcomeRaw.kind === "available"
        ? { kind: "available", tag: str(outcomeRaw.tag), url: str(outcomeRaw.url) }
        : outcomeRaw.kind === "failed"
          ? { kind: "failed", reason: str(outcomeRaw.reason) }
          : outcomeRaw.kind === "up_to_date"
            ? { kind: "up_to_date" }
            : outcomeRaw.kind === "no_published_release"
              ? { kind: "no_published_release" }
              : null;
  return {
    version: str(raw.version),
    data_root: str(raw.data_root),
    gh_installed: raw.gh_installed === true,
    gh_install_command: typeof raw.gh_install_command === "string" ? raw.gh_install_command : null,
    git_status: gitStatus,
    release_check:
      releaseRaw !== null && outcome !== null
        ? {
            last_checked_unix:
              typeof releaseRaw.last_checked_unix === "number" ? releaseRaw.last_checked_unix : 0,
            current_version: str(releaseRaw.current_version),
            outcome,
          }
        : null,
  };
}

/** 真桥：SettingsStorage facade 直调（页面默认使用）。 */
export const realAboutBridge: AboutBridge = {
  async aboutSnapshot() {
    return asSnapshot(await SettingsStorage.aboutSnapshot());
  },
  async runReleaseCheck() {
    return asSnapshot({ release_check: await SettingsStorage.runReleaseCheck() }).release_check!;
  },
  openFolder(path) {
    // fire-and-forget（os_open::open_folder 的 `let _ =` spawn 对位）。
    try {
      SettingsStorage.revealPath(path);
    } catch {
      // 打开失败静默。
    }
  },
};

/** about.rs display_home：$HOME 前缀折算成 ~（home 由调用方注入，可测）。 */
export function displayHome(path: string, home: string): string {
  if (home !== "" && path.startsWith(home)) {
    return `~${path.slice(home.length)}`;
  }
  return path;
}

/** about.rs gh_install_command 的平台键（cfg!(target_os) 的运行时对位）。 */
export function platformKey(platform: string): "macos" | "windows" | "linux" | "unknown" {
  if (platform === "macos" || platform === "windows" || platform === "linux") return platform;
  return "unknown";
}

/** data_path_row 的平台标签（macOS/Windows/Linux，其余落 Linux 同 GPUI else 臂）。 */
export function platformLabel(platform: string): string {
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  return "Linux";
}

// ---- 状态机 ------------------------------------------------------------------

export interface AboutState {
  snapshot: Signal<AboutSnapshotDto | null>;
  /** 上一轮（或刚完成的）检查记录；null = 从未检查。 */
  releaseCheck: Signal<ReleaseRecordDto | null>;
  releaseChecking: Signal<boolean>;
  /** 复制钮的 check 态（command_row 的 copied；再点别处换 id，无超时）。 */
  copied: Signal<string | null>;
  start(): Promise<void>;
  runReleaseCheck(): void;
  copyCommand(id: string, text: string): void;
  openDataRoot(): void;
}

export function createAboutState(bridge: AboutBridge): AboutState {
  const snapshot = signal<AboutSnapshotDto | null>(null);
  const releaseCheck = signal<ReleaseRecordDto | null>(null);
  const releaseChecking = signal(false);
  const copied = signal<string | null>(null);
  let startSeq = 0;

  return {
    snapshot,
    releaseCheck,
    releaseChecking,
    copied,
    async start() {
      // 读盘先进局部变量再落 signal：effect 追踪面必须为空。
      const seq = ++startSeq;
      const loaded = await bridge.aboutSnapshot();
      if (seq !== startSeq) return;
      batch(() => {
        snapshot.set(loaded);
        releaseCheck.set(loaded.release_check);
      });
    },
    runReleaseCheck() {
      if (releaseChecking()) return;
      releaseChecking.set(true);
      void bridge
        .runReleaseCheck()
        .then((record) => {
          batch(() => {
            releaseChecking.set(false);
            releaseCheck.set(record);
          });
        })
        .catch(() => {
          // run_check 的失败也是 failed outcome 记录（域侧）；桥层异常仅
          // 复位按钮，保留上一轮记录（GPUI 同款）。
          releaseChecking.set(false);
        });
    },
    copyCommand(id, text) {
      void Clipboard.writeText(text).then(() => copied.set(id));
    },
    openDataRoot() {
      const path = snapshot()?.data_root;
      if (path !== undefined) bridge.openFolder(path);
    },
  };
}
