import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { tauriInvoke } from "../../../lib/ipc";
import type { Skill, SkillCardDeck } from "../../../types";
import { normalizeSkillSources, uniqueNormalizedSkillNames } from "../lib/skillNames";

// ── Module-level install progress store ─────────────────────────────
// Survives component unmount/remount so switching pages doesn't lose the
// active install state. Each entry maps groupId → progress.
interface InstallProgressEntry {
  done: number;
  total: number;
  abortController?: AbortController;
}
const activeInstalls = new Map<string, InstallProgressEntry>();
const installListeners = new Set<() => void>();
function notifyInstallListeners() {
  for (const fn of installListeners) fn();
}

// Clean up module-level state during HMR to prevent stale data pollution.
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    activeInstalls.clear();
    installListeners.clear();
  });
}

interface UseDeckInstallProgressParams {
  /** Installed-skill lookup (normalized name → skill), from the hub skills list. */
  skillByName: Map<string, Skill>;
  installSkill: (url: string, name?: string) => Promise<unknown>;
  updateGroup: (
    id: string,
    patch: {
      name?: string;
      description?: string;
      icon?: string;
      skills?: string[];
      skillSources?: Record<string, string>;
    },
  ) => Promise<unknown>;
}

export interface DeckInstallProgress {
  /** Group id currently installing its missing skills, or null. */
  installingMissing: string | null;
  installProgress: { done: number; total: number } | null;
  /** Union of backend-installed names and the hub skills snapshot. */
  installedNameSet: Set<string>;
  /** Install every missing skill in a deck (concurrent, source-resolving). */
  handleInstallMissing: (group: SkillCardDeck) => Promise<void>;
}

/**
 * Owns the "install missing deck skills" workflow: a module-level progress
 * store (so the live state survives page remounts), backend-installed snapshot
 * tracking, and the concurrent install-with-source-resolution routine.
 */
export function useDeckInstallProgress({
  skillByName,
  installSkill,
  updateGroup,
}: UseDeckInstallProgressParams): DeckInstallProgress {
  const { t } = useTranslation();
  const [backendInstalledNames, setBackendInstalledNames] = useState<Set<string>>(new Set());
  // Track whether the install handler is owned by this mount.
  const installOwnerRef = useRef(false);
  const [installingMissing, setInstallingMissing] = useState<string | null>(
    // Restore from module-level store on mount.
    () => {
      for (const id of activeInstalls.keys()) return id;
      return null;
    },
  );
  const [installProgress, setInstallProgress] = useState<{ done: number; total: number } | null>(() => {
    for (const [, entry] of activeInstalls) return { done: entry.done, total: entry.total };
    return null;
  });

  const installedNameSet = useMemo(() => {
    const next = new Set<string>(backendInstalledNames);
    for (const name of skillByName.keys()) {
      next.add(name);
    }
    return next;
  }, [backendInstalledNames, skillByName]);

  const refreshBackendInstalledNames = useCallback(async () => {
    try {
      const latest = await tauriInvoke("list_skills");
      const next = new Set(latest.map((skill) => skill.name.trim()).filter(Boolean));
      setBackendInstalledNames(next);
      return next;
    } catch (e) {
      if (import.meta.env.DEV) console.error("Failed to refresh installed skills snapshot:", e);
      return null;
    }
  }, []);

  useEffect(() => {
    void refreshBackendInstalledNames();
  }, [refreshBackendInstalledNames]);

  // Subscribe to module-level install progress changes so remounts pick up live state.
  useEffect(() => {
    const listener = () => {
      const entry = Array.from(activeInstalls.entries())[0];
      if (entry) {
        setInstallingMissing(entry[0]);
        setInstallProgress({ done: entry[1].done, total: entry[1].total });
      } else {
        setInstallingMissing(null);
        setInstallProgress(null);
      }
    };
    installListeners.add(listener);
    return () => {
      installListeners.delete(listener);
    };
  }, []);

  const handleInstallMissing = async (group: SkillCardDeck) => {
    if (installingMissing || activeInstalls.has(group.id)) return;
    const groupSkillNames = uniqueNormalizedSkillNames(group.skills);
    if (groupSkillNames.length === 0) return;

    const refreshedInstalled = await refreshBackendInstalledNames();
    const installedSnapshot = refreshedInstalled ?? installedNameSet;
    const missing = groupSkillNames.filter((name) => !installedSnapshot.has(name));
    if (missing.length === 0) return;

    const nextSources = normalizeSkillSources(group.skill_sources);

    // Identify names that have no known source.
    const namesNeedingSource = missing.filter((name) => !nextSources[name]);

    // Batch-resolve missing sources via backend marketplace search.
    if (namesNeedingSource.length > 0) {
      try {
        const resolved = await tauriInvoke("resolve_skill_sources", {
          names: namesNeedingSource,
          existingSources: nextSources,
        });
        for (const [name, url] of Object.entries(resolved)) {
          if (url) nextSources[name] = url;
        }
      } catch (e) {
        if (import.meta.env.DEV) console.error("[SkillCards] resolve_skill_sources failed:", e);
      }
    }

    const installQueue: Array<{ name: string; url: string }> = [];
    const noSourceNames: string[] = [];
    for (const name of missing) {
      const url = nextSources[name];
      if (url) {
        installQueue.push({ name, url });
      } else {
        noSourceNames.push(name);
      }
    }

    if (installQueue.length === 0) {
      toast.error(
        t("skillCards.installNoSource", {
          defaultValue: "No install source found for missing skills",
        }),
      );
      return;
    }

    // Persist resolved sources back to the group.
    const sourcesChanged = namesNeedingSource.some((name) => !!nextSources[name]);

    // Register in module-level store.
    const progressEntry: InstallProgressEntry = { done: 0, total: installQueue.length };
    activeInstalls.set(group.id, progressEntry);
    setInstallingMissing(group.id);
    setInstallProgress({ done: 0, total: installQueue.length });
    installOwnerRef.current = true;
    notifyInstallListeners();

    let successCount = 0;
    const failedNames: string[] = [];

    // Concurrent install with bounded parallelism (3 at a time).
    const CONCURRENCY = 3;
    let cursor = 0;
    const runNext = async (): Promise<void> => {
      while (cursor < installQueue.length) {
        const idx = cursor++;
        const item = installQueue[idx];
        try {
          await installSkill(item.url, item.name);
          successCount++;
        } catch (e) {
          if (import.meta.env.DEV) console.error(`Failed to install ${item.name}:`, e);
          failedNames.push(item.name);
        }
        // Update progress.
        progressEntry.done++;
        activeInstalls.set(group.id, { ...progressEntry });
        // Only update local state if this mount owns the install.
        if (installOwnerRef.current) {
          setInstallProgress({ done: progressEntry.done, total: progressEntry.total });
        }
        notifyInstallListeners();
      }
    };

    try {
      await Promise.all(Array.from({ length: Math.min(CONCURRENCY, installQueue.length) }, () => runNext()));

      if (sourcesChanged) {
        await updateGroup(group.id, { skillSources: nextSources });
      }
      // Summary toast.
      if (successCount > 0 && failedNames.length === 0 && noSourceNames.length === 0) {
        toast.success(
          t("skillCards.installAllSuccess", {
            count: successCount,
            defaultValue: `Successfully installed ${successCount} skill(s)`,
          }),
        );
      } else if (successCount > 0) {
        toast.warning(
          t("skillCards.installPartial", {
            success: successCount,
            failed: failedNames.length + noSourceNames.length,
            defaultValue: `Installed ${successCount}, failed ${failedNames.length + noSourceNames.length}`,
          }),
        );
      } else {
        toast.error(
          t("skillCards.installAllFailed", {
            defaultValue: "Failed to install skills",
          }),
        );
      }
    } finally {
      activeInstalls.delete(group.id);
      installOwnerRef.current = false;
      setInstallingMissing(null);
      setInstallProgress(null);
      notifyInstallListeners();
      void refreshBackendInstalledNames();
      window.dispatchEvent(new Event("skillstar:refresh-skills"));
    }
  };

  return { installingMissing, installProgress, installedNameSet, handleInstallMissing };
}
