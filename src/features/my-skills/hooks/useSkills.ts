import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  createElement,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTauriEvent } from "../../../hooks/useTauriEvent";
import { installSkillWithProgress } from "../../../lib/installProgress";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import type { InstallStage, Skill, SkillUpdateReport, SkillUpdateState, UpstreamChange } from "../../../types";
import i18n from "../../../i18n";
import { needsAttention } from "../lib/pendingUpdates";

const SKILLS_QUERY_KEY = ["skills"] as const;
const SKILL_UPDATES_QUERY_KEY = ["skills", "updates"] as const;
const SKILL_LIST_REFRESH_INTERVAL_MS = 30_000;
const SKILL_UPDATE_REFRESH_FOREGROUND_MS = 5 * 60 * 1000;
const SKILL_UPDATE_REFRESH_BACKGROUND_MS = 15 * 60 * 1000;

function sameUpstreamChange(
  left: UpstreamChange | null | undefined,
  right: UpstreamChange | null | undefined,
): boolean {
  return JSON.stringify(left ?? null) === JSON.stringify(right ?? null);
}

/** Merge one check result into a cached Skill, keeping identity when nothing changed. */
function withUpdateState(
  skill: Skill,
  state: { update_available: boolean; upstream_change?: UpstreamChange | null },
): Skill {
  const upstream = state.upstream_change ?? null;
  if (skill.update_available === state.update_available && sameUpstreamChange(skill.upstream_change, upstream)) {
    return skill;
  }
  return { ...skill, update_available: state.update_available, upstream_change: upstream };
}

function getSkillUpdateRefreshIntervalMs(): number {
  const isVisible = typeof document === "undefined" ? true : !document.hidden;
  return isVisible ? SKILL_UPDATE_REFRESH_FOREGROUND_MS : SKILL_UPDATE_REFRESH_BACKGROUND_MS;
}

type SkillsState = ReturnType<typeof useSkillsState>;

const SkillsContext = createContext<SkillsState | null>(null);

async function listSkills(): Promise<Skill[]> {
  return tauriInvoke("list_skills");
}

function useSkillsState() {
  const queryClient = useQueryClient();
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [pendingUpdateNames, setPendingUpdateNames] = useState<Set<string>>(new Set());
  const pendingUpdateRef = useRef<Set<string>>(new Set());
  const [pendingAgentToggleKeys, setPendingAgentToggleKeys] = useState<Set<string>>(new Set());
  const pendingAgentToggleRef = useRef<Set<string>>(new Set());
  const [isTogglingAgent, setIsTogglingAgent] = useState(false);

  const updateCheckIntervalMs = getSkillUpdateRefreshIntervalMs();

  const applyUpdateStates = useCallback(
    (updates: SkillUpdateState[]) => {
      if (updates.length === 0) return;

      const updatesByName = new Map(updates.map((update) => [update.name, update]));

      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.length === 0) return prev;

        // Applied as-is: the backend resolves staleness before answering. A
        // scan that began before an update landed loses to it there, so a
        // response can no longer re-assert a badge the update just cleared.
        let changed = false;
        const next = prev.map((skill) => {
          const update = updatesByName.get(skill.name);
          if (!update) return skill;
          const merged = withUpdateState(skill, update);
          if (merged !== skill) changed = true;
          return merged;
        });

        return changed ? next : prev;
      });
    },
    [queryClient],
  );

  const skillsQuery = useQuery({
    queryKey: SKILLS_QUERY_KEY,
    queryFn: listSkills,
    refetchOnWindowFocus: false,
    refetchInterval: isTogglingAgent ? false : SKILL_LIST_REFRESH_INTERVAL_MS,
  });

  const skills = skillsQuery.data ?? [];

  const updatesQuery = useQuery({
    queryKey: SKILL_UPDATES_QUERY_KEY,
    queryFn: () => tauriInvoke("refresh_skill_updates"),
    enabled: skills.length > 0 && !isTogglingAgent,
    refetchOnWindowFocus: false,
    refetchInterval: isTogglingAgent ? false : updateCheckIntervalMs,
    staleTime: updateCheckIntervalMs,
  });

  useEffect(() => {
    if (updatesQuery.data) {
      applyUpdateStates(updatesQuery.data);
    }
  }, [updatesQuery.data, applyUpdateStates]);

  // ── Skills list / update checks ───────────────────────────────────

  const refetchSkills = skillsQuery.refetch;
  const refetchUpdates = updatesQuery.refetch;

  const refresh = useCallback(
    async (_silent = false, force = false) => {
      setRefreshError(null);

      try {
        if (force) {
          await queryClient.invalidateQueries({
            queryKey: SKILLS_QUERY_KEY,
            exact: true,
          });
        }

        await Promise.all([refetchSkills(), refetchUpdates()]);
      } catch (e) {
        setRefreshError(String(e));
      }
    },
    [queryClient, refetchSkills, refetchUpdates],
  );

  useEffect(() => {
    tauriInvoke("migrate_local_skills").catch(() => {});
    void refresh(true, true);
  }, [refresh]);

  useEffect(() => {
    const handleExternalRefresh = () => {
      void refresh(true, true);
    };
    window.addEventListener("skillstar:refresh-skills", handleExternalRefresh);
    return () => {
      window.removeEventListener("skillstar:refresh-skills", handleExternalRefresh);
    };
  }, [refresh]);

  // Rust backend emits "patrol://skill-checked"; merge into query cache. The
  // payload is the state patrol recorded, not the raw check result — a finding
  // overtaken by an update mid-check has already lost to it backend-side.
  useTauriEvent<{ name: string; update_available: boolean; upstream_change?: UpstreamChange | null }>(
    "patrol://skill-checked",
    (state) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        const skill = prev.find((item) => item.name === state.name);
        if (!skill) return prev;
        const merged = withUpdateState(skill, state);
        if (merged === skill) return prev;
        return prev.map((item) => (item.name === state.name ? merged : item));
      });
    },
  );

  const installMutation = useMutation({
    mutationFn: ({
      url,
      name,
      agentId,
      onStage,
    }: {
      url: string;
      name?: string;
      agentId?: string;
      onStage?: (stage: InstallStage, skill: string | undefined) => void;
    }) => installSkillWithProgress({ url, name, agentId }, onStage),
    onSuccess: (skill) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.some((item) => item.name === skill.name)) {
          return prev.map((item) => (item.name === skill.name ? skill : item));
        }
        return [...prev, skill];
      });
      // Deferred: the upstream check hits the network per repository and must
      // not chain into the install interaction's perceived latency.
      window.setTimeout(() => void refetchUpdates(), 1500);
    },
  });

  const installSkillMutate = installMutation.mutateAsync;

  const uninstallMutation = useMutation({
    mutationFn: (name: string) => tauriInvoke("uninstall_skill", { name }),
    onSuccess: (_result, name) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => prev.filter((item) => item.name !== name));
    },
  });
  const uninstallSkillMutate = uninstallMutation.mutateAsync;

  const installSkill = useCallback(
    async (
      url: string,
      name?: string,
      agentId?: string,
      onStage?: (stage: InstallStage, skill: string | undefined) => void,
    ) => {
      const toggleKey = name && agentId ? `${name}::${agentId}` : null;
      if (toggleKey) {
        if (pendingAgentToggleRef.current.has(toggleKey)) {
          const cached = queryClient.getQueryData<Skill[]>(SKILLS_QUERY_KEY)?.find((item) => item.name === name);
          if (cached) return cached;
        }
        pendingAgentToggleRef.current.add(toggleKey);
        setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
        setIsTogglingAgent(true);
      }
      try {
        return await installSkillMutate({ url, name, agentId, onStage });
      } catch (e) {
        throw new Error(String(e));
      } finally {
        if (toggleKey) {
          pendingAgentToggleRef.current.delete(toggleKey);
          setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
          setIsTogglingAgent(pendingAgentToggleRef.current.size > 0);
        }
      }
    },
    [installSkillMutate, queryClient],
  );

  /** Re-scan a repository and reinstall one Skill identity. Missing identity is fail-closed. */
  const reinstallSkill = useCallback(
    async (url: string, name: string) => {
      const scan = await tauriInvoke("scan_github_repo", {
        url,
        fullDepth: true,
      });
      const target = scan.skills.find((skill) => skill.id === name);
      if (!target) {
        throw new Error(i18n.t("mySkills.reinstallSkillMissing", { name }));
      }

      const installed = await tauriInvoke("install_from_scan", {
        spec: scan,
        skills: [{ id: target.id, folder_path: target.folder_path }],
      });

      await refresh(false, true);
      return installed;
    },
    [refresh],
  );

  /** Re-scan one repository at full depth and reinstall every discovered Skill. */
  const reinstallRepoSkills = useCallback(
    async (url: string) => {
      const scan = await tauriInvoke("scan_github_repo", {
        url,
        fullDepth: true,
      });
      if (scan.skills.length === 0) {
        throw new Error(i18n.t("mySkills.reinstallRepoNoSkills"));
      }

      const installed = await tauriInvoke("install_from_scan", {
        spec: scan,
        skills: scan.skills.map((skill) => ({
          id: skill.id,
          folder_path: skill.folder_path,
        })),
      });

      await refresh(false, true);
      return installed;
    },
    [refresh],
  );

  const uninstallSkill = useCallback(
    async (name: string) => {
      try {
        await uninstallSkillMutate(name);
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [uninstallSkillMutate],
  );

  /** The one way to update skills. The backend collapses names sharing a
   *  repository down to a single pull and reports per-skill outcomes, so
   *  callers neither group by repo nor aggregate errors themselves. */
  const updateSkills = useCallback(
    async (names: string[]): Promise<SkillUpdateReport> => {
      const toUpdate = names.filter((name) => !pendingUpdateRef.current.has(name));
      if (toUpdate.length === 0) {
        return { updated: [], failed: [], skipped: [], channel_managed: [] };
      }

      for (const name of toUpdate) {
        pendingUpdateRef.current.add(name);
      }
      setPendingUpdateNames(new Set(pendingUpdateRef.current));

      try {
        const report = await tauriInvoke("update_skills", { names: toUpdate });

        // Siblings rode along on a pulled checkout; their content moved too.
        const movedWithTheirRepo = new Set(report.updated.flatMap((result) => result.siblings_cleared));

        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
          const refreshed = new Map(report.updated.map((result) => [result.skill.name, result.skill]));

          return prev.map((item) => {
            const fresh = refreshed.get(item.name);
            if (fresh) return fresh;
            if (movedWithTheirRepo.has(item.name)) {
              return { ...item, update_available: false };
            }
            return item;
          });
        });
        if (movedWithTheirRepo.size > 0) {
          await queryClient.refetchQueries({ queryKey: SKILLS_QUERY_KEY, exact: true });
        }

        // The updates succeeded but some agent deployments could not be
        // refreshed (e.g. symlink privileges revoked) — surface it so the user
        // knows which agent may be stale instead of failing silently.
        const relinkFailures = report.updated.flatMap((result) => result.agent_link_failures ?? []);
        if (relinkFailures.length > 0) {
          toast.warning(`${i18n.t("mySkills.agentRelinkFailed")}\n${relinkFailures.join("\n")}`);
        }

        void refetchUpdates();
        return report;
      } finally {
        for (const name of toUpdate) {
          pendingUpdateRef.current.delete(name);
        }
        setPendingUpdateNames(new Set(pendingUpdateRef.current));
      }
    },
    [queryClient, refetchUpdates],
  );

  /**
   * The complete update path: pull what can be pulled, then surface the
   * per-skill outcomes in toasts — failures with their reasons, and `skipped`
   * names whose upstream no longer ships them. Never throws; failures are in
   * the report.
   */
  const runSkillUpdate = useCallback(
    async (names: string[]): Promise<SkillUpdateReport> => {
      const report = await updateSkills(names);

      if (report.failed.length > 0) {
        const lines = report.failed.map((failure) => `${failure.name}: ${failure.error}`).join("\n");
        toast.error(`${i18n.t("mySkills.updateFailed")}\n${lines}`);
      }

      if (report.skipped.length > 0) {
        toast.info(i18n.t("mySkills.updateSkippedToast", { names: report.skipped.join(", ") }));
      }

      return report;
    },
    [updateSkills],
  );

  /** Single-skill convenience over {@link runSkillUpdate}. Every page uses this
   *  path, so the outcome is reported consistently. */
  const updateSkill = useCallback(
    async (name: string): Promise<Skill> => {
      const report = await runSkillUpdate([name]);
      const cached = () => queryClient.getQueryData<Skill[]>(SKILLS_QUERY_KEY)?.find((skill) => skill.name === name);

      const updated = report.updated.find((result) => result.skill.name === name);
      if (updated) return cached() ?? updated.skill;

      const failure = report.failed.find((entry) => entry.name === name) ?? report.failed[0];
      if (failure) {
        // runSkillUpdate already surfaced this failure in a toast.
        const error = new Error(failure.error);
        Object.assign(error, { skillstarToastShown: true });
        throw error;
      }

      // Nothing failed: the Skill rode along a sibling's pull, the same update
      // was already running, or a shared channel owns it (declined by design —
      // callers read that from the report themselves).
      const current = cached();
      if (current && !report.channel_managed.some((entry) => entry.name === name)) return current;

      const declined = new Error(i18n.t("mySkills.updateCancelled"));
      Object.assign(declined, { skillstarToastShown: true });
      throw declined;
    },
    [queryClient, runSkillUpdate],
  );

  const toggleSkillForAgent = useCallback(
    async (skillName: string, agentId: string, enable: boolean, agentName?: string) => {
      const toggleKey = `${skillName}::${agentId}`;
      if (pendingAgentToggleRef.current.has(toggleKey)) return;

      pendingAgentToggleRef.current.add(toggleKey);
      setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
      setIsTogglingAgent(true);

      // Cancel any in-flight skills refetch to prevent stale server data from
      // overwriting the optimistic update while the backend processes the toggle.
      // On Windows, junction removal can take 1-3s due to retry_io backoff;
      // a concurrent refetch completing in that window would revert the UI.
      await queryClient.cancelQueries({ queryKey: SKILLS_QUERY_KEY });

      const previousSnapshot = queryClient.getQueryData<Skill[]>(SKILLS_QUERY_KEY) ?? [];
      const previousSkillSnapshot = previousSnapshot.find((item) => item.name === skillName) ?? null;

      try {
        if (agentName) {
          queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
            prev.map((item) => {
              if (item.name !== skillName) return item;
              const links = item.agent_links ?? [];
              return {
                ...item,
                agent_links: enable ? [...new Set([...links, agentName])] : links.filter((link) => link !== agentName),
              };
            }),
          );
        }

        await tauriInvoke("toggle_skill_for_agent", { skillName, agentId, enable });
      } catch (e) {
        if (previousSkillSnapshot) {
          queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
            prev.map((item) =>
              item.name === skillName ? { ...item, agent_links: previousSkillSnapshot.agent_links } : item,
            ),
          );
        } else {
          await queryClient.invalidateQueries({
            queryKey: SKILLS_QUERY_KEY,
            exact: true,
          });
        }
        await refresh(true, true);
        throw new Error(String(e));
      } finally {
        pendingAgentToggleRef.current.delete(toggleKey);
        setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
        setIsTogglingAgent(pendingAgentToggleRef.current.size > 0);
      }
    },
    [queryClient, refresh],
  );

  const batchRemoveSkillsFromAllAgents = useCallback(
    async (skillNames: string[]) => {
      try {
        await tauriInvoke("batch_remove_skills_from_all_agents", { skillNames });
        await refresh(true, true);
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [refresh],
  );

  const readSkillContent = useCallback(async (name: string) => {
    try {
      return await tauriInvoke("read_skill_content", { name });
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const updateSkillContent = useCallback(async (name: string, content: string) => {
    try {
      await tauriInvoke("update_skill_content", { name, content });
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const deleteLocalSkill = useCallback(
    async (name: string) => {
      try {
        await tauriInvoke("delete_local_skill", { name });
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => prev.filter((item) => item.name !== name));
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [queryClient],
  );

  const loading = skillsQuery.isPending || (skillsQuery.isFetching && skills.length === 0);
  const error = refreshError ?? (skillsQuery.error ? String(skillsQuery.error) : null);

  return useMemo(
    () => ({
      skills,
      loading,
      error,
      pendingUpdateNames,
      refresh,
      installSkill,
      reinstallSkill,
      reinstallRepoSkills,
      uninstallSkill,
      updateSkill,
      updateSkills,
      runSkillUpdate,
      toggleSkillForAgent,
      batchRemoveSkillsFromAllAgents,
      pendingAgentToggleKeys,
      readSkillContent,
      updateSkillContent,
      deleteLocalSkill,
    }),
    [
      skills,
      loading,
      error,
      pendingUpdateNames,
      refresh,
      installSkill,
      reinstallSkill,
      reinstallRepoSkills,
      uninstallSkill,
      updateSkill,
      updateSkills,
      runSkillUpdate,
      toggleSkillForAgent,
      batchRemoveSkillsFromAllAgents,
      pendingAgentToggleKeys,
      readSkillContent,
      updateSkillContent,
      deleteLocalSkill,
    ],
  );
}

export function SkillsProvider({ children }: { children: ReactNode }) {
  const value = useSkillsState();
  return createElement(SkillsContext.Provider, { value }, children);
}

export function useSkills() {
  const context = useContext(SkillsContext);
  if (!context) {
    throw new Error("useSkills must be used within a SkillsProvider");
  }
  return context;
}

/** Sidebar chrome only needs one number; keep App off the full skills list.
 *  The amber count is "needs attention": content updates plus Skills their
 *  source removed — the same predicate as the toolbar's attention filter, so
 *  the badge never promises skills the filter cannot show. */
export function useSkillBadgeCounts() {
  const { skills } = useSkills();
  return {
    pendingUpdatesCount: skills.filter(needsAttention).length,
  };
}
