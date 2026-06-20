import type { AgentProfile } from "../../../types";

/**
 * Dir-name aliases for the few cases where the remote discovery's agent id
 * (derived from the `~/.<agent>/skills` parent dir) differs from the local
 * agent-profile id. Keep this tiny — most ids already match the local profiles.
 */
const DIR_ALIASES: Record<string, string> = {
  "claude-code": "claude", // Models-style id → project/ssh profile id
  agent: "antigravity", // ~/.agent/skills is Antigravity's directory
};

function prettify(id: string): string {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join(" ");
}

/**
 * Resolve a remote-discovered agent id to an icon profile, using the SAME
 * locally-detected agent profiles the local skill cards use ({@link AgentProfile}
 * from `list_agent_profiles`) as the source of truth — so a remote skill under
 * `~/.codex/skills` shows the exact Codex icon a local skill does. Agents the
 * VPS has but the local machine doesn't fall back to the canonical
 * `agents/<id>.svg` icon convention (the same path scheme the profiles use).
 */
export function remoteAgentProfile(agentId: string, builtin: AgentProfile[]): AgentProfile {
  const id = agentId.trim().toLowerCase();
  const canonical = DIR_ALIASES[id] ?? id;
  const hit = builtin.find((p) => p.id === canonical) ?? builtin.find((p) => p.id === id);
  if (hit) return hit;

  return {
    id: canonical,
    display_name: prettify(canonical),
    icon: `agents/${canonical}.svg`,
    global_skills_dir: "",
    project_skills_rel: "",
    installed: false,
    enabled: true,
    synced_count: 0,
  };
}
