export interface GitHubIdentity {
  id: number;
  login: string;
  avatar_url: string | null;
}

export type GitHubConnectionStatus =
  | { state: "signed_out" }
  | {
      state: "connected";
      identity: GitHubIdentity;
      access_expires_at: string | null;
    }
  | { state: "expired"; identity: GitHubIdentity | null };

export interface GitHubDeviceAuthorization {
  user_code: string;
  verification_uri: string;
  expires_at: string;
  interval_seconds: number;
}

export type GitHubDeviceFlowPoll =
  | { state: "pending"; retry_after_seconds: number }
  | { state: "slow_down"; retry_after_seconds: number }
  | { state: "connected"; connection: GitHubConnectionStatus }
  | { state: "denied" }
  | { state: "expired" };

export type GitHubAuthErrorCode =
  | "unavailable"
  | "network"
  | "proxy"
  | "credential_store"
  | "protocol"
  | "no_pending_authorization"
  | "not_authenticated"
  | "refresh_unavailable";

export interface GitHubAuthError {
  code: GitHubAuthErrorCode;
  message: string;
}

export type GitTransportErrorCode =
  | "not_authenticated"
  | "token_expired"
  | "unauthorized"
  | "app_not_installed"
  | "network"
  | "cancelled"
  | "credential_unavailable"
  | "unsafe_remote"
  | "other";

export type GitOperationPhase = "preparing" | "running" | "completed" | "failed" | "cancelled";

/** Payload of the `skillstar://git-progress` event. It intentionally contains no URL credentials. */
export interface GitOperationProgress {
  session_id: string;
  phase: GitOperationPhase;
  repository: string;
}
