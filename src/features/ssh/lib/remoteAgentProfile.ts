import type { AgentProfile } from "../../../types";

/** Map a remote-discovered agent id to an icon profile for {@link AgentIcon}. */
export function remoteAgentProfile(agentId: string, builtin: AgentProfile[]): AgentProfile {
  const id = agentId.trim().toLowerCase();
  const hit = builtin.find((p) => p.id === id);
  if (hit) return hit;

  const iconOverrides: Record<string, string> = {
    grok: "agents/grok.svg",
    agents: "agents/claude.svg",
    agent: "agents/claude.svg",
    "claude-code": "agents/claude.svg",
    "claude-desktop": "agents/claude-desktop.svg",
  };

  const icon = iconOverrides[id] ?? `agents/${id}.svg`;
  const display = id.charAt(0).toUpperCase() + id.slice(1);

  return {
    id,
    display_name: display,
    icon,
    global_skills_dir: "",
    project_skills_rel: "",
    installed: false,
    enabled: true,
    synced_count: 0,
  };
}
