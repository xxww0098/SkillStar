import type { Accessor } from "nsh";
import {
  githubAuthPolling,
  githubAuthSummary,
  logoutGitHub,
  openGitHubSignIn,
  refreshGitHubAuth,
  type GitHubAuthSummary,
} from "../pages/my_skills/githubSignInSlot";
import { requestChannelsScope } from "./events";
import { setCurrentPage } from "./nav";

export interface SidebarGitHubBridge {
  summary: Accessor<GitHubAuthSummary>;
  polling: Accessor<boolean>;
  openSignIn(): void;
  refreshAuth(): Promise<void>;
  logout(): Promise<void>;
  openChannelsWorkspace(): void;
}

export const defaultSidebarGitHubBridge: SidebarGitHubBridge = {
  summary: githubAuthSummary,
  polling: githubAuthPolling,
  openSignIn: openGitHubSignIn,
  refreshAuth: refreshGitHubAuth,
  logout: logoutGitHub,
  openChannelsWorkspace(): void {
    setCurrentPage("my_skills");
    requestChannelsScope.publish();
  },
};
