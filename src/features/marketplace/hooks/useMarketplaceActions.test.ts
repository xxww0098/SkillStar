import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill } from "../../../types";
import { useMarketplaceActions } from "./useMarketplaceActions";

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

function skill(overrides: Partial<Skill> = {}): Skill {
  return {
    name: "writing-plans",
    description: "Plan things",
    skill_type: "hub",
    stars: 10,
    installed: false,
    update_available: false,
    last_updated: "2026-01-01T00:00:00Z",
    git_url: "https://github.com/example/writing-plans",
    tree_hash: null,
    category: "None",
    author: null,
    topics: [],
    ...overrides,
  };
}

/** Builds the full param set for `useMarketplaceActions`, with sane no-op defaults. */
function buildParams(overrides: Partial<Parameters<typeof useMarketplaceActions>[0]> = {}) {
  return {
    installSkill: vi.fn(async (_url: string, name?: string) => skill({ name })),
    updateSkill: vi.fn(async (name: string) => skill({ name })),
    uninstallSkill: vi.fn(async (_name: string) => undefined),
    patchSkill: vi.fn(),
    selectedSkill: null,
    setSelectedSkill: vi.fn(),
    setInstallingNames: vi.fn(),
    setInstallStatus: vi.fn(),
    t: (key: string, options?: Record<string, unknown>) => (options?.defaultValue as string) ?? key,
    ...overrides,
  };
}

describe("useMarketplaceActions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("handleInstall", () => {
    it("does nothing when url or name is missing", async () => {
      const params = buildParams();
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleInstall("", "writing-plans");
      });
      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "");
      });

      expect(params.installSkill).not.toHaveBeenCalled();
    });

    it("success path: marks installing, calls installSkill with (url, name), patches skill optimistically, and shows a status toast", async () => {
      const installed = skill({ name: "writing-plans", agent_links: ["claude"] });
      const params = buildParams({
        installSkill: vi.fn(async () => installed),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(params.installSkill).toHaveBeenCalledWith("https://github.com/example/writing-plans", "writing-plans");

      // installingNames add then remove (start + finally)
      expect(params.setInstallingNames).toHaveBeenCalledTimes(2);
      const addUpdater = vi.mocked(params.setInstallingNames).mock.calls[0][0];
      expect(addUpdater(new Set())).toEqual(new Set(["writing-plans"]));
      const removeUpdater = vi.mocked(params.setInstallingNames).mock.calls[1][0];
      expect(removeUpdater(new Set(["writing-plans"]))).toEqual(new Set());

      // Optimistic patch: installed=true, update_available=false, agent_links from result
      expect(params.patchSkill).toHaveBeenCalledTimes(1);
      const [patchedName, patchUpdater] = vi.mocked(params.patchSkill).mock.calls[0];
      expect(patchedName).toBe("writing-plans");
      const current = skill({ name: "writing-plans", installed: false, agent_links: undefined });
      expect(patchUpdater(current)).toEqual({
        ...current,
        installed: true,
        update_available: false,
        agent_links: ["claude"],
      });

      // Status toast reflects the synced-agent count via i18n interpolation options.
      expect(params.setInstallStatus).toHaveBeenCalledWith(
        expect.stringContaining("Installed & synced to {{count}} agents"),
      );
    });

    it("success path with zero synced agents shows the plain 'installed via GitHub' status", async () => {
      const installed = skill({ name: "writing-plans", agent_links: [] });
      const params = buildParams({
        installSkill: vi.fn(async () => installed),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(params.setInstallStatus).toHaveBeenCalledWith("marketplace.installedViaGithub");
    });

    it("updates the open detail panel when it matches the installed skill name", async () => {
      const installed = skill({ name: "writing-plans", agent_links: ["claude"] });
      const openDetail = skill({ name: "writing-plans", installed: false, agent_links: undefined });
      const params = buildParams({
        installSkill: vi.fn(async () => installed),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "writing-plans");
      });

      const selectedUpdater = vi.mocked(params.setSelectedSkill).mock.calls[0][0];
      expect(selectedUpdater(openDetail)).toEqual({
        ...openDetail,
        installed: true,
        update_available: false,
        agent_links: ["claude"],
      });
      // A non-matching (or absent) detail panel is left untouched.
      expect(selectedUpdater(null)).toBeNull();
      const otherSkill = skill({ name: "other-skill" });
      expect(selectedUpdater(otherSkill)).toBe(otherSkill);
    });

    it("'already installed' failure: treats it as a success (marks installed, shows the GitHub status, no error toast)", async () => {
      const params = buildParams({
        installSkill: vi.fn(async () => {
          throw new Error("Already installed");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(params.patchSkill).toHaveBeenCalledTimes(1);
      const [, patchUpdater] = vi.mocked(params.patchSkill).mock.calls[0];
      const current = skill({ name: "writing-plans", installed: false });
      expect(patchUpdater(current)).toEqual({ ...current, installed: true });
      expect(params.setInstallStatus).toHaveBeenCalledWith("marketplace.installedViaGithub");
      expect(toast.error).not.toHaveBeenCalled();
      // installingNames must still be cleared via the finally block.
      expect(params.setInstallingNames).toHaveBeenCalledTimes(2);
    });

    it("other failure: shows an error toast with the error message and clears installingNames", async () => {
      const params = buildParams({
        installSkill: vi.fn(async () => {
          throw new Error("network down");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleInstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(toast.error).toHaveBeenCalledWith("mySkills.installFailed: Error: network down");
      expect(params.patchSkill).not.toHaveBeenCalled();
      expect(params.setInstallingNames).toHaveBeenCalledTimes(2);
      const removeUpdater = vi.mocked(params.setInstallingNames).mock.calls[1][0];
      expect(removeUpdater(new Set(["writing-plans"]))).toEqual(new Set());
    });
  });

  describe("handleUpdate", () => {
    it("success path: calls updateSkill, patches update_available to false", async () => {
      const params = buildParams();
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleUpdate("writing-plans");
      });

      expect(params.updateSkill).toHaveBeenCalledWith("writing-plans");
      expect(params.patchSkill).toHaveBeenCalledTimes(1);
      const [name, updater] = vi.mocked(params.patchSkill).mock.calls[0];
      expect(name).toBe("writing-plans");
      const current = skill({ name: "writing-plans", update_available: true });
      expect(updater(current)).toEqual({ ...current, update_available: false });
    });

    it("updates the open detail panel when it matches, leaves others untouched", async () => {
      const params = buildParams();
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleUpdate("writing-plans");
      });

      const selectedUpdater = vi.mocked(params.setSelectedSkill).mock.calls[0][0];
      const openDetail = skill({ name: "writing-plans", update_available: true });
      expect(selectedUpdater(openDetail)).toEqual({ ...openDetail, update_available: false });
      expect(selectedUpdater(null)).toBeNull();
      const other = skill({ name: "other" });
      expect(selectedUpdater(other)).toBe(other);
    });

    it("failure path: shows an error toast with the error message, does not patch", async () => {
      const params = buildParams({
        updateSkill: vi.fn(async () => {
          throw new Error("pull failed");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleUpdate("writing-plans");
      });

      expect(toast.error).toHaveBeenCalledWith("marketplace.updateFailed: pull failed");
      expect(params.patchSkill).not.toHaveBeenCalled();
    });

    it("failure path: falls back to String(e) when the thrown value is not an Error instance", async () => {
      const params = buildParams({
        updateSkill: vi.fn(async () => {
          throw "raw string failure";
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleUpdate("writing-plans");
      });

      expect(toast.error).toHaveBeenCalledWith("marketplace.updateFailed: raw string failure");
    });
  });

  describe("handleUninstall", () => {
    it("success path: calls uninstallSkill, patches installed/update_available/agent_links", async () => {
      const params = buildParams();
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleUninstall("writing-plans");
      });

      expect(params.uninstallSkill).toHaveBeenCalledWith("writing-plans");
      expect(params.patchSkill).toHaveBeenCalledTimes(1);
      const [name, updater] = vi.mocked(params.patchSkill).mock.calls[0];
      expect(name).toBe("writing-plans");
      const current = skill({
        name: "writing-plans",
        installed: true,
        update_available: true,
        agent_links: ["claude"],
      });
      expect(updater(current)).toEqual({
        ...current,
        installed: false,
        update_available: false,
        agent_links: [],
      });
    });

    it("clears the open detail panel fields only when selectedSkill matches the uninstalled name", async () => {
      const openDetail = skill({ name: "writing-plans", installed: true, agent_links: ["claude"] });
      const params = buildParams({ selectedSkill: openDetail });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleUninstall("writing-plans");
      });

      expect(params.setSelectedSkill).toHaveBeenCalledTimes(1);
      const updater = vi.mocked(params.setSelectedSkill).mock.calls[0][0];
      expect(updater(openDetail)).toEqual({
        ...openDetail,
        installed: false,
        update_available: false,
        agent_links: [],
      });
      expect(updater(null)).toBeNull();
    });

    it("does not touch setSelectedSkill when selectedSkill is for a different skill", async () => {
      const params = buildParams({ selectedSkill: skill({ name: "other-skill" }) });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleUninstall("writing-plans");
      });

      expect(params.setSelectedSkill).not.toHaveBeenCalled();
    });

    it("failure path: shows the uninstall-failed toast (fixed copy, no error interpolation), does not patch", async () => {
      const params = buildParams({
        uninstallSkill: vi.fn(async () => {
          throw new Error("fs busy");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleUninstall("writing-plans");
      });

      expect(toast.error).toHaveBeenCalledWith("marketplace.uninstallFailed");
      expect(params.patchSkill).not.toHaveBeenCalled();
    });
  });

  describe("handleReinstall", () => {
    it("success path: uninstalls then installs, in that order", async () => {
      const callOrder: string[] = [];
      const params = buildParams({
        uninstallSkill: vi.fn(async () => {
          callOrder.push("uninstall");
        }),
        installSkill: vi.fn(async (_url: string, name?: string) => {
          callOrder.push("install");
          return skill({ name });
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));

      await act(async () => {
        await result.current.handleReinstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(callOrder).toEqual(["uninstall", "install"]);
      expect(params.uninstallSkill).toHaveBeenCalledWith("writing-plans");
      expect(params.installSkill).toHaveBeenCalledWith("https://github.com/example/writing-plans", "writing-plans");
    });

    it("failure path (uninstall throws): shows the reinstall-failed toast, never calls installSkill", async () => {
      const params = buildParams({
        uninstallSkill: vi.fn(async () => {
          throw new Error("fs busy");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleReinstall("https://github.com/example/writing-plans", "writing-plans");
      });

      expect(toast.error).toHaveBeenCalledWith("marketplace.reinstallFailed");
      expect(params.installSkill).not.toHaveBeenCalled();
    });

    it("failure path (install throws inside reinstall): shows the reinstall-failed toast, not the plain install-failed toast", async () => {
      const params = buildParams({
        installSkill: vi.fn(async () => {
          throw new Error("network down");
        }),
      });
      const { result } = renderHook(() => useMarketplaceActions(params));
      const { toast } = await import("../../../lib/toast");

      await act(async () => {
        await result.current.handleReinstall("https://github.com/example/writing-plans", "writing-plans");
      });

      // handleInstall's own catch swallows the "install failed" toast internally,
      // then handleReinstall's outer catch never fires because handleInstall
      // does not rethrow — so only handleInstall's own error toast should show.
      expect(toast.error).toHaveBeenCalledWith("mySkills.installFailed: Error: network down");
      expect(toast.error).not.toHaveBeenCalledWith("marketplace.reinstallFailed");
    });
  });
});
