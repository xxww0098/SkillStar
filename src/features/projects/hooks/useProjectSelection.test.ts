import { invoke } from "@tauri-apps/api/core";
import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AgentProfile,
  ProjectDeployMode,
  ProjectEntry,
  ProjectScanResult,
  ScannedSkill,
  SkillsList,
} from "../../../types";
import { useProjectSelection } from "./useProjectSelection";

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
  },
}));

const mockedInvoke = vi.mocked(invoke);

const CLAUDE: AgentProfile = {
  id: "claude",
  display_name: "Claude Code",
  icon: "claude.svg",
  enabled: true,
  global_skills_dir: "/home/user/.claude/skills",
  project_skills_rel: ".claude/skills",
  installed: true,
  synced_count: 3,
};

const CURSOR: AgentProfile = {
  id: "cursor",
  display_name: "Cursor",
  icon: "cursor.svg",
  enabled: true,
  global_skills_dir: "/home/user/.cursor/rules/skills",
  project_skills_rel: ".cursor/rules/skills",
  installed: true,
  synced_count: 0,
};

const PROJECT: ProjectEntry = {
  path: "/tmp/my-project",
  name: "my-project",
  created_at: "2026-01-01T00:00:00Z",
};

function scannedSkill(overrides: Partial<ScannedSkill> = {}): ScannedSkill {
  return {
    name: "writing-plans",
    agent_id: "claude",
    is_symlink: true,
    in_hub: true,
    has_skill_md: true,
    managed: false,
    ...overrides,
  };
}

/** Builds the full param set for `useProjectSelection`, with sane no-op defaults. */
function buildParams(overrides: Partial<Parameters<typeof useProjectSelection>[0]> = {}) {
  const identityCanonicalize = vi.fn((agents: Record<string, string[]>) => agents);
  const identityFilterAgents = vi.fn((agents: Record<string, string[]>) => agents);
  const identityFilterUnmanaged = vi.fn((skills: ScannedSkill[]) => skills);
  const buildSymlinkSkillIndex = vi.fn((_skills: ScannedSkill[]) => ({}) as Record<string, string[]>);

  return {
    enabledProfiles: [CLAUDE, CURSOR],
    enabledProfileIdSet: new Set(["claude", "cursor"]),
    pathByAgentId: new Map([
      ["claude", ".claude/skills"],
      ["cursor", ".cursor/rules/skills"],
    ]),
    canonicalizeAgentsBySharedPath: identityCanonicalize,
    filterAgentsByEnabledProfiles: identityFilterAgents,
    filterUnmanagedByEnabledProfiles: identityFilterUnmanaged,
    buildSymlinkSkillIndex,
    loadProjectSkills: vi.fn(async (_name: string): Promise<SkillsList | null> => null),
    scanProjectSkills: vi.fn(
      async (_projectPath: string): Promise<ProjectScanResult> => ({ skills: [], agents_found: [] }),
    ),
    rebuildProjectSkillsFromDisk: vi.fn(
      async (_projectPath: string): Promise<SkillsList> => ({
        agents: {},
        updated_at: "2026-01-01T00:00:00Z",
      }),
    ),
    resetScanState: vi.fn(),
    resetDisambigState: vi.fn(),
    setScannedSymlinkSkillsByAgent: vi.fn(),
    runAgentDetection: vi.fn(async () => {}),
    setUnmanagedAndMaybeExpand: vi.fn(),
    presentProjectState: vi.fn(),
    openDeployAgentDialog: vi.fn(),
    pendingGroupSkills: null,
    setDeployModes: vi.fn(),
    t: (key: string, options?: Record<string, unknown>) => (options?.defaultValue as string) ?? key,
    ...overrides,
  };
}

describe("useProjectSelection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedInvoke.mockResolvedValue(undefined);
  });

  it("self-heals: disk has skills but config is empty, so it rebuilds skills-list.json from disk", async () => {
    const params = buildParams({
      // Config is empty (no configured skills) — matches `loadProjectSkills` returning
      // an agents map with no non-empty skill lists.
      loadProjectSkills: vi.fn(async () => ({
        agents: { claude: [] },
        updated_at: "2026-01-01T00:00:00Z",
      })),
      // Disk scan finds a real, has_skill_md skill for an enabled agent.
      scanProjectSkills: vi.fn(async () => ({
        skills: [scannedSkill({ agent_id: "claude", is_symlink: false, has_skill_md: true })],
        agents_found: ["claude"],
      })),
      rebuildProjectSkillsFromDisk: vi.fn(async (projectPath: string) => {
        expect(projectPath).toBe(PROJECT.path);
        return {
          agents: { claude: ["writing-plans"] },
          updated_at: "2026-01-01T00:00:00Z",
        };
      }),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    await waitFor(() => {
      expect(params.rebuildProjectSkillsFromDisk).toHaveBeenCalledTimes(1);
    });
    expect(params.rebuildProjectSkillsFromDisk).toHaveBeenCalledWith(PROJECT.path);

    // The rebuilt agents map (post self-heal) is what gets presented, not the
    // original empty config.
    expect(params.presentProjectState).toHaveBeenCalledWith(
      PROJECT,
      expect.objectContaining({ claude: expect.arrayContaining(["writing-plans"]) }),
      false,
    );
  });

  it("normal path: disk and config already agree, so no self-heal rebuild happens", async () => {
    const params = buildParams({
      // Config already has a configured skill for claude.
      loadProjectSkills: vi.fn(async () => ({
        agents: { claude: ["writing-plans"] },
        updated_at: "2026-01-01T00:00:00Z",
      })),
      scanProjectSkills: vi.fn(async () => ({
        skills: [scannedSkill({ agent_id: "claude", is_symlink: true, has_skill_md: true })],
        agents_found: ["claude"],
      })),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(params.rebuildProjectSkillsFromDisk).not.toHaveBeenCalled();
    expect(params.presentProjectState).toHaveBeenCalledWith(
      PROJECT,
      expect.objectContaining({ claude: expect.arrayContaining(["writing-plans"]) }),
      false,
    );
  });

  it("does not self-heal when the disk scan finds nothing (empty project)", async () => {
    const params = buildParams({
      loadProjectSkills: vi.fn(async () => ({
        agents: {},
        updated_at: "2026-01-01T00:00:00Z",
      })),
      scanProjectSkills: vi.fn(async () => ({ skills: [], agents_found: [] })),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(params.rebuildProjectSkillsFromDisk).not.toHaveBeenCalled();
    expect(params.presentProjectState).toHaveBeenCalledWith(PROJECT, {}, false);
  });

  it("tolerates a failed disk scan: reports the error and continues with an empty scan result", async () => {
    const scanError = new Error("disk unreadable");
    const params = buildParams({
      loadProjectSkills: vi.fn(async () => ({
        agents: {},
        updated_at: "2026-01-01T00:00:00Z",
      })),
      scanProjectSkills: vi.fn(async () => {
        throw scanError;
      }),
    });

    const { toast } = await import("../../../lib/toast");
    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    // Scan failure must not throw out of handleSelectProject — it recovers and
    // still presents project state (with no scanned skills).
    expect(toast.error).toHaveBeenCalledTimes(1);
    expect(params.rebuildProjectSkillsFromDisk).not.toHaveBeenCalled();
    expect(params.presentProjectState).toHaveBeenCalledWith(PROJECT, {}, false);
    expect(params.setUnmanagedAndMaybeExpand).toHaveBeenCalledWith([]);
  });

  it("always fires a background refresh_stale_project_copies invoke, regardless of scan outcome", async () => {
    const params = buildParams({
      loadProjectSkills: vi.fn(async () => null),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(mockedInvoke).toHaveBeenCalledWith("refresh_stale_project_copies", { projectPath: PROJECT.path });
  });

  it("opens the deploy-agent dialog and suppresses the disambiguation dialog when skills are pending from a SkillCards deploy", async () => {
    const params = buildParams({
      loadProjectSkills: vi.fn(async () => null),
      pendingGroupSkills: ["writing-plans"],
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(params.openDeployAgentDialog).toHaveBeenCalledWith(PROJECT, {});
    expect(params.runAgentDetection).toHaveBeenCalledWith(PROJECT.path, {}, {}, PROJECT, true);
  });

  it("hydrates per-agent deploy modes from the persisted path-keyed config", async () => {
    const params = buildParams({
      loadProjectSkills: vi.fn(
        async (): Promise<SkillsList> => ({
          agents: { claude: ["writing-plans"] },
          deploy_modes: { ".claude/skills": "copy" as ProjectDeployMode },
          updated_at: "2026-01-01T00:00:00Z",
        }),
      ),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(params.setDeployModes).toHaveBeenCalledWith({ claude: "copy" });
  });

  it("resets scan and disambiguation state at the start of every selection", async () => {
    const params = buildParams({
      loadProjectSkills: vi.fn(async () => null),
    });

    const { result } = renderHook(() => useProjectSelection(params));
    await result.current.handleSelectProject(PROJECT);

    expect(params.resetScanState).toHaveBeenCalledTimes(1);
    expect(params.resetDisambigState).toHaveBeenCalledTimes(1);
  });
});
