import { useCallback } from "react";
import { toast } from "../../../lib/toast";
import type { Skill } from "../../../types";

interface UseMarketplaceActionsParams {
  installSkill: (url: string, name?: string, agentId?: string) => Promise<Skill>;
  updateSkill: (name: string) => Promise<Skill>;
  uninstallSkill: (name: string) => Promise<unknown>;
  /** Page-owned: applies an optimistic patch to a skill across all marketplace state slices. */
  patchSkill: (name: string, updater: (skill: Skill) => Skill) => void;
  /** Page-owned: the currently open detail-panel skill, if any. */
  selectedSkill: Skill | null;
  setSelectedSkill: (updater: (prev: Skill | null) => Skill | null) => void;
  setInstallingNames: (updater: (prev: Set<string>) => Set<string>) => void;
  setInstallStatus: (status: string | null) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

/**
 * The four optimistic-update handlers for marketplace skill actions:
 * install, update, uninstall, and reinstall (uninstall + install).
 *
 * Extracted verbatim from `Marketplace.tsx`'s `handleInstall` / `handleUpdate` /
 * `handleUninstall` / `handleReinstall` callbacks — see that file's git
 * history for the original inline versions.
 */
export function useMarketplaceActions({
  installSkill,
  updateSkill,
  uninstallSkill,
  patchSkill,
  selectedSkill,
  setSelectedSkill,
  setInstallingNames,
  setInstallStatus,
  t,
}: UseMarketplaceActionsParams) {
  const handleInstall = useCallback(
    async (url: string, name: string, agentId?: string) => {
      if (!url || !name) return;

      setInstallingNames((prev) => {
        const next = new Set(prev);
        next.add(name);
        return next;
      });

      try {
        const skill = await installSkill(url, name, agentId);
        patchSkill(name, (current) => ({
          ...current,
          installed: true,
          update_available: false,
          agent_links: skill.agent_links ?? current.agent_links,
        }));
        setSelectedSkill((prev) => {
          if (!prev) return prev;
          if (prev.name === name) {
            return {
              ...prev,
              installed: true,
              update_available: false,
              agent_links: skill.agent_links ?? prev.agent_links,
            };
          }
          return prev;
        });

        const agentCount = skill.agent_links?.length ?? 0;
        setInstallStatus(
          agentCount > 0
            ? t("marketplace.installedSynced", {
                count: agentCount,
                defaultValue: "✓ Installed & synced to {{count}} agents",
              })
            : t("marketplace.installedViaGithub"),
        );
        setTimeout(() => setInstallStatus(null), 4000);
      } catch (e) {
        const message = String(e).toLowerCase();
        if (message.includes("already installed")) {
          patchSkill(name, (current) => ({ ...current, installed: true }));
          setSelectedSkill((prev) => (prev?.name === name ? { ...prev, installed: true } : prev));
          setInstallStatus(t("marketplace.installedViaGithub"));
          setTimeout(() => setInstallStatus(null), 4000);
          return;
        }
        if (import.meta.env.DEV) console.error("[Marketplace] Install failed:", e);
        toast.error(String(e) ? `${t("mySkills.installFailed")}: ${String(e)}` : t("mySkills.installFailed"));
      } finally {
        setInstallingNames((prev) => {
          const next = new Set(prev);
          next.delete(name);
          return next;
        });
      }
    },
    [installSkill, patchSkill, setInstallingNames, setInstallStatus, setSelectedSkill, t],
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
        patchSkill(name, (current) => ({
          ...current,
          update_available: false,
        }));
        setSelectedSkill((prev) => (prev?.name === name ? { ...prev, update_available: false } : prev));
      } catch (e) {
        if (import.meta.env.DEV) console.error("Update failed:", e);
        const reason = e instanceof Error ? e.message : String(e);
        const alreadyShown =
          e instanceof Error && (e as Error & { skillstarToastShown?: boolean }).skillstarToastShown === true;
        if (!alreadyShown) {
          toast.error(reason ? `${t("marketplace.updateFailed")}: ${reason}` : t("marketplace.updateFailed"));
        }
      }
    },
    [patchSkill, setSelectedSkill, t, updateSkill],
  );

  const handleUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        patchSkill(name, (current) => ({
          ...current,
          installed: false,
          update_available: false,
          agent_links: [],
        }));
        if (selectedSkill?.name === name) {
          setSelectedSkill((prev) =>
            prev
              ? {
                  ...prev,
                  installed: false,
                  update_available: false,
                  agent_links: [],
                }
              : null,
          );
        }
      } catch (e) {
        if (import.meta.env.DEV) console.error("[Marketplace] Uninstall failed:", e);
        toast.error(t("marketplace.uninstallFailed"));
      }
    },
    [patchSkill, uninstallSkill, selectedSkill, setSelectedSkill, t],
  );

  const handleReinstall = useCallback(
    async (url: string, name: string) => {
      try {
        await uninstallSkill(name);
        await handleInstall(url, name);
      } catch (e) {
        if (import.meta.env.DEV) console.error("[Marketplace] Reinstall failed:", e);
        toast.error(t("marketplace.reinstallFailed"));
      }
    },
    [uninstallSkill, handleInstall, t],
  );

  return { handleInstall, handleUpdate, handleUninstall, handleReinstall };
}
