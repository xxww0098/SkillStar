import { computed, signal } from "nsh";
import { toast } from "../../shell/events";
import type { McpBridge, MutationResult } from "./bridge";
import { sameServerConfig } from "./editorValidation";
import { ms } from "./mcpStrings";
import type {
  BackupEntry,
  CheckResult,
  ConflictStrategy,
  FoundServer,
  LibrarySnapshot,
  McpAgentTarget,
  McpServerRecord,
  ProjectSnippet,
  StoreItem,
  SyncWarning,
} from "./types";

const EMPTY_SET: ReadonlySet<string> = new Set();
const STORE_DEBOUNCE_MS = 200;
const PROBE_ALL_CONCURRENCY = 4;

function sameElements<T>(prev: readonly T[], next: readonly T[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

export type McpTab = "managed" | "store" | "project";
export type StoreFilter = "all" | "featured" | "stdio" | "remote";

export type McpModalState =
  | { kind: "editor"; originalName: string | null; draft: McpServerRecord }
  | { kind: "paste"; raw: string; error: string | null }
  | {
      kind: "storeInstall";
      item: StoreItem;
      customName: string;
      values: Record<string, string>;
      agents: string[];
      error?: string | null;
    }
  | {
      kind: "importConflict";
      agentId: string;
      name: string;
      existing: McpServerRecord;
      discovered: McpServerRecord;
    }
  | { kind: "backups" };

export function createMcpState(bridge: McpBridge) {
  const servers = signal<McpServerRecord[]>([]);
  const agents = signal<McpAgentTarget[]>([]);
  const found = signal<FoundServer[]>([]);
  const warnings = signal<SyncWarning[]>([]);
  const started = signal(false);
  const loading = signal(false);
  const busy = signal(false);
  const backups = signal<BackupEntry[]>([]);
  const backupsLoading = signal(false);

  const tab = signal<McpTab>("managed");
  const search = signal("");
  const sortOrder = signal<"asc" | "desc">("asc");
  const selectionMode = signal(false);
  const selectedNames = signal<string[]>([]);
  const installedOnlyAgents = signal(true);
  const selectedName = signal<string | null>(null);
  /** Discovered-but-unmanaged strip: open state, and whether the user chose it. */
  const foundOpen = signal(true);
  const agentQuery = signal("");
  let foundOpenChosen = false;
  /** Set when the user closes the detail column, so a refresh does not reopen it. */
  let detailDismissed = false;

  const storeQuery = signal("");
  const storeOnline = signal(false);
  const storeFilter = signal<StoreFilter>("all");
  const storeItems = signal<StoreItem[]>([]);
  const storeLoading = signal(false);
  const storeOnlineError = signal<string | null>(null);

  const projectSnippets = signal<ProjectSnippet[]>([]);
  const selectedProjectAgent = signal("claude");

  const probeResults = signal<Record<string, CheckResult>>({});
  const probingName = signal<string | null>(null);
  const probingSet = signal<ReadonlySet<string>>(EMPTY_SET);
  const modal = signal<McpModalState | null>(null);
  const pageActive = signal(true);

  let storeEpoch = 0;
  let cancelStoreDebounce: ReturnType<typeof setTimeout> | null = null;

  const gated = <T,>(
    read: () => T,
    fallback: T,
    equals?: (prev: T, next: T) => boolean,
  ) =>
    computed<T>(
      (prev) => (!pageActive() ? (prev !== undefined ? prev : fallback) : read()),
      equals ? { equals } : undefined,
    );

  function reconcileSelection(nextServers: readonly McpServerRecord[]): void {
    const sel = selectedName();
    if (sel !== null && !nextServers.some((s) => s.name === sel)) {
      selectedName.set(nextServers[0]?.name ?? null);
    } else if (sel === null && !detailDismissed) {
      selectedName.set(nextServers[0]?.name ?? null);
    }
  }

  function applySnapshot(snap: LibrarySnapshot): void {
    servers.set(snap.servers);
    agents.set(snap.agents);
    found.set(snap.found);
    warnings.set(snap.warnings);
    reconcileSelection(snap.servers);
    if (!foundOpenChosen) foundOpen.set(snap.found.length <= 3);
    if (tab() === "project") void loadProjectSnippets();
  }

  function applyMutation(res: MutationResult): void {
    if ("agents" in res && "found" in res) {
      applySnapshot(res);
      return;
    }
    servers.set(res.servers);
    warnings.set(res.warnings);
    reconcileSelection(res.servers);
    if (tab() === "project") void loadProjectSnippets();
  }

  function setFoundOpen(open: boolean): void {
    foundOpenChosen = true;
    foundOpen.set(open);
  }

  /** Open the detail column on `name`; `null` closes it for good this session. */
  function selectServer(name: string | null): void {
    detailDismissed = name === null;
    selectedName.set(name);
  }

  async function refresh(): Promise<void> {
    loading.set(true);
    try {
      const snap = await bridge.snapshot();
      applySnapshot(snap);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      loading.set(false);
    }
  }

  async function ensureStarted(): Promise<void> {
    if (started()) return;
    started.set(true);
    await refresh();
  }

  const filteredServers = gated<McpServerRecord[]>(
    () => {
      const q = search().trim().toLowerCase();
      const order = sortOrder();
      const list = servers().filter((srv) => {
        if (q.length === 0) return true;
        return (
          srv.name.toLowerCase().includes(q) ||
          srv.command.toLowerCase().includes(q) ||
          srv.url.toLowerCase().includes(q) ||
          srv.args.some((a) => a.toLowerCase().includes(q))
        );
      });
      return [...list].sort((a, b) => {
        const cmp = a.name.localeCompare(b.name);
        return order === "asc" ? cmp : -cmp;
      });
    },
    [],
    sameElements,
  );

  /** Discovered entries, narrowed by the page search so one query drives both lists. */
  const filteredFound = gated<FoundServer[]>(
    () => {
      const q = search().trim().toLowerCase();
      if (q.length === 0) return found();
      return found().filter(
        (f) =>
          f.server.name.toLowerCase().includes(q) ||
          f.agentName.toLowerCase().includes(q) ||
          f.server.command.toLowerCase().includes(q) ||
          f.server.url.toLowerCase().includes(q),
      );
    },
    [],
    sameElements,
  );

  const agentById = gated<ReadonlyMap<string, McpAgentTarget>>(
    () => new Map(agents().map((a) => [a.id, a])),
    new Map(),
  );

  const selectedSet = gated<ReadonlySet<string>>(
    () => (selectedNames().length === 0 ? EMPTY_SET : new Set(selectedNames())),
    EMPTY_SET,
  );

  const pausedCount = gated<number>(
    () => servers().reduce((acc, s) => acc + (s.paused ? 1 : 0), 0),
    0,
  );

  const selectedServer = gated<McpServerRecord | null>(() => {
    const name = selectedName();
    if (!name) return null;
    return servers().find((s) => s.name === name) ?? null;
  }, null);

  const visibleAgents = gated<McpAgentTarget[]>(
    () => {
      const all = agents();
      if (!installedOnlyAgents()) return all;
      const activeInServers = new Set(servers().flatMap((s) => s.agents));
      const filtered = all.filter((a) => a.installed || activeInServers.has(a.id));
      return filtered.length > 0 ? filtered : all;
    },
    [],
    sameElements,
  );

  /** Agents the detail grid lists: installed-only scope, narrowed by its own query. */
  const filteredAgents = gated<McpAgentTarget[]>(
    () => {
      const q = agentQuery().trim().toLowerCase();
      const base = visibleAgents();
      if (q.length === 0) return base;
      return base.filter((a) => a.name.toLowerCase().includes(q) || a.id.includes(q));
    },
    [],
    sameElements,
  );

  const filteredStoreItems = gated<StoreItem[]>(
    () => {
      const filter = storeFilter();
      const items = storeItems();
      if (filter === "all") return items;
      if (filter === "featured") return items.filter((item) => item.featured);
      if (filter === "stdio") return items.filter((item) => item.transport === "stdio");
      return items.filter((item) => item.transport === "http" || item.transport === "sse");
    },
    [],
    sameElements,
  );

  const probingCount = gated<number>(() => {
    const set = probingSet();
    const single = probingName();
    if (single === null || set.has(single)) return set.size;
    return set.size + 1;
  }, 0);

  function isProbing(name: string): boolean {
    return probingName() === name || probingSet().has(name);
  }

  function defaultInstallAgents(): string[] {
    return agents()
      .filter((a) => a.installed)
      .map((a) => a.id);
  }

  async function toggleAgent(name: string, agentId: string, enabled: boolean): Promise<void> {
    busy.set(true);
    try {
      const res = await bridge.toggleServerAgent(name, agentId, enabled);
      applyMutation(res);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function togglePaused(name: string, paused: boolean): Promise<void> {
    busy.set(true);
    try {
      const res = await bridge.setServerPaused(name, paused);
      applyMutation(res);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function removeServer(name: string): Promise<void> {
    busy.set(true);
    try {
      const res = await bridge.removeServer(name);
      applyMutation(res);
      toast(ms().removedToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function importFound(
    agentId: string,
    name: string,
    conflict?: ConflictStrategy,
  ): Promise<void> {
    if (conflict === undefined) {
      const existing = servers().find((s) => s.name === name);
      const discovered = found().find(
        (f) => f.agentId === agentId && f.server.name === name,
      );
      if (existing && discovered && !sameServerConfig(existing, discovered.server)) {
        modal.set({
          kind: "importConflict",
          agentId,
          name,
          existing,
          discovered: discovered.server,
        });
        return;
      }
    }

    busy.set(true);
    try {
      const snap = await bridge.importFoundServer(agentId, name, conflict ?? "rename");
      applySnapshot(snap);
      if (modal()?.kind === "importConflict") {
        modal.set(null);
      }
      toast(ms().adoptedToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function removeFound(agentId: string, name: string): Promise<void> {
    busy.set(true);
    try {
      const snap = await bridge.removeFoundServer(agentId, name);
      applySnapshot(snap);
      toast(ms().removedToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function syncAll(): Promise<void> {
    busy.set(true);
    try {
      const snap = await bridge.syncAll();
      applySnapshot(snap);
      toast(ms().syncDoneToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function runCheck(name: string): Promise<void> {
    probingName.set(name);
    try {
      const res = await bridge.checkServer(name);
      probeResults.set({ ...probeResults(), [name]: res });
    } catch (err) {
      probeResults.set({
        ...probeResults(),
        [name]: {
          ok: false,
          serverName: "",
          serverVersion: "",
          tools: [],
          error: String(err),
          durationMs: 0,
        },
      });
    } finally {
      probingName.set(null);
    }
  }

  async function toggleDisabledTool(server: McpServerRecord, toolName: string): Promise<void> {
    const exists = server.disabledTools.includes(toolName);
    const nextDisabled = exists
      ? server.disabledTools.filter((t) => t !== toolName)
      : [...server.disabledTools, toolName];
    const updated: McpServerRecord = { ...server, disabledTools: nextDisabled };
    busy.set(true);
    try {
      const res = await bridge.saveServer(updated, server.name, "replace");
      applyMutation(res);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function saveEditedServer(
    draft: McpServerRecord,
    originalName: string | null,
  ): Promise<boolean> {
    busy.set(true);
    const prevNames = new Set(servers().map((s) => s.name));
    try {
      const res = await bridge.saveServer(
        draft,
        originalName,
        originalName !== null ? "replace" : "rename",
      );
      applyMutation(res);
      const savedEntry =
        originalName === null
          ? res.servers.find((s) => !prevNames.has(s.name)) ??
            res.servers.find((s) => s.name === draft.name)
          : res.servers.find((s) => s.name === draft.name) ??
            res.servers.find((s) => s.name === originalName);
      selectedName.set(savedEntry?.name ?? draft.name);
      modal.set(null);
      toast(ms().savedToast, "success");
      return true;
    } catch (err) {
      toast(String(err), "error");
      return false;
    } finally {
      busy.set(false);
    }
  }

  async function searchStore(q = storeQuery(), online = storeOnline()): Promise<void> {
    if (cancelStoreDebounce !== null) {
      clearTimeout(cancelStoreDebounce);
      cancelStoreDebounce = null;
    }
    storeEpoch += 1;
    const epoch = storeEpoch;
    storeLoading.set(true);
    try {
      const res = await bridge.queryStore(q, online, 60);
      if (epoch !== storeEpoch) return;
      storeItems.set(res.items);
      storeOnlineError.set(res.onlineError ?? null);
    } catch (err) {
      if (epoch !== storeEpoch) return;
      storeOnlineError.set(String(err));
    } finally {
      if (epoch === storeEpoch) {
        storeLoading.set(false);
      }
    }
  }

  function onStoreSearchInput(q: string): void {
    storeQuery.set(q);
    if (cancelStoreDebounce !== null) {
      clearTimeout(cancelStoreDebounce);
    }
    storeEpoch += 1;
    const epoch = storeEpoch;
    const online = storeOnline();
    cancelStoreDebounce = setTimeout(() => {
      cancelStoreDebounce = null;
      if (epoch !== storeEpoch) return;
      void searchStore(q, online);
    }, STORE_DEBOUNCE_MS);
  }

  async function installStoreItem(
    item: StoreItem,
    customName: string,
    values: Record<string, string>,
    targetAgents: string[],
  ): Promise<void> {
    for (const inp of item.inputs) {
      if (inp.required && (values[inp.key] ?? "").trim().length === 0) {
        const msg = `${ms().storeRequiredFieldError}${inp.label || inp.key}`;
        const cur = modal();
        if (cur?.kind === "storeInstall") {
          modal.set({ ...cur, error: msg });
        }
        toast(msg, "error");
        return;
      }
    }
    if (targetAgents.length === 0) {
      const msg = ms().storeNoAgentsBlocked;
      const cur = modal();
      if (cur?.kind === "storeInstall") {
        modal.set({ ...cur, error: msg });
      } else {
        modal.set({
          kind: "storeInstall",
          item,
          customName,
          values,
          agents: targetAgents,
          error: msg,
        });
      }
      toast(msg, "error");
      return;
    }

    busy.set(true);
    try {
      const prevNames = new Set(servers().map((s) => s.name));
      const res = await bridge.installStoreServer({
        item,
        name: customName.trim() || item.name,
        values,
        agents: targetAgents,
      });
      applyMutation(res);
      const added =
        res.servers.find((s) => s.marketId === item.id && !prevNames.has(s.name)) ??
        res.servers.find((s) => !prevNames.has(s.name));
      const installedName = added?.name ?? (customName.trim() || item.name);
      storeItems.set(
        storeItems().map((entry) =>
          entry.id === item.id ? { ...entry, have: installedName } : entry,
        ),
      );
      modal.set(null);
      toast(ms().storeInstalledToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function loadProjectSnippets(): Promise<void> {
    try {
      const list = await bridge.projectSnippets([]);
      projectSnippets.set(list);
      if (list.length > 0 && !list.some((s) => s.agentId === selectedProjectAgent())) {
        selectedProjectAgent.set(list[0].agentId);
      }
    } catch (err) {
      toast(String(err), "error");
    }
  }

  function openNewModal(): void {
    modal.set({
      kind: "editor",
      originalName: null,
      draft: {
        name: "",
        transport: "stdio",
        command: "",
        args: [],
        env: {},
        url: "",
        headers: {},
        paused: false,
        agents: defaultInstallAgents(),
        disabledTools: [],
      },
    });
  }

  function openEditModal(srv: McpServerRecord): void {
    modal.set({
      kind: "editor",
      originalName: srv.name,
      draft: {
        ...srv,
        args: [...srv.args],
        env: { ...srv.env },
        headers: { ...srv.headers },
        agents: [...srv.agents],
        disabledTools: [...srv.disabledTools],
      },
    });
  }

  function openPasteModal(): void {
    modal.set({ kind: "paste", raw: "", error: null });
  }

  async function applyPaste(raw: string): Promise<void> {
    try {
      const parsed = await bridge.parsePaste(raw);
      if (parsed.servers.length === 1) {
        const srv = parsed.servers[0];
        modal.set({
          kind: "editor",
          originalName: null,
          draft: {
            ...srv,
            agents: defaultInstallAgents(),
          },
        });
        toast(`${ms().modalPasteDetected} ${parsed.detectedKind}`, "info");
        return;
      }
      // Multiple pasted servers: import all into library in one pass and switch to managed grid.
      busy.set(true);
      const defaults = defaultInstallAgents();
      const batch = parsed.servers.map((srv) => ({ ...srv, agents: defaults }));
      let res: MutationResult | null = null;
      if (bridge.saveServers) {
        res = await bridge.saveServers(batch, "rename");
      } else {
        for (const srv of batch) {
          res = await bridge.saveServer(srv, null, "rename");
        }
      }
      if (res) applyMutation(res);
      modal.set(null);
      if (tab() !== "project") tab.set("managed");
      toast(ms().savedToast, "success");
    } catch (err) {
      modal.set({ kind: "paste", raw, error: String(err) });
    } finally {
      busy.set(false);
    }
  }

  function triggerInstallStoreItem(item: StoreItem, forceCopy = false): void {
    const existing = item.have
      ? servers().find((s) => s.name === item.have || s.marketId === item.id)
      : undefined;
    if (!forceCopy && item.have && item.inputs.length === 0) {
      if (existing) {
        openEditModal(existing);
      } else {
        selectServer(item.have);
        if (tab() !== "managed") tab.set("managed");
      }
      toast(ms().storeAlreadyInstalledHint, "info");
      return;
    }

    const defaults = defaultInstallAgents();
    if (item.inputs.length > 0 || defaults.length === 0 || forceCopy) {
      modal.set({
        kind: "storeInstall",
        item,
        customName: forceCopy && item.have ? `${item.name}-copy` : item.name,
        values: {},
        agents: defaults,
        error: defaults.length === 0 ? ms().storeNoAgentsNotice : null,
      });
    } else {
      void installStoreItem(item, item.name, {}, defaults);
    }
  }

  async function loadBackups(): Promise<void> {
    if (!bridge.listBackups) {
      backups.set([]);
      return;
    }
    backupsLoading.set(true);
    try {
      backups.set(await bridge.listBackups(40));
    } catch (err) {
      toast(String(err), "error");
    } finally {
      backupsLoading.set(false);
    }
  }

  function openBackupsModal(): void {
    modal.set({ kind: "backups" });
    void loadBackups();
  }

  async function restoreBackup(backupId: string): Promise<void> {
    if (!bridge.restoreBackup) return;
    busy.set(true);
    try {
      const res = await bridge.restoreBackup(backupId, null);
      await refresh();
      await loadBackups();
      toast(res.removedNewFile ? ms().backupsRemovedToast : ms().backupsRestoredToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  function toggleSelectMode(): void {
    const next = !selectionMode();
    selectionMode.set(next);
    if (!next) selectedNames.set([]);
  }

  function toggleSelectServer(name: string): void {
    const cur = selectedNames();
    selectedNames.set(
      cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name],
    );
  }

  function selectAllFiltered(): void {
    selectedNames.set(filteredServers().map((s) => s.name));
  }

  function clearSelection(): void {
    selectedNames.set([]);
  }

  /** Flip the selected server for a set of agents (grid "select all"/"clear"). */
  async function setAllAgents(targets: McpAgentTarget[], enabled: boolean): Promise<void> {
    const srv = selectedServer();
    if (!srv || targets.length === 0) return;
    const ids = targets.map((a) => a.id);
    const next = enabled
      ? Array.from(new Set([...srv.agents, ...ids]))
      : srv.agents.filter((id) => !ids.includes(id));
    busy.set(true);
    try {
      const res = await bridge.setServerAgents(srv.name, next);
      applyMutation(res);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function batchSetPaused(paused: boolean): Promise<void> {
    const names = selectedNames();
    if (names.length === 0) return;
    busy.set(true);
    try {
      let res: MutationResult | null = null;
      if (bridge.batchSetPaused) {
        res = await bridge.batchSetPaused(names, paused);
      } else {
        for (const name of names) {
          res = await bridge.setServerPaused(name, paused);
        }
      }
      if (res) applyMutation(res);
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function batchRemoveSelected(): Promise<void> {
    const names = [...selectedNames()];
    if (names.length === 0) return;
    busy.set(true);
    try {
      let res: MutationResult | null = null;
      if (bridge.batchRemoveServers) {
        res = await bridge.batchRemoveServers(names);
      } else {
        for (const name of names) {
          res = await bridge.removeServer(name);
        }
      }
      if (res) applyMutation(res);
      selectedNames.set([]);
      selectionMode.set(false);
      toast(ms().removedToast, "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      busy.set(false);
    }
  }

  async function probeAllServers(): Promise<void> {
    const active = servers().filter((s) => !s.paused);
    if (active.length === 0) return;
    const names = active.map((s) => s.name);
    probingSet.set(new Set(names));
    let cursor = 0;
    const pendingPatch: Record<string, CheckResult> = {};
    let flushScheduled = false;
    const flushPatch = () => {
      flushScheduled = false;
      const keys = Object.keys(pendingPatch);
      if (keys.length === 0) return;
      const merged = { ...probeResults(), ...pendingPatch };
      for (const k of keys) delete pendingPatch[k];
      probeResults.set(merged);
    };

    const worker = async (): Promise<void> => {
      while (cursor < names.length) {
        const name = names[cursor];
        cursor += 1;
        let res: CheckResult;
        try {
          res = await bridge.checkServer(name);
        } catch (err) {
          res = {
            ok: false,
            serverName: "",
            serverVersion: "",
            tools: [],
            error: String(err),
            durationMs: 0,
          };
        }
        pendingPatch[name] = res;
        const nextSet = new Set(probingSet());
        nextSet.delete(name);
        probingSet.set(nextSet.size === 0 ? EMPTY_SET : nextSet);
        if (!flushScheduled) {
          flushScheduled = true;
          queueMicrotask(flushPatch);
        }
      }
    };

    const count = Math.min(PROBE_ALL_CONCURRENCY, names.length);
    await Promise.all(Array.from({ length: count }, worker));
    flushPatch();
    probingSet.set(EMPTY_SET);
  }

  return {
    bridge,
    servers,
    agents,
    found,
    warnings,
    started,
    loading,
    busy,
    tab,
    search,
    sortOrder,
    selectionMode,
    selectedNames,
    selectedSet,
    pausedCount,
    agentById,
    installedOnlyAgents,
    selectedName,
    foundOpen,
    agentQuery,
    setFoundOpen,
    selectServer,
    storeQuery,
    storeOnline,
    storeFilter,
    storeItems,
    filteredStoreItems,
    storeLoading,
    storeOnlineError,
    projectSnippets,
    selectedProjectAgent,
    probeResults,
    probingName,
    probingCount,
    isProbing,
    modal,
    pageActive,
    ensureStarted,
    refresh,
    filteredServers,
    filteredFound,
    filteredAgents,
    selectedServer,
    visibleAgents,
    defaultInstallAgents,
    toggleAgent,
    togglePaused,
    removeServer,
    importFound,
    removeFound,
    syncAll,
    runCheck,
    toggleDisabledTool,
    saveEditedServer,
    searchStore,
    onStoreSearchInput,
    installStoreItem,
    loadProjectSnippets,
    openNewModal,
    openEditModal,
    openPasteModal,
    applyPaste,
    triggerInstallStoreItem,
    toggleSelectMode,
    toggleSelectServer,
    selectAllFiltered,
    clearSelection,
    setAllAgents,
    batchSetPaused,
    batchRemoveSelected,
    probeAllServers,
    backups,
    backupsLoading,
    loadBackups,
    openBackupsModal,
    restoreBackup,
  };
}

export type McpState = ReturnType<typeof createMcpState>;
