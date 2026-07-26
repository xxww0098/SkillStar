/**
 * Dev-mock fragment: GitHub — gh/git tool status, user repos, and skill
 * publishing. All sample payloads are small and inline.
 */

import type { DevMockHandlers } from "./shared";

export const GITHUB_HANDLERS: DevMockHandlers = {
  check_gh_installed: () => true,
  check_gh_status: () => ({ status: "Ready", username: "dev-user" }),
  check_git_status: () => ({ status: "Installed", version: "2.45.0" }),
  list_repo_history: () => [],
  list_user_repos: () => [
    {
      full_name: "dev-user/my-skills",
      url: "https://github.com/dev-user/my-skills",
      description: "Personal SkillStar skills collection",
      is_public: true,
      folders: ["pdf-tools", "xlsx"],
    },
    {
      full_name: "dev-user/private-skills",
      url: "https://github.com/dev-user/private-skills",
      description: "Private work skills",
      is_public: false,
      folders: ["internal-tools"],
    },
  ],
  inspect_repo_folders: (args) => {
    const full = String((args?.repoFullName as string) ?? "");
    return full.endsWith("private-skills") ? ["internal-tools"] : ["pdf-tools", "xlsx"];
  },
  publish_skill_to_github: (args) => {
    const repoName = String((args?.repoName as string) ?? "my-skills");
    const folderName = String((args?.folderName as string) ?? (args?.skillName as string) ?? "skill");
    const url = `https://github.com/dev-user/${repoName}`;
    return { url, git_url: `${url}.git`, source_folder: folderName };
  },
};
