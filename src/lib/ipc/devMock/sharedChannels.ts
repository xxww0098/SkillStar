import type { ExistingChannelScanPreview, SharedChannelDescriptor } from "../../../types";
import type { DevMockHandlers } from "./shared";

let channels: SharedChannelDescriptor[] = [];
const registrationPreviews = new Map<string, ExistingChannelScanPreview>();

export const SHARED_CHANNEL_HANDLERS: DevMockHandlers = {
  list_shared_channel_organizations: () => [
    { id: 7, login: "acme", avatar_url: null, viewer_is_admin: true },
    { id: 8, login: "design-lab", avatar_url: null, viewer_is_admin: false },
  ],
  list_shared_channels: () => channels,
  create_shared_channel: (args) => {
    const request = (args?.request ?? {}) as { organization?: string; repository_name?: string };
    const owner = request.organization || "acme";
    const name = request.repository_name || "skillstar-team";
    const now = new Date().toISOString();
    const channel: SharedChannelDescriptor = {
      descriptor_version: 1,
      repository_id: 42,
      organization_id: 7,
      owner,
      name,
      html_url: `https://github.com/${owner}/${name}`,
      clone_url: `https://github.com/${owner}/${name}.git`,
      role: "owner",
      status: "active",
      authorization: { repository_selection: "selected", administration: "write", contents: "write" },
      created_at: now,
      updated_at: now,
    };
    channels = [channel];
    return channel;
  },
  resume_shared_channel: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const current = channels.find((channel) => channel.repository_id === repositoryId);
    if (!current) throw new Error("repository_not_found: Pending shared channel not found");
    const active = { ...current, status: "active" as const, updated_at: new Date().toISOString() };
    channels = channels.map((channel) => (channel.repository_id === repositoryId ? active : channel));
    return active;
  },
  list_existing_channel_repositories: () => [
    {
      repository_id: 84,
      organization_id: 7,
      owner: "acme",
      name: "existing-skills",
      html_url: "https://github.com/acme/existing-skills",
      clone_url: "https://github.com/acme/existing-skills.git",
      role: "owner",
      already_registered: channels.some((channel) => channel.repository_id === 84),
    },
  ],
  scan_existing_shared_channel: (args) => {
    const sessionId = String(args?.sessionId ?? crypto.randomUUID());
    const preview: ExistingChannelScanPreview = {
      session_id: sessionId,
      repository: {
        repository_id: 84,
        organization_id: 7,
        owner: "acme",
        name: "existing-skills",
        html_url: "https://github.com/acme/existing-skills",
        clone_url: "https://github.com/acme/existing-skills.git",
        role: "owner",
        already_registered: false,
      },
      skills: [{ id: "writer", folder_path: "skills/writer", description: "Write clearly" }],
      non_skill_files: ["README.md", ".github/workflows/ci.yml"],
      total_files: 5,
      exposure: { full_repository_contents_readable: true, full_history_readable: true },
    };
    registrationPreviews.set(sessionId, preview);
    return preview;
  },
  confirm_existing_shared_channel: (args) => {
    const sessionId = String(args?.sessionId ?? "");
    const preview = registrationPreviews.get(sessionId);
    if (!preview) throw new Error("registration_session_not_found: Scan the repository again");
    const now = new Date().toISOString();
    const channel: SharedChannelDescriptor = {
      descriptor_version: 1,
      repository_id: preview.repository.repository_id,
      organization_id: preview.repository.organization_id,
      owner: preview.repository.owner,
      name: preview.repository.name,
      html_url: preview.repository.html_url,
      clone_url: preview.repository.clone_url,
      role: "owner",
      status: "active",
      authorization: { repository_selection: "selected", administration: "write", contents: "write" },
      created_at: now,
      updated_at: now,
    };
    channels = [channel, ...channels.filter((item) => item.repository_id !== channel.repository_id)];
    registrationPreviews.delete(sessionId);
    return channel;
  },
  cancel_existing_shared_channel_registration: (args) => registrationPreviews.delete(String(args?.sessionId ?? "")),
};
