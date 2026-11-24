import { batch, signal, type Signal } from "nsh";
import { SettingsAgentConnections, SettingsStorage } from "skillstar-gui/native";
import { t, tf } from "../../../services/translation";
import { agentsChanged, publishToast, type ToastTone } from "../../../shell/events";
import { st, stf } from "../settingsStrings";

// 「智能体」分区状态机与纯计算（从 agentConnections.tsx 拆出以控制单文件行数，并消除环依赖）。

export interface AgentProfileDto {
  id: string;
  display_name: string;
  icon: string;
  global_skills_dir: string;
  project_skills_rel: string;
  installed: boolean;
  enabled: boolean;
  synced_count: number;
}

export interface ManagedStateDto {
  active_skill_names: string[];
  suspended_skill_names: string[];
}

export interface ManagedSkipDto {
  skill_name: string;
  code: string;
  path: string;
  reason: string;
}

export interface ManagedFailureDto {
  skill_name: string;
  error: string;
}

export interface ManagedReportDto {
  action: "paused" | "restored";
  state: ManagedStateDto;
  succeeded: string[];
  skipped: ManagedSkipDto[];
  failed: ManagedFailureDto[];
}

export type AgentFilter = "all" | "enabled" | "disabled";

export interface AgentNotice {
  tone: ToastTone;
  text: string;
  openPath?: string;
}

export type DeployMode = "symlink" | "copy";
export type AgentDeployModeChoice = "inherit" | DeployMode;

export interface DeployModesDto {
  defaultMode: DeployMode;
  overrides: Record<string, DeployMode>;
  disallowJunction: string[];
}

export interface ReconcileModeReportDto {
  updated: number;
  skipped_modified: number;
  skipped_foreign: number;
  failures: string[];
}

export interface AgentConnectionsBridge {
  listProfiles(): Promise<AgentProfileDto[]>;
  toggleProfile(id: string): Promise<boolean>;
  listLinkedSkills(id: string): Promise<string[]>;
  unlinkSkill(skill: string, agentId: string): Promise<void>;
  unlinkAll(agentId: string): Promise<number>;
  getManagedState(agentId: string): Promise<ManagedStateDto>;
  toggleManaged(agentId: string): Promise<ManagedReportDto>;
  getDeployModes?(): Promise<DeployModesDto>;
  setDefaultDeployMode?(mode: DeployMode): Promise<{
    defaultMode: DeployMode;
    reconciled: ReconcileModeReportDto;
  }>;
  setAgentDeployMode?(
    agentId: string,
    mode: AgentDeployModeChoice,
  ): Promise<{
    overrides: Record<string, DeployMode>;
    reconciled: ReconcileModeReportDto;
  }>;
}

function asProfiles(value: unknown): AgentProfileDto[] {
  const raw = Array.isArray(value) ? value : [];
  return raw.map((row) => {
    const item = (row ?? {}) as Partial<AgentProfileDto>;
    return {
      id: typeof item.id === "string" ? item.id : "",
      display_name: typeof item.display_name === "string" ? item.display_name : "",
      icon: typeof item.icon === "string" ? item.icon : "",
      global_skills_dir:
        typeof item.global_skills_dir === "string" ? item.global_skills_dir : "",
      project_skills_rel:
        typeof item.project_skills_rel === "string" ? item.project_skills_rel : "",
      installed: item.installed === true,
      enabled: item.enabled === true,
      synced_count: typeof item.synced_count === "number" ? item.synced_count : 0,
    };
  });
}

function asManagedState(value: unknown): ManagedStateDto {
  const raw = (value ?? {}) as Partial<ManagedStateDto>;
  const list = (names: unknown): string[] =>
    Array.isArray(names) ? names.filter((name): name is string => typeof name === "string") : [];
  return {
    active_skill_names: list(raw.active_skill_names),
    suspended_skill_names: list(raw.suspended_skill_names),
  };
}

function asManagedReport(value: unknown): ManagedReportDto {
  const raw = (value ?? {}) as Partial<ManagedReportDto>;
  return {
    action: raw.action === "restored" ? "restored" : "paused",
    state: asManagedState(raw.state),
    succeeded: Array.isArray(raw.succeeded)
      ? raw.succeeded.filter((name): name is string => typeof name === "string")
      : [],
    skipped: Array.isArray(raw.skipped)
      ? raw.skipped.filter((item): item is ManagedSkipDto => item !== null && typeof item === "object")
      : [],
    failed: Array.isArray(raw.failed)
      ? raw.failed.filter((item): item is ManagedFailureDto => item !== null && typeof item === "object")
      : [],
  };
}

function asDeployMode(value: unknown): DeployMode {
  return value === "copy" ? "copy" : "symlink";
}

function asDeployOverrides(value: unknown): Record<string, DeployMode> {
  if (value === null || typeof value !== "object") return {};
  const result: Record<string, DeployMode> = {};
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    if (v === "copy" || v === "symlink") {
      result[k] = v;
    }
  }
  return result;
}

function asDeployModes(value: unknown): DeployModesDto {
  const raw = (value ?? {}) as Record<string, unknown>;
  return {
    defaultMode: asDeployMode(raw.defaultMode),
    overrides: asDeployOverrides(raw.overrides),
    disallowJunction: Array.isArray(raw.disallowJunction)
      ? raw.disallowJunction.filter((id): id is string => typeof id === "string")
      : [],
  };
}

function asReconcileReport(value: unknown): ReconcileModeReportDto {
  const raw = (value ?? {}) as Record<string, unknown>;
  return {
    updated: typeof raw.updated === "number" ? raw.updated : 0,
    skipped_modified: typeof raw.skipped_modified === "number" ? raw.skipped_modified : 0,
    skipped_foreign: typeof raw.skipped_foreign === "number" ? raw.skipped_foreign : 0,
    failures: Array.isArray(raw.failures)
      ? raw.failures.filter((item): item is string => typeof item === "string")
      : [],
  };
}

export const realAgentConnectionsBridge: AgentConnectionsBridge = {
  async listProfiles() {
    return asProfiles(await SettingsAgentConnections.listProfiles());
  },
  async toggleProfile(id) {
    const enabled = await SettingsAgentConnections.toggleProfile(id);
    if (typeof enabled !== "boolean") throw new Error("unexpected toggle wire");
    return enabled;
  },
  async listLinkedSkills(id) {
    const names = await SettingsAgentConnections.listLinkedSkills(id);
    return Array.isArray(names)
      ? names.filter((name): name is string => typeof name === "string")
      : [];
  },
  async unlinkSkill(skill, agentId) {
    await SettingsAgentConnections.unlinkSkillFromAgent(skill, agentId);
  },
  async unlinkAll(agentId) {
    const removed = await SettingsAgentConnections.unlinkAllSkillsFromAgent(agentId);
    return typeof removed === "number" ? removed : 0;
  },
  async getManagedState(agentId) {
    return asManagedState(await SettingsAgentConnections.getManagedState(agentId));
  },
  async toggleManaged(agentId) {
    return asManagedReport(await SettingsAgentConnections.toggleManagedSkills(agentId));
  },
  async getDeployModes() {
    return asDeployModes(await SettingsAgentConnections.getDeployModes());
  },
  async setDefaultDeployMode(mode) {
    const raw = (await SettingsAgentConnections.setDefaultDeployMode(mode)) as Record<
      string,
      unknown
    >;
    return {
      defaultMode: asDeployMode(raw?.defaultMode),
      reconciled: asReconcileReport(raw?.reconciled),
    };
  },
  async setAgentDeployMode(agentId, mode) {
    const raw = (await SettingsAgentConnections.setAgentDeployMode(
      agentId,
      mode === "inherit" ? null : mode,
    )) as Record<string, unknown>;
    return {
      overrides: asDeployOverrides(raw?.overrides),
      reconciled: asReconcileReport(raw?.reconciled),
    };
  },
};

export const VISIBLE_AGENTS = 10;
export const SKIP_UNMANAGED_REAL_DIRECTORY = "unmanaged_real_directory";
export const SKIP_FOREIGN_LINK = "foreign_link";

export function managedTargetKeyOf(path: string): string {
  return path.trim().replaceAll("\\", "/").replace(/\/+$/, "");
}

export function hasGlobalSkills(profile: AgentProfileDto): boolean {
  return profile.global_skills_dir.length > 0;
}

export function matchedAgents(
  profiles: AgentProfileDto[],
  query: string,
  filter: AgentFilter,
): AgentProfileDto[] {
  const ordered = [...profiles].sort((a, b) => Number(!a.enabled) - Number(!b.enabled));
  const needle = query.trim().toLowerCase();
  const searched = needle
    ? ordered.filter(
        (profile) =>
          profile.display_name.toLowerCase().includes(needle) ||
          profile.id.toLowerCase().includes(needle),
      )
    : ordered;
  return searched.filter((profile) => {
    if (filter === "enabled") return profile.enabled;
    if (filter === "disabled") return !profile.enabled;
    return true;
  });
}

export function statusCounts(profiles: AgentProfileDto[], query: string): [number, number, number] {
  const needle = query.trim().toLowerCase();
  const searched = profiles.filter(
    (profile) =>
      needle === "" ||
      profile.display_name.toLowerCase().includes(needle) ||
      profile.id.toLowerCase().includes(needle),
  );
  const enabled = searched.filter((profile) => profile.enabled).length;
  return [searched.length, enabled, searched.length - enabled];
}

export function homePrefix(path: string, home: string): string {
  return home !== "" && path.startsWith(home) ? `~${path.slice(home.length)}` : path;
}

export function displayPaths(profile: AgentProfileDto, home: string): string[] {
  const primary = homePrefix(profile.global_skills_dir.trim(), home);
  if (primary === "") return [];
  if (profile.id !== "codex") return [primary];
  const legacy = homePrefix(`${home}/.agents/skills`, home);
  return primary.toLowerCase() === legacy.toLowerCase() ? [primary] : [primary, legacy];
}

export function sharedNames(profiles: AgentProfileDto[], profile: AgentProfileDto): string[] {
  if (!hasGlobalSkills(profile)) return [];
  const key = managedTargetKeyOf(profile.global_skills_dir);
  return profiles
    .filter(
      (candidate) =>
        candidate.id !== profile.id &&
        hasGlobalSkills(candidate) &&
        managedTargetKeyOf(candidate.global_skills_dir) === key,
    )
    .map((candidate) => candidate.display_name);
}

export function noticeFromReport(name: string, report: ManagedReportDto): AgentNotice {
  const failed = report.failed.length;
  const skipped = report.skipped.length;
  const total = report.succeeded.length + skipped + failed;
  const paused = report.action === "paused";

  const details: string[] = report.failed
    .slice(0, 3)
    .map((failure) => `${failure.skill_name}: ${failure.error}`);
  const hiddenFailures = failed - Math.min(failed, 3);
  if (hiddenFailures > 0) {
    details.push(tf("settings.managedSkillsMoreFailures", { count: String(hiddenFailures) }));
  }
  const visibleSkips = report.skipped
    .slice(0, 3)
    .map((skip) =>
      skip.code === SKIP_UNMANAGED_REAL_DIRECTORY
        ? tf("skillToggle.skipUnmanagedDirItem", { name: skip.skill_name, path: skip.path })
        : skip.code === SKIP_FOREIGN_LINK
          ? tf("skillToggle.skipForeignLinkItem", { name: skip.skill_name, path: skip.path })
          : skip.reason === ""
            ? skip.skill_name
            : `${skip.skill_name}: ${skip.reason}`,
    );
  const hiddenSkips = skipped - Math.min(skipped, 3);
  details.push(...visibleSkips);
  if (hiddenSkips > 0) {
    details.push(tf("settings.managedSkillsMoreSkipped", { count: String(hiddenSkips) }));
  }
  const openPath = report.skipped.find((skip) => skip.path.trim() !== "")?.path;

  let tone: AgentNotice["tone"];
  let head: string;
  if (failed > 0) {
    const key =
      paused && skipped > 0
        ? "settings.managedSkillsPausePartialMixed"
        : paused
          ? "settings.managedSkillsPausePartialFailed"
          : skipped > 0
            ? "settings.managedSkillsRestorePartialMixed"
            : "settings.managedSkillsRestorePartialFailed";
    tone = "error";
    head = tf(key, { failed: String(failed), skipped: String(skipped), total: String(total) });
  } else if (skipped > 0) {
    tone = "warning";
    head = tf(
      paused ? "settings.managedSkillsPausePartialSkipped" : "settings.managedSkillsRestorePartialSkipped",
      { skipped: String(skipped), total: String(total) },
    );
  } else {
    tone = "success";
    head = tf(paused ? "settings.managedSkillsPaused" : "settings.managedSkillsRestored", {
      count: String(report.succeeded.length),
      name,
    });
  }
  const text = details.length === 0 ? head : `${head} · ${details.join(" · ")}`;
  return openPath === undefined ? { tone, text } : { tone, text, openPath };
}

export function sharedNamesByDir(list: AgentProfileDto[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const item of list) {
    if (!hasGlobalSkills(item)) continue;
    const key = managedTargetKeyOf(item.global_skills_dir);
    let group = map.get(key);
    if (group === undefined) {
      group = [];
      map.set(key, group);
    }
    group.push(item.display_name);
  }
  return map;
}

export interface AgentConnectionsState {
  profiles: Signal<AgentProfileDto[]>;
  agentQuery: Signal<string>;
  agentFilter: Signal<AgentFilter>;
  showAllAgents: Signal<boolean>;
  expandedAgent: Signal<string | null>;
  linkedSkills: Signal<Record<string, string[]>>;
  managedStates: Signal<Record<string, ManagedStateDto>>;
  managedPending: Signal<Record<string, boolean>>;
  defaultDeployMode: Signal<DeployMode>;
  agentDeployOverrides: Signal<Record<string, DeployMode>>;
  disallowJunctionAgents: Signal<string[]>;
  deployModePending: Signal<boolean>;
  deployModeAvailable: Signal<boolean>;
  start(): Promise<void>;
  setAgentFilter(filter: AgentFilter): void;
  editAgentQuery(value: string): void;
  clearAgentFilters(): void;
  toggleShowAllAgents(): void;
  toggleAgentExpand(agentId: string): Promise<void>;
  toggleAgentEnabled(agentId: string): Promise<void>;
  unlinkLinkedSkill(agentId: string, skill: string): void;
  unlinkAllLinkedSkills(agentId: string): void;
  toggleManagedSkills(agentId: string): void;
  setDefaultDeployMode(mode: DeployMode): void;
  setAgentDeployMode(agentId: string, choice: AgentDeployModeChoice): void;
}

export function createAgentConnectionsState(bridge: AgentConnectionsBridge): AgentConnectionsState {
  const profiles = signal<AgentProfileDto[]>([]);
  const agentQuery = signal("");
  const agentFilter = signal<AgentFilter>("all");
  const showAllAgents = signal(false);
  const expandedAgent = signal<string | null>(null);
  const linkedSkills = signal<Record<string, string[]>>({});
  const managedStates = signal<Record<string, ManagedStateDto>>({});
  const managedPending = signal<Record<string, boolean>>({});
  const defaultDeployMode = signal<DeployMode>("symlink");
  const agentDeployOverrides = signal<Record<string, DeployMode>>({});
  const disallowJunctionAgents = signal<string[]>([]);
  const deployModePending = signal(false);
  const deployModeAvailable = signal(false);

  const epochs: Record<string, number> = {};
  const requests: Record<string, number> = {};
  const begin = (key: string): [number, number] => {
    requests[key] = (requests[key] ?? 0) + 1;
    return [epochs[key] ?? 0, requests[key]];
  };
  const invalidate = (key: string): void => {
    epochs[key] = (epochs[key] ?? 0) + 1;
  };
  const accepts = (key: string, epoch: number, request: number): boolean =>
    (epochs[key] ?? 0) === epoch && requests[key] === request;

  const pendingOf = (key: string): boolean => managedPending()[key] === true;
  const setPending = (key: string, value: boolean): void => {
    const next = { ...managedPending() };
    if (value) next[key] = true;
    else delete next[key];
    managedPending.set(next);
  };

  const showNotice = (notice: AgentNotice, kind: "result" | "read", key: string): void => {
    const pinned = kind === "read" || notice.openPath !== undefined;
    publishToast({
      tone: notice.tone,
      message: notice.text,
      replaceKey: `${kind === "result" ? "managed-result" : "managed-read"}-${key}`,
      ...(pinned ? { pinned: true } : {}),
      ...(notice.openPath !== undefined
        ? {
            action: {
              label: t("skillToggle.openOccupiedFolder"),
              run: () => {
                SettingsStorage.revealPath(notice.openPath ?? "");
              },
            },
          }
        : {}),
    });
  };

  const errorNotice = (key: string, message: string): void =>
    showNotice({ tone: "error", text: message }, "result", key);

  const noteReadError = (purpose: "preload" | "refresh", key: string, err: string): void => {
    showNotice(
      {
        tone: purpose === "preload" ? "error" : "warning",
        text:
          purpose === "preload"
            ? tf("settings.connectionFailed", { error: err })
            : t("settings.managedSkillsRefreshFailed"),
      },
      "read",
      key,
    );
  };

  const applyManagedStatesBatch = (updates: { key: string; state: ManagedStateDto }[]): void => {
    if (updates.length === 0) return;
    const states = { ...managedStates() };
    const linked = { ...linkedSkills() };
    const byKey = new Map<string, string[]>();
    for (const item of updates) {
      states[item.key] = item.state;
      byKey.set(item.key, item.state.active_skill_names);
    }
    const next = profiles().map((profile) => {
      if (!hasGlobalSkills(profile)) return profile;
      const names = byKey.get(managedTargetKeyOf(profile.global_skills_dir));
      if (names === undefined) return profile;
      linked[profile.id] = [...names];
      return { ...profile, synced_count: names.length };
    });
    batch(() => {
      managedStates.set(states);
      linkedSkills.set(linked);
      profiles.set(next);
    });
  };

  const applyManagedState = (key: string, state: ManagedStateDto): void => {
    applyManagedStatesBatch([{ key, state }]);
  };

  const spawnManagedReads = (
    jobs: { key: string; agentId: string; purpose: "preload" | "refresh" }[],
  ): void => {
    if (jobs.length === 0) return;
    const armed = jobs.map((job) => ({ ...job, guard: begin(job.key) }));
    void Promise.all(
      armed.map((job) =>
        bridge
          .getManagedState(job.agentId)
          .then((state) => ({ job, state, error: null as string | null }))
          .catch((err: unknown) => ({ job, state: null, error: String(err) })),
      ),
    ).then((rows) => {
      const accepted: { key: string; state: ManagedStateDto }[] = [];
      for (const row of rows) {
        if (!accepts(row.job.key, row.job.guard[0], row.job.guard[1])) continue;
        if (row.error !== null) {
          noteReadError(row.job.purpose, row.job.key, row.error);
        } else if (row.state !== null) {
          accepted.push({ key: row.job.key, state: row.state });
        }
      }
      applyManagedStatesBatch(accepted);
    });
  };

  const refreshManaged = (agentId: string, dir: string): void => {
    spawnManagedReads([{ key: managedTargetKeyOf(dir), agentId, purpose: "refresh" }]);
  };

  const refreshManagedKey = (key: string): void => {
    const profile = profiles().find(
      (item) => hasGlobalSkills(item) && managedTargetKeyOf(item.global_skills_dir) === key,
    );
    if (profile === undefined) return;
    spawnManagedReads([{ key, agentId: profile.id, purpose: "refresh" }]);
  };

  const preloadManagedSkills = (list: AgentProfileDto[]): void => {
    const seen = new Set<string>();
    const jobs: { key: string; agentId: string; purpose: "preload" | "refresh" }[] = [];
    for (const profile of list) {
      if (!profile.enabled || !hasGlobalSkills(profile)) continue;
      const key = managedTargetKeyOf(profile.global_skills_dir);
      if (seen.has(key)) continue;
      seen.add(key);
      jobs.push({ key, agentId: profile.id, purpose: "preload" });
    }
    spawnManagedReads(jobs);
  };

  const forgetLinkedSkill = (agentId: string, skill: string): void => {
    const profile = profiles().find((item) => item.id === agentId && hasGlobalSkills(item));
    const key = profile ? managedTargetKeyOf(profile.global_skills_dir) : null;
    const ids = key
      ? profiles()
          .filter(
            (item) =>
              hasGlobalSkills(item) && managedTargetKeyOf(item.global_skills_dir) === key,
          )
          .map((item) => item.id)
      : [agentId];
    const linked = { ...linkedSkills() };
    for (const id of ids) {
      if (linked[id]) linked[id] = linked[id].filter((name) => name !== skill);
    }
    linkedSkills.set(linked);
    profiles.set(
      profiles().map((item) =>
        ids.includes(item.id)
          ? { ...item, synced_count: Math.max(0, item.synced_count - 1) }
          : item,
      ),
    );
    if (key !== null) {
      const states = { ...managedStates() };
      if (states[key]) {
        states[key] = {
          ...states[key],
          active_skill_names: states[key].active_skill_names.filter((name) => name !== skill),
        };
        managedStates.set(states);
      }
    }
  };

  const forgetAllLinkedSkills = (key: string): void => {
    const ids = profiles()
      .filter((item) => hasGlobalSkills(item) && managedTargetKeyOf(item.global_skills_dir) === key)
      .map((item) => item.id);
    const linked = { ...linkedSkills() };
    for (const id of ids) delete linked[id];
    linkedSkills.set(linked);
    profiles.set(
      profiles().map((item) => (ids.includes(item.id) ? { ...item, synced_count: 0 } : item)),
    );
    const states = { ...managedStates() };
    if (states[key]) {
      states[key] = { active_skill_names: [], suspended_skill_names: [] };
      managedStates.set(states);
    }
  };

  const clearedLinksNotice = (removed: number, name: string): AgentNotice => {
    if (removed === 0) return { tone: "success", text: t("settings.unlinkAllFromAgentNone") };
    return {
      tone: "success",
      text: tf("settings.unlinkAllFromAgentDone", { count: String(removed), name }),
    };
  };

  const announceReconcile = (reconciled: ReconcileModeReportDto): void => {
    if (reconciled.failures.length > 0) {
      publishToast({
        tone: "error",
        message: `${st("settings.deployModeSaveFailed")}: ${reconciled.failures[0]}`,
      });
      return;
    }
    const skipped = reconciled.skipped_modified + reconciled.skipped_foreign;
    publishToast({
      tone: skipped > 0 ? "warning" : "success",
      message:
        skipped > 0
          ? stf("settings.deployModeUpdatedWithSkipped", {
              updated: reconciled.updated,
              skipped,
            })
          : stf("settings.deployModeUpdated", { updated: reconciled.updated }),
    });
  };

  let startSeq = 0;

  const refreshAfterDeployModeChange = async (): Promise<void> => {
    const linkedIds = Object.keys(linkedSkills());
    const [reloaded, linkedPairs] = await Promise.all([
      bridge.listProfiles(),
      Promise.all(
        linkedIds.map(async (id) => [id, await bridge.listLinkedSkills(id)] as const),
      ),
    ]);
    const nextLinked = { ...linkedSkills() };
    for (const [id, names] of linkedPairs) {
      nextLinked[id] = names;
    }
    batch(() => {
      profiles.set(reloaded);
      linkedSkills.set(nextLinked);
    });
    preloadManagedSkills(reloaded);
    agentsChanged.publish();
  };

  const state: AgentConnectionsState = {
    profiles,
    agentQuery,
    agentFilter,
    showAllAgents,
    expandedAgent,
    linkedSkills,
    managedStates,
    managedPending,
    defaultDeployMode,
    agentDeployOverrides,
    disallowJunctionAgents,
    deployModePending,
    deployModeAvailable,
    async start() {
      const seq = ++startSeq;
      const [loaded, modes] = await Promise.all([
        bridge.listProfiles(),
        bridge.getDeployModes ? bridge.getDeployModes() : Promise.resolve(null),
      ]);
      if (seq !== startSeq) return;
      batch(() => {
        profiles.set(loaded);
        if (modes !== null) {
          defaultDeployMode.set(modes.defaultMode);
          agentDeployOverrides.set(modes.overrides);
          disallowJunctionAgents.set(modes.disallowJunction);
          deployModeAvailable.set(true);
        } else {
          deployModeAvailable.set(false);
        }
      });
      preloadManagedSkills(loaded);
    },
    setAgentFilter(filter) {
      agentFilter.set(filter);
    },
    editAgentQuery(value) {
      agentQuery.set(value);
    },
    clearAgentFilters() {
      batch(() => {
        agentQuery.set("");
        agentFilter.set("all");
      });
    },
    toggleShowAllAgents() {
      showAllAgents.set(!showAllAgents());
    },
    async toggleAgentExpand(agentId) {
      if (expandedAgent() === agentId) {
        expandedAgent.set(null);
        return;
      }
      expandedAgent.set(agentId);
      const profile = profiles().find((item) => item.id === agentId);
      if (profile === undefined) return;
      if (hasGlobalSkills(profile)) {
        refreshManaged(agentId, profile.global_skills_dir);
      } else if (linkedSkills()[agentId] === undefined) {
        try {
          const names = await bridge.listLinkedSkills(agentId);
          linkedSkills.set({ ...linkedSkills(), [agentId]: names });
        } catch {
          errorNotice(agentId, t("settings.listLinkedFailed"));
        }
      }
    },
    async toggleAgentEnabled(agentId) {
      let enabled: boolean;
      try {
        enabled = await bridge.toggleProfile(agentId);
      } catch {
        errorNotice(agentId, t("settings.toggleFailed"));
        return;
      }
      const current = profiles().find((item) => item.id === agentId);
      const next = profiles().map((item) =>
        item.id === agentId ? { ...item, enabled } : item,
      );
      profiles.set(next);
      if (enabled && current !== undefined && hasGlobalSkills(current)) {
        refreshManaged(agentId, current.global_skills_dir);
      }
      agentsChanged.publish();
    },
    unlinkLinkedSkill(agentId, skill) {
      const profile = profiles().find((item) => item.id === agentId && hasGlobalSkills(item));
      const key = profile ? managedTargetKeyOf(profile.global_skills_dir) : null;
      if (key !== null) {
        if (pendingOf(key)) return;
        invalidate(key);
      }
      void bridge
        .unlinkSkill(skill, agentId)
        .then(() => {
          forgetLinkedSkill(agentId, skill);
          agentsChanged.publish();
          if (key !== null) refreshManagedKey(key);
        })
        .catch(() => {
          errorNotice(agentId, t("settings.unlinkFailed"));
          if (key !== null) refreshManagedKey(key);
        });
    },
    unlinkAllLinkedSkills(agentId) {
      const profile = profiles().find((item) => item.id === agentId && hasGlobalSkills(item));
      if (profile === undefined) return;
      const key = managedTargetKeyOf(profile.global_skills_dir);
      if (pendingOf(key)) return;
      invalidate(key);
      setPending(key, true);
      const name = profile.display_name;
      void bridge
        .unlinkAll(agentId)
        .then((removed) => {
          setPending(key, false);
          forgetAllLinkedSkills(key);
          showNotice(clearedLinksNotice(removed, name), "result", key);
          agentsChanged.publish();
          refreshManagedKey(key);
        })
        .catch(() => {
          setPending(key, false);
          errorNotice(key, t("settings.unlinkAllFromAgentFailed"));
          refreshManagedKey(key);
        });
    },
    toggleManagedSkills(agentId) {
      const profile = profiles().find((item) => item.id === agentId);
      if (profile === undefined) return;
      if (!profile.enabled || !hasGlobalSkills(profile)) return;
      const key = managedTargetKeyOf(profile.global_skills_dir);
      if (pendingOf(key)) return;
      invalidate(key);
      setPending(key, true);
      const name = profile.display_name;
      void bridge
        .toggleManaged(agentId)
        .then((report) => {
          setPending(key, false);
          applyManagedState(key, report.state);
          showNotice(noticeFromReport(name, report), "result", key);
          agentsChanged.publish();
          refreshManagedKey(key);
        })
        .catch(() => {
          setPending(key, false);
          errorNotice(key, t("settings.managedSkillsOperationFailed"));
          refreshManagedKey(key);
        });
    },
    setDefaultDeployMode(mode) {
      if (deployModePending() || bridge.setDefaultDeployMode === undefined) return;
      deployModePending.set(true);
      void bridge
        .setDefaultDeployMode(mode)
        .then(async (res) => {
          batch(() => {
            deployModePending.set(false);
            defaultDeployMode.set(res.defaultMode);
          });
          announceReconcile(res.reconciled);
          await refreshAfterDeployModeChange();
        })
        .catch(() => {
          deployModePending.set(false);
          publishToast({ tone: "error", message: st("settings.deployModeSaveFailed") });
        });
    },
    setAgentDeployMode(agentId, choice) {
      if (deployModePending() || bridge.setAgentDeployMode === undefined) return;
      deployModePending.set(true);
      void bridge
        .setAgentDeployMode(agentId, choice)
        .then(async (res) => {
          batch(() => {
            deployModePending.set(false);
            agentDeployOverrides.set(res.overrides);
          });
          announceReconcile(res.reconciled);
          await refreshAfterDeployModeChange();
        })
        .catch(() => {
          deployModePending.set(false);
          publishToast({ tone: "error", message: st("settings.deployModeSaveFailed") });
        });
    },
  };
  return state;
}
