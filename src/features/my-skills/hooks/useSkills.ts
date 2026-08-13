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
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import type {
  LocalDivergenceResolution,
  RepoNewSkill,
  Skill,
  SkillUpdateReport,
  SkillUpdateRunReport,
  SkillUpdateState,
} from "../../../types";
import i18n from "../../../i18n";
import { useLocalDivergenceResolver } from "./useLocalDivergenceResolver";

const SKILLS_QUERY_KEY = ["skills"] as const;
const SKILL_UPDATES_QUERY_KEY = ["skills", "updates"] as const;
const GHOST_SKILLS_QUERY_KEY = ["skills", "ghost"] as const;
const SKILL_LIST_REFRESH_INTERVAL_MS = 30_000;
const SKILL_UPDATE_REFRESH_FOREGROUND_MS = 5 * 60 * 1000;
const SKILL_UPDATE_REFRESH_BACKGROUND_MS = 15 * 60 * 1000;

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

      const updatesByName = new Map(updates.map((update) => [update.name, update.update_available]));

      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.length === 0) return prev;

        // Applied as-is: the backend resolves staleness before answering. A
        // scan that began before an update landed loses to it there, so a
        // response can no longer re-assert a badge the update just cleared.
        let changed = false;
        const next = prev.map((skill) => {
          const updateAvailable = updatesByName.get(skill.name);
          if (updateAvailable === undefined || updateAvailable === skill.update_available) {
            return skill;
          }
          changed = true;
          return { ...skill, update_available: updateAvailable };
        });

        return changed ? next : prev;
      });
    },
    [queryClient],
  );

  const skillsQuery = useQuery({
    queryKey: SKILLS_QUERY_KEY,
    queryFn: listSkills,
    refetchOnWindowFocus: true,
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

        await refetchSkills();
        await refetchUpdates();
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
  useTauriEvent<{ name: string; update_available: boolean }>("patrol://skill-checked", ({ name, update_available }) => {
    queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
      const skill = prev.find((item) => item.name === name);
      if (!skill || skill.update_available === update_available) return prev;
      return prev.map((item) => (item.name === name ? { ...item, update_available } : item));
    });
  });

  // ── Ghost Skills (new repo skills) ────────────────────────────────

  const ghostQuery = useQuery({
    queryKey: GHOST_SKILLS_QUERY_KEY,
    queryFn: () => tauriInvoke("check_new_repo_skills"),
    enabled: skills.length > 0,
    refetchOnWindowFocus: false,
    staleTime: 60_000,
  });

  const ghostSkills = ghostQuery.data ?? [];

  // Listen for patrol event to update ghost skills
  useTauriEvent<RepoNewSkill[]>("patrol://new-skills-detected", (payload) => {
    if (payload.length === 0) return;
    // Merge with dismissed filter: fetch dismissed list and filter
    tauriInvoke("get_dismissed_new_skills")
      .then((dismissed) => {
        const dismissedSet = new Set(dismissed);
        const filtered = payload.filter((s) => !dismissedSet.has(`${s.repo_source}/${s.skill_id}`));
        queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, filtered);
      })
      .catch(() => {
        // Fallback: set all (dismissed filter will apply on next full fetch)
        queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, payload);
      });
  });

  const dismissGhostSkill = useCallback(
    async (repoSource: string, skillId: string) => {
      const key = `${repoSource}/${skillId}`;
      // Optimistic: remove immediately from cache
      queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, (prev = []) =>
        prev.filter((s) => `${s.repo_source}/${s.skill_id}` !== key),
      );
      try {
        await tauriInvoke("dismiss_new_skill", { key });
      } catch (e) {
        // Revert on failure: re-fetch
        void ghostQuery.refetch();
        throw new Error(String(e));
      }
    },
    [queryClient, ghostQuery],
  );

  const dismissGhostRepo = useCallback(
    async (repoSource: string) => {
      // Get all keys for this repo from current ghost list
      const currentGhosts = queryClient.getQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY) ?? [];
      const keys = currentGhosts
        .filter((s) => s.repo_source === repoSource)
        .map((s) => `${s.repo_source}/${s.skill_id}`);
      if (keys.length === 0) return;
      // Optimistic: remove all from cache
      queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, (prev = []) =>
        prev.filter((s) => s.repo_source !== repoSource),
      );
      try {
        await tauriInvoke("dismiss_new_skills_batch", { keys });
      } catch (e) {
        void ghostQuery.refetch();
        throw new Error(String(e));
      }
    },
    [queryClient, ghostQuery],
  );

  const installMutation = useMutation({
    mutationFn: ({ url, name }: { url: string; name?: string }) => tauriInvoke("install_skill", { url, name }),
    onSuccess: (skill) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.some((item) => item.name === skill.name)) {
          return prev.map((item) => (item.name === skill.name ? skill : item));
        }
        return [...prev, skill];
      });
      void refetchUpdates();
    },
  });

  const installGhostSkill = useCallback(
    async (skill: RepoNewSkill) => {
      try {
        const installed = await installMutation.mutateAsync({ url: skill.repo_url, name: skill.skill_id });
        // Remove from ghost list after successful install
        queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, (prev = []) =>
          prev.filter((s) => s.skill_id !== skill.skill_id || s.repo_source !== skill.repo_source),
        );
        return installed;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [installMutation, queryClient],
  );

  const uninstallMutation = useMutation({
    mutationFn: (name: string) => tauriInvoke("uninstall_skill", { name }),
    onSuccess: (_result, name) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => prev.filter((item) => item.name !== name));
    },
  });

  const installSkill = useCallback(
    async (url: string, name?: string) => {
      try {
        return await installMutation.mutateAsync({ url, name });
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [installMutation],
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
        repoUrl: scan.source_url,
        source: scan.source,
        skills: scan.skills.map((skill) => ({
          id: skill.id,
          folder_path: skill.folder_path,
        })),
      });

      queryClient.setQueryData<RepoNewSkill[]>(GHOST_SKILLS_QUERY_KEY, (prev = []) =>
        prev.filter((skill) => skill.repo_source !== scan.source || !installed.includes(skill.skill_id)),
      );
      await refresh(false, true);
      return installed;
    },
    [queryClient, refresh],
  );

  const uninstallSkill = useCallback(
    async (name: string) => {
      try {
        await uninstallMutation.mutateAsync(name);
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [uninstallMutation],
  );

  /** The one way to update skills. The backend collapses names sharing a
   *  repository down to a single pull and reports per-skill outcomes, so
   *  callers neither group by repo nor aggregate errors themselves. */
  const updateSkills = useCallback(
    async (names: string[]): Promise<SkillUpdateReport> => {
      const toUpdate = names.filter((name) => !pendingUpdateRef.current.has(name));
      if (toUpdate.length === 0) {
        return { updated: [], blocked: [], failed: [], skipped: [], channel_managed: [] };
      }

      for (const name of toUpdate) {
        pendingUpdateRef.current.add(name);
      }
      setPendingUpdateNames(new Set(pendingUpdateRef.current));

      try {
        const report = await tauriInvoke("update_skills", { names: toUpdate });

        // Skipped names rode along on a sibling's pull; siblings_cleared are
        // installed skills from the same repo that were not even requested.
        const movedWithTheirRepo = new Set([
          ...report.skipped,
          ...report.updated.flatMap((result) => result.siblings_cleared),
        ]);

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

  const resolveSkillUpdate = useCallback(
    async (name: string, resolution: LocalDivergenceResolution) => {
      if (pendingUpdateRef.current.has(name)) {
        throw new Error(i18n.t("mySkills.updateInProgress"));
      }
      pendingUpdateRef.current.add(name);
      setPendingUpdateNames(new Set(pendingUpdateRef.current));

      try {
        const result = await tauriInvoke("resolve_skill_update", { name, resolution });
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
          const movedWithRepo = new Set(result.update?.siblings_cleared ?? []);
          // A Skill its source dropped is removed, not refreshed — it must
          // leave the library rather than linger as a broken card.
          const removed = new Set(result.uninstalled);
          const next = prev
            .filter((item) => !removed.has(item.name))
            .map((item) => {
              if (item.name === result.update?.skill.name) return result.update.skill;
              if (movedWithRepo.has(item.name)) return { ...item, update_available: false };
              return item;
            });
          if (result.local_copy && !next.some((item) => item.name === result.local_copy?.name)) {
            next.push(result.local_copy);
          }
          return next;
        });

        if (result.update && result.update.agent_link_failures.length > 0) {
          toast.warning(`${i18n.t("mySkills.agentRelinkFailed")}\n${result.update.agent_link_failures.join("\n")}`);
        }
        if (result.update) {
          // A blocked sibling can be the representative that finally pulls a
          // shared checkout. Refresh the full Skill objects so the originally
          // requested sibling does not retain stale description/tree data.
          if (result.update.siblings_cleared.length > 0) {
            await queryClient.refetchQueries({ queryKey: SKILLS_QUERY_KEY, exact: true });
          }
          void refetchUpdates();
        }
        return result;
      } finally {
        pendingUpdateRef.current.delete(name);
        setPendingUpdateNames(new Set(pendingUpdateRef.current));
      }
    },
    [queryClient, refetchUpdates],
  );

  const takenSkillNames = useMemo(() => skills.map((skill) => skill.name), [skills]);
  const { resolveBlocked, dialogElement: localDivergenceDialog } = useLocalDivergenceResolver({
    resolveSkillUpdate,
    takenNames: takenSkillNames,
  });

  /**
   * The complete update path: pull what can be pulled, then take every Skill
   * the backend blocked through the divergence dialog. The returned report is
   * the *final* one — resolved Skills have moved from `blocked` to `updated` —
   * so callers summarise a single outcome instead of re-implementing the
   * resolution loop. Never throws; failures are in the report.
   */
  const runSkillUpdate = useCallback(
    async (names: string[]): Promise<SkillUpdateRunReport> => {
      const report = await updateSkills(names);
      if (report.blocked.length === 0) return { ...report, uninstalled: [] };

      const outcome = await resolveBlocked(report.blocked);
      return {
        updated: [...report.updated, ...outcome.updated],
        blocked: outcome.unresolved,
        failed: [...report.failed, ...outcome.failed],
        skipped: report.skipped,
        channel_managed: report.channel_managed,
        uninstalled: outcome.uninstalled,
      };
    },
    [resolveBlocked, updateSkills],
  );

  /** Single-skill convenience over {@link runSkillUpdate}. Every page uses this
   *  path, so a divergent subscription always gets the same explicit choice. */
  const updateSkill = useCallback(
    async (name: string): Promise<Skill> => {
      const report = await runSkillUpdate([name]);
      const cached = () => queryClient.getQueryData<Skill[]>(SKILLS_QUERY_KEY)?.find((skill) => skill.name === name);

      const updated = report.updated.find((result) => result.skill.name === name);
      if (updated) return cached() ?? updated.skill;

      // Resolving the stop removed it: there is no Skill left to hand back, and
      // that is the outcome the user asked for, not a failure.
      if (report.uninstalled.includes(name)) {
        toast.success(i18n.t("mySkills.droppedSkillRemoved", { name }));
        const removed = new Error(i18n.t("mySkills.droppedSkillRemoved", { name }));
        Object.assign(removed, { skillstarToastShown: true });
        throw removed;
      }

      const failure = report.failed.find((entry) => entry.name === name) ?? report.failed[0];
      if (failure) {
        toast.error(failure.error);
        const error = new Error(failure.error);
        Object.assign(error, { skillstarToastShown: true });
        throw error;
      }

      // Nothing failed and nothing is still blocked: the Skill rode along a
      // sibling's pull, or the same update was already running.
      const current = cached();
      if (report.blocked.length === 0 && current) return current;

      // Backing out of the dialog is a decision, not an error — no toast.
      const cancelled = new Error(i18n.t("mySkills.updateCancelled"));
      Object.assign(cancelled, { skillstarToastShown: true });
      throw cancelled;
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
      reinstallRepoSkills,
      uninstallSkill,
      updateSkill,
      updateSkills,
      runSkillUpdate,
      resolveSkillUpdate,
      toggleSkillForAgent,
      batchRemoveSkillsFromAllAgents,
      pendingAgentToggleKeys,
      readSkillContent,
      updateSkillContent,
      deleteLocalSkill,
      ghostSkills,
      dismissGhostSkill,
      dismissGhostRepo,
      installGhostSkill,
      localDivergenceDialog,
    }),
    [
      skills,
      loading,
      error,
      pendingUpdateNames,
      refresh,
      installSkill,
      reinstallRepoSkills,
      uninstallSkill,
      updateSkill,
      updateSkills,
      runSkillUpdate,
      resolveSkillUpdate,
      toggleSkillForAgent,
      batchRemoveSkillsFromAllAgents,
      pendingAgentToggleKeys,
      readSkillContent,
      updateSkillContent,
      deleteLocalSkill,
      ghostSkills,
      dismissGhostSkill,
      dismissGhostRepo,
      installGhostSkill,
      localDivergenceDialog,
    ],
  );
}

export function SkillsProvider({ children }: { children: ReactNode }) {
  const value = useSkillsState();
  return createElement(SkillsContext.Provider, { value }, children, value.localDivergenceDialog);
}

export function useSkills() {
  const context = useContext(SkillsContext);
  if (!context) {
    throw new Error("useSkills must be used within a SkillsProvider");
  }
  return context;
}
