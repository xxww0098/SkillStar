export type SharedChannelRole = "owner" | "publisher" | "subscriber";
export type SharedChannelStatus = "awaiting_app_installation" | "active";

export interface GitHubOrganization {
  id: number;
  login: string;
  avatar_url: string | null;
  viewer_is_admin: boolean;
}

export interface SharedChannelAuthorization {
  repository_selection: "selected";
  administration: "write";
  contents: "write";
}

export interface SharedChannelDescriptor {
  descriptor_version: number;
  repository_id: number;
  organization_id: number;
  owner: string;
  name: string;
  html_url: string;
  clone_url: string;
  role: SharedChannelRole;
  status: SharedChannelStatus;
  authorization: SharedChannelAuthorization;
  created_at: string;
  updated_at: string;
}

export interface CreateSharedChannelRequest {
  organization: string;
  repository_name: string;
  description: string;
}

export interface ExistingChannelRepositoryCandidate {
  repository_id: number;
  organization_id: number;
  owner: string;
  name: string;
  html_url: string;
  clone_url: string;
  role: SharedChannelRole;
  already_registered: boolean;
}

export interface ExistingChannelSkillPreview {
  id: string;
  folder_path: string;
  description: string;
}

export interface ExistingChannelExposure {
  full_repository_contents_readable: boolean;
  full_history_readable: boolean;
}

export interface ExistingChannelScanPreview {
  session_id: string;
  repository: ExistingChannelRepositoryCandidate;
  skills: ExistingChannelSkillPreview[];
  non_skill_files: string[];
  total_files: number;
  exposure: ExistingChannelExposure;
}

export interface ExistingChannelScanRequest {
  organization_id: number;
  repository_id: number;
}

export type ChannelSkillReleaseStatus = "added" | "updated" | "unchanged" | "removed";

export interface ChannelReleaseSkill {
  id: string;
  content_root: string;
  content_hash: string;
  content_hash_version: number;
  status: ChannelSkillReleaseStatus;
}

export interface ChannelPublisherIdentity {
  id: number;
  login: string;
}

export interface ChannelReleaseManifest {
  schema_version: number;
  repository_id: number;
  organization_id: number;
  revision: number;
  tag_name: string;
  commit_sha: string;
  publisher: ChannelPublisherIdentity;
  published_at: string;
  title: string;
  notes: string;
  skills: ChannelReleaseSkill[];
}

export interface ChannelPublishPreview {
  session_id: string;
  repository_id: number;
  commit_sha: string;
  next_revision: number;
  tag_name: string;
  changes: ChannelReleaseSkill[];
}

export interface RemoteChannelRelease {
  id: number;
  html_url: string;
}

export interface ChannelPublishResult {
  manifest: ChannelReleaseManifest;
  release: RemoteChannelRelease;
}
