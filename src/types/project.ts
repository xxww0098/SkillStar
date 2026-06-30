//! project domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export interface AgentProfile {
  id: string;
  display_name: string;
  icon: string;
  global_skills_dir: string;
  project_skills_rel: string;
  installed: boolean;
  enabled: boolean;
  synced_count: number;
}

export interface CustomProfileDef {
  id: string;
  display_name: string;
  global_skills_dir: string;
  project_skills_rel: string;
  icon_data_uri: string | null;
}

export interface ProjectEntry {
  path: string;
  name: string;
  created_at: string;
}

/** Per `project_skills_rel` path (e.g. `.agent/skills`), how hub skills are materialized in the project. */

export type ProjectDeployMode = "symlink" | "copy";

export interface SkillsList {
  agents: Record<string, string[]>;
  /** Keyed by `project_skills_rel`; omitted or empty means symlink for that path. */
  deploy_modes?: Record<string, ProjectDeployMode>;
  updated_at: string;
}

export interface ScannedSkill {
  name: string;
  agent_id: string;
  is_symlink: boolean;
  in_hub: boolean;
  has_skill_md: boolean;
  /** Backend-computed: skill is already recorded in skills-list.json (e.g. copy-mode deploy). */
  managed: boolean;
}

export interface ProjectScanResult {
  skills: ScannedSkill[];
  agents_found: string[];
}

export interface ImportTarget {
  name: string;
  agent_id: string;
}

export interface ImportResult {
  imported_to_hub: string[];
  skills_list_updated: boolean;
  symlink_count: number;
}

export interface ImportDone {
  hub: number;
  links: number;
}

// ── Project Agent Detection ─────────────────────────────────────────

export interface DetectedAgent {
  agent_id: string;
  display_name: string;
  icon: string;
  project_skills_rel: string;
  exists: boolean;
}

export interface AmbiguousGroup {
  path: string;
  agent_ids: string[];
  agent_names: string[];
}

export interface ProjectAgentDetection {
  detected: DetectedAgent[];
  /** Groups of agents that share the same directory and that directory exists */
  ambiguous_groups: AmbiguousGroup[];
  /** Agent IDs with a unique directory that exists — safe to auto-enable */
  auto_enable: string[];
}
