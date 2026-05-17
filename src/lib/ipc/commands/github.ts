import type {
  GhStatus,
  PublishResult,
  RepoHistoryEntry,
  ScanResult,
  SkillInstallTarget,
  UserRepo,
} from "../../../types";

/** GitHub CLI status, repo scan/import, and skill publish. */
export interface GitHubCommands {
  check_gh_installed: { args: Record<string, never>; result: boolean };
  check_gh_status: { args: Record<string, never>; result: GhStatus };
  check_git_status: {
    args: Record<string, never>;
    result: import("../../../types").GitStatus;
  };

  list_user_repos: { args: { limit?: number }; result: UserRepo[] };
  inspect_repo_folders: { args: { repoFullName: string }; result: string[] };

  scan_github_repo: { args: { url: string; fullDepth?: boolean }; result: ScanResult };
  install_from_scan: {
    args: { repoUrl: string; source: string; skills: SkillInstallTarget[] };
    result: string[];
  };

  publish_skill_to_github: {
    args: {
      skillName: string;
      description: string;
      isPublic: boolean;
      existingRepoUrl?: string | null;
      folderName: string;
      repoName: string;
    };
    result: PublishResult;
  };

  list_repo_history: { args: Record<string, never>; result: RepoHistoryEntry[] };
}
