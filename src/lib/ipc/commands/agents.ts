import type { AgentProfile, CustomProfileDef } from "../../../types";

/** How a skill physically landed in an agent's skills dir (mirrors Rust `DeployKind`). */
export type DeployKind = "missing" | "link" | "copy" | "unknown";

/** Per-agent deploy inspection row returned by `get_skill_deploy_status`. */
export interface AgentDeployStatus {
  agent_id: string;
  agent_name: string;
  target_path: string;
  kind: DeployKind;
  /** Only meaningful when `kind === "link"`: `false` means the link is dangling. */
  link_alive: boolean;
}

/** Global agent profile configuration + per-agent skill links. */
export interface AgentCommands {
  list_agent_profiles: { args: Record<string, never>; result: AgentProfile[] };
  toggle_agent_profile: { args: { id: string }; result: boolean };
  add_custom_agent_profile: { args: { def: CustomProfileDef }; result: void };
  remove_custom_agent_profile: { args: { id: string }; result: void };

  unlink_all_skills_from_agent: { args: { agentId: string }; result: number };
  batch_link_skills_to_agent: { args: { skillNames: string[]; agentId: string }; result: number };
  list_linked_skills: { args: { agentId: string }; result: string[] };
  unlink_skill_from_agent: { args: { skillName: string; agentId: string }; result: void };
  batch_remove_skills_from_all_agents: { args: { skillNames: string[] }; result: void };

  toggle_skill_for_agent: {
    args: { skillName: string; agentId: string; enable: boolean };
    result: void;
  };

  get_skill_deploy_status: { args: { skillName: string }; result: AgentDeployStatus[] };
}
