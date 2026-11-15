// 「存储」状态机与 DTO — 从 storage.tsx 拆出（保持单文件 < 800 行）。

import { signal, type Signal } from "nsh";
import { SettingsStorage } from "skillstar-gui/native";
import { t, tf } from "../../../services/translation";
import { agentsChanged, publishToast } from "../../../shell/events";
import { st, stf } from "../settingsStrings";

// ---- DTO（native facade 的 wire 形状） --------------------------------------

export interface IntakeItemDto {
  agentId: string;
  skill: string;
  kind: string;
}

export interface StorageOverviewDto {
  data_root_path: string;
  hub_root_path: string;
  is_hub_under_data: boolean;
  config_bytes: number;
  config_path: string;
  hub_bytes: number;
  hub_path: string;
  hub_count: number;
  broken_count: number;
  local_count: number;
  local_bytes: number;
  local_path: string;
  cache_bytes: number;
  cache_path: string;
  cache_count: number;
  cache_unused_count: number;
  cache_unused_bytes: number;
  health_issue_count: number;
  health_repairable: number;
  intake: IntakeItemDto[];
  history_count: number;
  backups_count?: number;
  backups_bytes?: number;
}

export type BackupReason = "Uninstall" | "UpdateOverwrite" | "CopyConflict";

export interface BackupEntryDto {
  id: string;
  skillName: string;
  reason: BackupReason;
  createdAt: string;
  source?: string;
  sourceUrl?: string;
  called?: string;
  sizeBytes: number;
  path: string;
}

export interface ClearBackupsReportDto {
  removed_count: number;
  freed_bytes: number;
}

export interface CacheCleanDto {
  repos_removed: number;
  history_cleared: number;
}

export interface ForceDeleteReportDto {
  removed: string[];
  failed: string[];
  kept: string[];
}

export interface RepairPreviewDto {
  issues: number;
  steps: string[];
  untouched: string[];
}

export interface ApplyReportDto {
  applied: number;
  failed: string[];
}

export interface IntakePreviewDto {
  steps: string[];
  reported: string[];
  would_apply: number;
}

export type DeleteTarget = "hub" | "cache";

export interface StorageBridge {
  /** get_storage_overview（async，load_storage 对位）。 */
  storageOverview(): Promise<StorageOverviewDto>;
  /** clear_all_caches：返回 {repos_removed, history_cleared}。 */
  clearAllCaches(): Promise<CacheCleanDto>;
  /** preview_force_delete_installed_skills：将卸载名单（删除的同一选择）。 */
  previewHubDelete(): Promise<string[]>;
  /** force_delete_installed_skills：{removed, failed, kept}。 */
  forceDeleteHub(): Promise<ForceDeleteReportDto>;
  /** force_delete_repo_caches：返回删除数。 */
  forceDeleteCaches(): Promise<number>;
  previewRepair(): Promise<RepairPreviewDto>;
  applyRepair(): Promise<ApplyReportDto>;
  previewIntake(): Promise<IntakePreviewDto>;
  applyIntake(): Promise<ApplyReportDto>;
  /** os_open::open_folder 对位：在文件管理器中显示（fire-and-forget）。 */
  openFolder(path: string): void;
  listBackups?(): Promise<BackupEntryDto[]>;
  restoreBackup?(id: string): Promise<string>;
  clearBackups?(): Promise<ClearBackupsReportDto>;
}

function num(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function strList(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

export function asStorageOverview(value: unknown): StorageOverviewDto {
  const raw = (value ?? {}) as Record<string, unknown>;
  return {
    data_root_path: str(raw.data_root_path),
    hub_root_path: str(raw.hub_root_path),
    is_hub_under_data: raw.is_hub_under_data === true,
    config_bytes: num(raw.config_bytes),
    config_path: str(raw.config_path),
    hub_bytes: num(raw.hub_bytes),
    hub_path: str(raw.hub_path),
    hub_count: num(raw.hub_count),
    broken_count: num(raw.broken_count),
    local_count: num(raw.local_count),
    local_bytes: num(raw.local_bytes),
    local_path: str(raw.local_path),
    cache_bytes: num(raw.cache_bytes),
    cache_path: str(raw.cache_path),
    cache_count: num(raw.cache_count),
    cache_unused_count: num(raw.cache_unused_count),
    cache_unused_bytes: num(raw.cache_unused_bytes),
    health_issue_count: num(raw.health_issue_count),
    health_repairable: num(raw.health_repairable),
    intake: Array.isArray(raw.intake)
      ? raw.intake.map((item) => {
          const row = (item ?? {}) as Record<string, unknown>;
          return {
            agentId: str(row.agentId),
            skill: str(row.skill),
            kind: str(row.kind),
          };
        })
      : [],
    history_count: num(raw.history_count),
    backups_count: num(raw.backups_count),
    backups_bytes: num(raw.backups_bytes),
  };
}

function asBackupReason(value: unknown): BackupReason {
  if (value === "UpdateOverwrite" || value === "CopyConflict") return value;
  return "Uninstall";
}

function asBackupList(value: unknown): BackupEntryDto[] {
  const rows = Array.isArray(value) ? value : [];
  return rows.map((item) => {
    const raw = (item ?? {}) as Record<string, unknown>;
    return {
      id: str(raw.id),
      skillName: str(raw.skillName),
      reason: asBackupReason(raw.reason),
      createdAt: str(raw.createdAt),
      ...(typeof raw.source === "string" ? { source: raw.source } : {}),
      ...(typeof raw.sourceUrl === "string" ? { sourceUrl: raw.sourceUrl } : {}),
      ...(typeof raw.called === "string" ? { called: raw.called } : {}),
      sizeBytes: num(raw.sizeBytes),
      path: str(raw.path),
    };
  });
}

/** 真桥：SettingsStorage facade 直调（页面默认使用）。 */
export const realStorageBridge: StorageBridge = {
  async storageOverview() {
    return asStorageOverview(await SettingsStorage.storageOverview());
  },
  async clearAllCaches() {
    const raw = (await SettingsStorage.clearAllCaches()) as Record<string, unknown>;
    return { repos_removed: num(raw?.repos_removed), history_cleared: num(raw?.history_cleared) };
  },
  async previewHubDelete() {
    return strList(await SettingsStorage.previewHubDelete());
  },
  async forceDeleteHub() {
    const raw = (await SettingsStorage.forceDeleteHub()) as Record<string, unknown>;
    return {
      removed: strList(raw?.removed),
      failed: strList(raw?.failed),
      kept: strList(raw?.kept),
    };
  },
  async forceDeleteCaches() {
    return num(await SettingsStorage.forceDeleteCaches());
  },
  async previewRepair() {
    const raw = (await SettingsStorage.previewRepair()) as Record<string, unknown>;
    return {
      issues: num(raw?.issues),
      steps: strList(raw?.steps),
      untouched: strList(raw?.untouched),
    };
  },
  async applyRepair() {
    const raw = (await SettingsStorage.applyRepair()) as Record<string, unknown>;
    return { applied: num(raw?.applied), failed: strList(raw?.failed) };
  },
  async previewIntake() {
    const raw = (await SettingsStorage.previewIntake()) as Record<string, unknown>;
    return {
      steps: strList(raw?.steps),
      reported: strList(raw?.reported),
      would_apply: num(raw?.would_apply),
    };
  },
  async applyIntake() {
    const raw = (await SettingsStorage.applyIntake()) as Record<string, unknown>;
    return { applied: num(raw?.applied), failed: strList(raw?.failed) };
  },
  openFolder(path) {
    SettingsStorage.revealPath(path);
  },
  async listBackups() {
    return asBackupList(await SettingsStorage.listBackups());
  },
  async restoreBackup(id) {
    const restored = await SettingsStorage.restoreBackup(id);
    return typeof restored === "string" ? restored : "";
  },
  async clearBackups() {
    const raw = (await SettingsStorage.clearBackups()) as Record<string, unknown>;
    return {
      removed_count: num(raw?.removed_count),
      freed_bytes: num(raw?.freed_bytes),
    };
  },
};

// ---- 纯文案组装（mod.rs hub_delete_confirm / hub_delete_status） ------------

export function hubDeleteConfirm(names: string[]): string {
  const listed = names.join(t("settings.skillNameSeparator"));
  return tf("settings.confirmForceDeleteHub", { names: listed });
}

export function hubDeleteStatus(report: ForceDeleteReportDto): string {
  const count = String(report.removed.length);
  if (report.failed.length > 0) {
    return tf("settings.forceDeleteHubPartial", { count, failed: String(report.failed.length) });
  }
  if (report.removed.length === 0 && report.kept.length === 0) {
    return t("settings.forceDeleteHubEmpty");
  }
  if (report.kept.length > 0) {
    return tf("settings.forceDeleteHubKept", { count, kept: String(report.kept.length) });
  }
  return tf("settings.forceDeleteHubDone", { count });
}

export function confirmKeyOf(target: DeleteTarget): string {
  return target === "hub" ? "settings.confirmForceDeleteHub" : "settings.confirmForceDelete";
}

/** parts.rs hub_relative：Hub 相对 Data 的路径（大小写不敏感前缀）。 */
export function hubRelative(parent: string, child: string): string {
  const p = parent.replaceAll("\\", "/").replace(/\/+$/, "");
  const c = child.replaceAll("\\", "/").replace(/\/+$/, "");
  const prefix = `${p}/`;
  if (c.length > prefix.length && c.slice(0, prefix.length).toLowerCase() === prefix.toLowerCase()) {
    return c.slice(prefix.length);
  }
  return ".agents";
}

// ---- 状态机（storage/mod.rs + skill_repair.rs 的方法半边） -------------------

function fmtBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / 1024 ** exp;
  return `${exp === 0 ? val : val.toFixed(1).replace(/\.0$/, "")} ${units[exp]}`;
}

export interface StorageState {
  storage: Signal<StorageOverviewDto | null>;
  storageLoading: Signal<boolean>;
  storageBusy: Signal<"hub" | "cache" | null>;
  storageConfirm: Signal<DeleteTarget | null>;
  cleaning: Signal<boolean>;
  repairing: Signal<boolean>;
  previewing: Signal<boolean>;
  adopting: Signal<boolean>;
  previewingIntake: Signal<boolean>;
  pathStructureOpen: Signal<boolean>;
  storageStatus: Signal<string | null>;
  backups: Signal<BackupEntryDto[]>;
  backupsLoading: Signal<boolean>;
  backupBusyId: Signal<string | null>;
  backupsAvailable: Signal<boolean>;
  start(): void;
  /** skill_repair.rs storage_busy_repair()：四标志之或（与删除/cleaning 门独立）。 */
  busyRepair(): boolean;
  /** folder_button 的打开动作（os_open::open_folder；失败静默，GPUI 同款）。 */
  reveal(path: string): void;
  /** arm_delete：两步强删的第一击（第二击由已 arm 状态直接执行）。 */
  armDelete(target: DeleteTarget): void;
  cleanCaches(): void;
  previewSkillRepair(): void;
  repairSkills(): void;
  previewAgentIntake(): void;
  adoptAgentSkills(): void;
  togglePathStructure(): void;
  reloadBackups(): void;
  restoreBackup(id: string): void;
  clearAllBackups(): void;
}

export function createStorageState(bridge: StorageBridge): StorageState {
  const storage = signal<StorageOverviewDto | null>(null);
  const storageLoading = signal(false);
  const storageBusy = signal<"hub" | "cache" | null>(null);
  const storageConfirm = signal<DeleteTarget | null>(null);
  const cleaning = signal(false);
  const repairing = signal(false);
  const previewing = signal(false);
  const adopting = signal(false);
  const previewingIntake = signal(false);
  const pathStructureOpen = signal(false);
  const storageStatus = signal<string | null>(null);
  const backups = signal<BackupEntryDto[]>([]);
  const backupsLoading = signal(false);
  const backupBusyId = signal<string | null>(null);
  const backupsAvailable = signal(bridge.listBackups !== undefined);
  let backupSeq = 0;

  const busyRepair = (): boolean =>
    repairing() || previewing() || adopting() || previewingIntake();

  const loadBackups = (): void => {
    if (bridge.listBackups === undefined) {
      backupsAvailable.set(false);
      return;
    }
    backupsAvailable.set(true);
    backupsLoading.set(true);
    const req = (backupSeq += 1);
    void bridge
      .listBackups()
      .then((list) => {
        if (req !== backupSeq) return;
        backupsLoading.set(false);
        backups.set(list);
      })
      .catch(() => {
        if (req !== backupSeq) return;
        backupsLoading.set(false);
      });
  };

  const loadStorage = (): void => {
    storageLoading.set(true);
    loadBackups();
    void bridge
      .storageOverview()
      .then((overview) => {
        storageLoading.set(false);
        storage.set(overview);
      })
      .catch((error: unknown) => {
        storageLoading.set(false);
        storageStatus.set(tf("settings.connectionFailed", { error: String(error) }));
      });
  };

  /** storage_action：第二击执行删除，完成后重读 overview。 */
  const storageAction = (target: DeleteTarget): void => {
    storageConfirm.set(null);
    storageBusy.set(target);
    const finish = (): void => {
      storageBusy.set(null);
      loadStorage();
    };
    if (target === "hub") {
      void bridge
        .forceDeleteHub()
        .then((report) => {
          storageStatus.set(hubDeleteStatus(report));
          finish();
        })
        .catch(() => {
          storageStatus.set(t("settings.forceDeleteFailed"));
          finish();
        });
    } else {
      void bridge
        .forceDeleteCaches()
        .then((count) => {
          storageStatus.set(
            count === 0
              ? t("settings.forceDeleteCacheEmpty")
              : tf("settings.forceDeleteCacheDone", { count: String(count) }),
          );
          finish();
        })
        .catch(() => {
          storageStatus.set(t("settings.forceDeleteFailed"));
          finish();
        });
    }
  };

  /** preview_hub_delete：先读将卸载名单，非空才 arm 第二击。 */
  const previewHubDelete = (): void => {
    storageConfirm.set(null);
    storageBusy.set("hub");
    storageStatus.set(null);
    void bridge
      .previewHubDelete()
      .then((names) => {
        storageBusy.set(null);
        if (names.length === 0) {
          storageStatus.set(t("settings.forceDeleteHubEmpty"));
        } else {
          storageConfirm.set("hub");
          storageStatus.set(hubDeleteConfirm(names));
        }
      })
      .catch(() => {
        storageBusy.set(null);
        storageStatus.set(t("settings.forceDeleteFailed"));
      });
  };

  const state: StorageState = {
    storage,
    storageLoading,
    storageBusy,
    storageConfirm,
    cleaning,
    repairing,
    previewing,
    adopting,
    previewingIntake,
    pathStructureOpen,
    storageStatus,
    backups,
    backupsLoading,
    backupBusyId,
    backupsAvailable,
    start() {
      loadStorage();
    },
    busyRepair,
    reveal(path) {
      bridge.openFolder(path);
    },
    armDelete(target) {
      if (storageBusy() !== null || cleaning()) return;
      if (storageConfirm() === target) {
        storageAction(target);
        return;
      }
      if (target === "hub") {
        previewHubDelete();
        return;
      }
      storageConfirm.set(target);
      storageStatus.set(t(confirmKeyOf(target)));
    },
    cleanCaches() {
      if (cleaning()) return;
      cleaning.set(true);
      storageConfirm.set(null);
      void bridge
        .clearAllCaches()
        .then((cleaned) => {
          cleaning.set(false);
          const total = cleaned.repos_removed + cleaned.history_cleared;
          storageStatus.set(
            total === 0
              ? t("settings.cacheEmpty")
              : tf("settings.cacheCleanDone", { count: String(total) }),
          );
          loadStorage();
        })
        .catch(() => {
          cleaning.set(false);
          storageStatus.set(t("settings.cacheCleanFailed"));
          loadStorage();
        });
    },
    previewSkillRepair() {
      if (busyRepair()) return;
      previewing.set(true);
      void bridge
        .previewRepair()
        .then((preview) => {
          previewing.set(false);
          const details = [...preview.steps, ...preview.untouched].join("\n");
          storageStatus.set(
            tf("settings.repairPreviewBody", {
              issues: String(preview.issues),
              steps: String(preview.steps.length),
              untouched: String(preview.untouched.length),
              details,
            }),
          );
          loadStorage();
        })
        .catch((error: unknown) => {
          previewing.set(false);
          storageStatus.set(tf("settings.repairFailed", { error: String(error) }));
          loadStorage();
        });
    },
    repairSkills() {
      if (busyRepair()) return;
      repairing.set(true);
      void bridge
        .applyRepair()
        .then((report) => {
          repairing.set(false);
          storageStatus.set(
            report.failed.length > 0
              ? tf("settings.repairPartial", {
                  count: String(report.applied),
                  details: report.failed.join("\n"),
                })
              : report.applied === 0
                ? t("settings.repairNone")
                : tf("settings.repairDone", { count: String(report.applied) }),
          );
          loadStorage();
        })
        .catch((error: unknown) => {
          repairing.set(false);
          storageStatus.set(tf("settings.repairFailed", { error: String(error) }));
          loadStorage();
        });
    },
    previewAgentIntake() {
      if (busyRepair()) return;
      previewingIntake.set(true);
      void bridge
        .previewIntake()
        .then((preview) => {
          previewingIntake.set(false);
          const details = [...preview.steps, ...preview.reported].join("\n");
          storageStatus.set(
            tf("settings.intakePreviewBody", {
              steps: String(preview.would_apply),
              reported: String(preview.reported.length),
              details,
            }),
          );
          loadStorage();
        })
        .catch((error: unknown) => {
          previewingIntake.set(false);
          storageStatus.set(tf("settings.intakeFailed", { error: String(error) }));
          loadStorage();
        });
    },
    adoptAgentSkills() {
      if (busyRepair()) return;
      adopting.set(true);
      void bridge
        .applyIntake()
        .then((report) => {
          adopting.set(false);
          storageStatus.set(
            report.failed.length > 0
              ? tf("settings.intakePartial", {
                  count: String(report.applied),
                  details: report.failed.join("\n"),
                })
              : report.applied === 0
                ? t("settings.intakeNone")
                : tf("settings.intakeDone", { count: String(report.applied) }),
          );
          loadStorage();
        })
        .catch((error: unknown) => {
          adopting.set(false);
          storageStatus.set(tf("settings.intakeFailed", { error: String(error) }));
          loadStorage();
        });
    },
    togglePathStructure() {
      pathStructureOpen.set(!pathStructureOpen());
    },
    reloadBackups() {
      loadBackups();
    },
    restoreBackup(id) {
      if (backupBusyId() !== null || bridge.restoreBackup === undefined) return;
      backupBusyId.set(id);
      void bridge
        .restoreBackup(id)
        .then((name) => {
          backupBusyId.set(null);
          publishToast({
            tone: "success",
            message: stf("settings.backupsRestored", { name: name || id }),
          });
          agentsChanged.publish();
          loadStorage();
        })
        .catch(() => {
          backupBusyId.set(null);
          publishToast({ tone: "error", message: st("settings.backupsRestoreFailed") });
        });
    },
    clearAllBackups() {
      if (backupBusyId() !== null || bridge.clearBackups === undefined) return;
      backupBusyId.set("__all__");
      void bridge
        .clearBackups()
        .then((report) => {
          backupBusyId.set(null);
          backups.set([]);
          publishToast({
            tone: "success",
            message: stf("settings.backupsCleared", {
              count: report.removed_count,
              size: fmtBytes(report.freed_bytes),
            }),
          });
          loadStorage();
        })
        .catch(() => {
          backupBusyId.set(null);
          publishToast({ tone: "error", message: st("settings.backupsClearFailed") });
        });
    },
  };
  return state;
}
