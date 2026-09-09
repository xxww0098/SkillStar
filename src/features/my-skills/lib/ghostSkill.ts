import type { RepoNewSkill, Skill } from "../../../types";

/**
 * Project a repo-discovered (ghost) skill onto the synthetic `Skill` the detail
 * drawer renders. A renamed ghost carries its `upstream_change` so the drawer
 * offers the same "migrate" action (deployments carried over) as the card,
 * instead of a bare install of the successor.
 */
export function syntheticSkillFromGhost(ghost: RepoNewSkill): Skill {
  return {
    name: ghost.skill_id,
    description: ghost.description,
    skill_type: "hub",
    stars: 0,
    installed: false,
    update_available: false,
    last_updated: new Date().toISOString(),
    git_url: ghost.repo_url,
    tree_hash: null,
    category: "None",
    author: null,
    topics: [],
    source: ghost.repo_source,
    upstream_change: ghost.renamed_from
      ? {
          kind: "removed",
          suggested_local_name: ghost.skill_id,
          successor: {
            skill_id: ghost.skill_id,
            folder_path: ghost.folder_path,
            description: ghost.description,
            similarity: null,
          },
        }
      : undefined,
  };
}
