import type { AgentProfile, Skill } from "../../../types";
import type { RemoteSkill } from "../../../lib/ipc/commands/ssh";
import { remoteAgentProfile } from "./remoteAgentProfile";

/** Map a discovered remote skill into {@link Skill} for {@link SkillCard} / {@link SkillGrid}. */
export function remoteSkillToSkill(remote: RemoteSkill, builtinProfiles: AgentProfile[]): Skill {
  const agentProfile = remote.agent ? remoteAgentProfile(remote.agent, builtinProfiles) : null;
  return {
    name: remote.name,
    description: remote.path,
    localized_description: null,
    skill_type: "hub",
    stars: 0,
    installed: true,
    update_available: false,
    last_updated: remote.modified ?? "",
    /** Stable grid key + drawer lookup (remote path). */
    git_url: remote.path,
    tree_hash: null,
    category: "None",
    author: remote.agent || null,
    topics: [],
    agent_links: agentProfile ? [agentProfile.display_name] : [],
    source: "remote",
  };
}

export function formatRemoteSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
