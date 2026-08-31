import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentProfile } from "../../../types";
import { AgentConnectionsSection } from "./AgentConnectionsSection";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === "settings.toggleAgent") return `Toggle ${String(options?.name)}`;
      if (key === "settings.toggleManagedSkills") return `Managed skills ${String(options?.name)}`;
      if (key === "settings.activeCount") {
        return `${String(options?.enabled)} / ${String(options?.total)} active`;
      }
      if (key === "settings.showRemainingAgents") return `Show ${String(options?.count)} more Agents`;
      if (key === "settings.collapseAgentList") return "Show fewer Agents";
      if (key === "settings.managedSkillsActiveCount") return `${String(options?.count)} managed active`;
      if (key === "settings.managedSkillsPausedCount") return `${String(options?.count)} paused skills`;
      if (key === "settings.managedSkillsPartialCount") {
        return `${String(options?.paused)} paused / ${String(options?.active)} active`;
      }
      if (key === "settings.noManagedSkills") return "No managed skills";
      if (key === "settings.sharedSkillsTarget") return `Shared with ${String(options?.names)}`;
      return key;
    },
  }),
}));

const ICON = "data:image/svg+xml;base64,PHN2Zy8+";

function profile(id: string, installed: boolean, enabled: boolean): AgentProfile {
  return {
    id,
    display_name: id,
    icon: ICON,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed,
    enabled,
    synced_count: 0,
  };
}

function activationSwitches() {
  return screen.getAllByRole("switch").filter((toggle) => toggle.getAttribute("aria-label")?.startsWith("Toggle "));
}

describe("AgentConnectionsSection", () => {
  it("renders a manual switch for every registered Agent and ignores the legacy installed flag", () => {
    const onToggleProfile = vi.fn();
    const enabledWithoutInstallSignal = profile("enabled", false, true);
    const disabledWithInstallSignal = profile("disabled", true, false);
    const disabledWithoutInstallSignal = profile("inactive", false, false);

    render(
      <AgentConnectionsSection
        profiles={[enabledWithoutInstallSignal, disabledWithInstallSignal, disabledWithoutInstallSignal]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={onToggleProfile}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(screen.getByText("1 / 3 active")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Toggle enabled" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "Toggle disabled" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Toggle inactive" })).not.toBeChecked();

    fireEvent.click(screen.getByRole("switch", { name: "Toggle inactive" }));
    expect(onToggleProfile).toHaveBeenCalledWith(disabledWithoutInstallSignal);
  });

  it("moves enabled Agents ahead while preserving registry order within each group", () => {
    render(
      <AgentConnectionsSection
        profiles={[
          profile("disabled-first", false, false),
          profile("enabled-first", false, true),
          profile("disabled-second", false, false),
          profile("enabled-second", false, true),
        ]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(activationSwitches().map((toggle) => toggle.getAttribute("aria-label"))).toEqual([
      "Toggle enabled-first",
      "Toggle enabled-second",
      "Toggle disabled-first",
      "Toggle disabled-second",
    ]);
  });

  it("shows ten Agents by default and folds the remainder behind an explicit control", () => {
    const agents = Array.from({ length: 12 }, (_, index) => profile(`agent-${index + 1}`, false, false));

    render(
      <AgentConnectionsSection
        profiles={agents}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(activationSwitches()).toHaveLength(10);
    expect(screen.queryByRole("switch", { name: "Toggle agent-11" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show 2 more Agents" }));
    expect(activationSwitches()).toHaveLength(12);
    expect(screen.getByRole("switch", { name: "Toggle agent-11" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show fewer Agents" }));
    expect(activationSwitches()).toHaveLength(10);
  });

  it("keeps linked-card expansion and unlink actions available", () => {
    const onToggleExpand = vi.fn();
    const onUnlinkSkill = vi.fn();
    const agent = profile("manual", false, false);

    const { rerender } = render(
      <AgentConnectionsSection
        profiles={[agent]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{ manual: ["pdf-tools"] }}
        onToggleProfile={vi.fn()}
        onToggleExpand={onToggleExpand}
        onUnlinkSkill={onUnlinkSkill}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /1 settings\.linked/ }));
    expect(onToggleExpand).toHaveBeenCalledWith("manual");

    rerender(
      <AgentConnectionsSection
        profiles={[agent]}
        profilesLoading={false}
        expandedAgentId="manual"
        linkedSkills={{ manual: ["pdf-tools"] }}
        onToggleProfile={vi.fn()}
        onToggleExpand={onToggleExpand}
        onUnlinkSkill={onUnlinkSkill}
      />,
    );

    expect(screen.getByText("pdf-tools")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "settings.unlink" }));
    expect(onUnlinkSkill).toHaveBeenCalledWith("pdf-tools", "manual");
  });

  it("does not show a stale synced count when the linked skill list is empty", () => {
    const agent = { ...profile("manual", false, false), synced_count: 3 };
    const onToggleExpand = vi.fn();

    const { rerender } = render(
      <AgentConnectionsSection
        profiles={[agent]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={onToggleExpand}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /3 settings\.linked/ })).toBeInTheDocument();

    rerender(
      <AgentConnectionsSection
        profiles={[agent]}
        profilesLoading={false}
        expandedAgentId="manual"
        linkedSkills={{ manual: [] }}
        onToggleProfile={vi.fn()}
        onToggleExpand={onToggleExpand}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(screen.getByText("settings.noSkillsLinked")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /3 settings\.linked/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /0 settings\.linked/ }));
    expect(onToggleExpand).toHaveBeenCalledWith("manual");
  });

  it("narrows the list by search query and by activation status", () => {
    render(
      <AgentConnectionsSection
        profiles={[profile("claude", false, true), profile("codex", false, false), profile("kiro", false, false)]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText("settings.searchAgents"), { target: { value: "C" } });
    expect(activationSwitches().map((toggle) => toggle.getAttribute("aria-label"))).toEqual([
      "Toggle claude",
      "Toggle codex",
    ]);

    fireEvent.click(screen.getByRole("button", { name: /settings\.filterAgentsDisabled/ }));
    expect(activationSwitches().map((toggle) => toggle.getAttribute("aria-label"))).toEqual(["Toggle codex"]);

    fireEvent.click(screen.getByRole("button", { name: "settings.clearAgentSearch" }));
    expect(activationSwitches().map((toggle) => toggle.getAttribute("aria-label"))).toEqual([
      "Toggle codex",
      "Toggle kiro",
    ]);
  });

  it("skips the ten-Agent fold while a filter is active", () => {
    const agents = Array.from({ length: 12 }, (_, index) => profile(`agent-${index + 1}`, false, false));

    render(
      <AgentConnectionsSection
        profiles={agents}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(activationSwitches()).toHaveLength(10);

    fireEvent.click(screen.getByRole("button", { name: /settings\.filterAgentsDisabled/ }));
    expect(activationSwitches()).toHaveLength(12);
    expect(screen.queryByRole("button", { name: /more Agents/ })).not.toBeInTheDocument();
  });

  it("offers a reset control when nothing matches the filter", () => {
    render(
      <AgentConnectionsSection
        profiles={[profile("claude", false, true)]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText("settings.searchAgents"), { target: { value: "nothing-matches" } });
    expect(screen.queryAllByRole("switch")).toHaveLength(0);
    expect(screen.getByText("settings.noAgentsMatchFilter")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "settings.clearAgentFilters" }));
    expect(screen.getByRole("switch", { name: "Toggle claude" })).toBeInTheDocument();
  });

  it("pauses only the active managed names and delegates the compact control", () => {
    const onToggleAllSkills = vi.fn();
    const claude = profile("claude", false, true);

    render(
      <AgentConnectionsSection
        profiles={[claude]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{ claude: ["alpha"] }}
        managedSkillStates={{
          [claude.global_skills_dir]: { active_skill_names: ["alpha"], suspended_skill_names: [] },
        }}
        pendingSkillTargetKeys={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
        onToggleAllSkills={onToggleAllSkills}
      />,
    );

    expect(screen.getByText("1 managed active")).toBeInTheDocument();
    const managedSkillsSwitch = screen.getByRole("switch", { name: "Managed skills claude" });
    expect(managedSkillsSwitch).toBeChecked();
    fireEvent.click(managedSkillsSwitch);
    expect(onToggleAllSkills).toHaveBeenCalledWith(claude);
  });

  it("keeps a paused snapshot restorable even when the folder is empty", () => {
    const onToggleAllSkills = vi.fn();
    const claude = profile("claude", false, true);

    render(
      <AgentConnectionsSection
        profiles={[claude]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{ claude: [] }}
        managedSkillStates={{
          [claude.global_skills_dir]: { active_skill_names: [], suspended_skill_names: ["alpha", "beta"] },
        }}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
        onToggleAllSkills={onToggleAllSkills}
      />,
    );

    expect(screen.getByText("2 paused skills")).toBeInTheDocument();
    const managedSkillsSwitch = screen.getByRole("switch", { name: "Managed skills claude" });
    expect(managedSkillsSwitch).not.toBeChecked();
    expect(managedSkillsSwitch).toBeEnabled();
    fireEvent.click(managedSkillsSwitch);
    expect(onToggleAllSkills).toHaveBeenCalledWith(claude);
  });

  it("disables an empty managed-skills folder instead of proposing a Hub sync", () => {
    const onToggleAllSkills = vi.fn();
    const claude = profile("claude", false, true);

    render(
      <AgentConnectionsSection
        profiles={[claude]}
        profilesLoading={false}
        expandedAgentId={null}
        linkedSkills={{ claude: [] }}
        managedSkillStates={{
          [claude.global_skills_dir]: { active_skill_names: [], suspended_skill_names: [] },
        }}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
        onToggleAllSkills={onToggleAllSkills}
      />,
    );

    expect(screen.getByText("No managed skills")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Managed skills claude" })).toBeDisabled();
    expect(onToggleAllSkills).not.toHaveBeenCalled();
  });

  it("shares the same suspended state and pending lock across a shared directory", () => {
    const claude = profile("claude", false, true);
    const codex = { ...profile("codex", false, true), global_skills_dir: claude.global_skills_dir };

    render(
      <AgentConnectionsSection
        profiles={[claude, codex]}
        profilesLoading={false}
        expandedAgentId="claude"
        linkedSkills={{ claude: ["alpha"], codex: ["alpha"] }}
        managedSkillStates={{
          [claude.global_skills_dir]: { active_skill_names: [], suspended_skill_names: ["alpha"] },
        }}
        pendingSkillTargetKeys={{ [claude.global_skills_dir]: true }}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
        onToggleAllSkills={vi.fn()}
      />,
    );

    expect(screen.getByText("Shared with codex")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Managed skills claude" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Managed skills codex" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "settings.unlink" })).toBeDisabled();
  });

  it("renders a matching skeleton while the Agent registry is loading", () => {
    render(
      <AgentConnectionsSection
        profiles={[]}
        profilesLoading
        expandedAgentId={null}
        linkedSkills={{}}
        onToggleProfile={vi.fn()}
        onToggleExpand={vi.fn()}
        onUnlinkSkill={vi.fn()}
      />,
    );

    expect(screen.getByRole("status", { name: "settings.loadingAgents" })).toBeInTheDocument();
  });
});
