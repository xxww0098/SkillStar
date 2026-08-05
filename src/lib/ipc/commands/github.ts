import type {
  GitHubConnectionStatus,
  GitHubDeviceAuthorization,
  GitHubDeviceFlowPoll,
  GhStatus,
  PublishResult,
  RepoHistoryEntry,
  ScanResult,
  SkillInstallTarget,
  UserRepo,
} from "../../../types";

/** GitHub CLI status, repo scan/import, and skill publish. */
export interface GitHubCommands {
  github_auth_status: { args: Record<string, never>; result: GitHubConnectionStatus };
  github_auth_start: { args: Record<string, never>; result: GitHubDeviceAuthorization };
  github_auth_poll: { args: Record<string, never>; result: GitHubDeviceFlowPoll };
  github_auth_cancel: { args: Record<string, never>; result: boolean };
  github_auth_refresh: { args: Record<string, never>; result: GitHubConnectionStatus };
  github_auth_logout: { args: Record<string, never>; result: void };
  cancel_git_operation: { args: { sessionId: string }; result: boolean };
  check_gh_installed: { args: Record<string, never>; result: boolean };
  check_gh_status: { args: Record<string, never>; result: GhStatus };
  check_git_status: {
    args: Record<string, never>;
    result: import("../../../types").GitStatus;
  };

  list_user_repos: { args: { limit?: number }; result: UserRepo[] };
  inspect_repo_folders: { args: { repoFullName: string }; result: string[] };

  scan_github_repo: { args: { url: string; fullDepth?: boolean; sessionId?: string }; result: ScanResult };
  install_from_scan: {
    args: { repoUrl: string; source: string; skills: SkillInstallTarget[]; sessionId?: string };
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
