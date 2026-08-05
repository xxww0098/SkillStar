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
