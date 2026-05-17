import type {
  ImportResult,
  ImportTarget,
  ProjectAgentDetection,
  ProjectDeployMode,
  ProjectEntry,
  ProjectScanResult,
  SkillsList,
} from "../../../types";

/** Project registration, sync, import, and on-disk reconciliation. */
export interface ProjectCommands {
  register_project: { args: { projectPath: string }; result: ProjectEntry };
  list_projects: { args: Record<string, never>; result: ProjectEntry[] };
  update_project_path: { args: { name: string; newPath: string }; result: number };
  remove_project: { args: { name: string }; result: void };

  get_project_skills: { args: { name: string }; result: SkillsList | null };
  create_project_skills: {
    args: { projectPath: string; selectedSkills: string[]; agentTypes: string[] };
    result: number;
  };
  save_and_sync_project: {
    args: {
      projectPath: string;
      agents: Record<string, string[]>;
      deployModes?: Record<string, ProjectDeployMode>;
    };
    result: number;
  };
  save_project_skills_list: {
    args: { projectPath: string; agents: Record<string, string[]> };
    result: SkillsList;
  };

  scan_project_skills: { args: { projectPath: string }; result: ProjectScanResult };
  rebuild_project_skills_from_disk: { args: { projectPath: string }; result: SkillsList };
  import_project_skills: {
    args: { projectPath: string; projectName: string; targets: ImportTarget[] };
    result: ImportResult;
  };
  detect_project_agents: { args: { projectPath: string }; result: ProjectAgentDetection };

  /** Background maintenance: re-deploy copy-deployed skills whose source changed. */
  refresh_stale_project_copies: { args: { projectPath: string }; result: number };
}
