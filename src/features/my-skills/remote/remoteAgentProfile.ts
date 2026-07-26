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

const DISPLAY_NAMES: Readonly<Record<string, string>> = {
  claude: "Claude Code",
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
 * registered local profiles as visual metadata only — so a remote skill under
 * `~/.codex/skills` shows the same Codex icon. Remote discovery is explicit and
 * must not inherit the local Settings activation switch.
 */
export function remoteAgentProfile(agentId: string, builtin: AgentProfile[]): AgentProfile {
  const id = agentId.trim().toLowerCase();
  const canonical = DIR_ALIASES[id] ?? id;
  const hit = builtin.find((p) => p.id === canonical) ?? builtin.find((p) => p.id === id);
  if (hit) return { ...hit, installed: true, enabled: true };

  return {
    id: canonical,
    display_name: DISPLAY_NAMES[canonical] ?? prettify(canonical),
    icon: `lobe:${canonical}`,
    global_skills_dir: "",
    project_skills_rel: "",
    installed: true,
    enabled: true,
    synced_count: 0,
  };
}
