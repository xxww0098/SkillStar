import type {
  ChannelInvitation,
  ChannelInvitationAction,
  ChannelInviteRole,
  ChannelMembershipSnapshot,
  ChannelPublishPreview,
  ChannelPublishResult,
  ChannelSubscription,
  ChannelSubscriptionReview,
  ChannelAutoUpdateRun,
  ChannelUpdateSnapshot,
  ExistingChannelScanPreview,
  SharedChannelDescriptor,
} from "../../../types";
import type { DevMockHandlers } from "./shared";

let channels: SharedChannelDescriptor[] = [];
const registrationPreviews = new Map<string, ExistingChannelScanPreview>();
const publicationPreviews = new Map<string, ChannelPublishPreview>();
let publishedRevision = 0;
let nextInvitationId = 902;
let repositoryInvitations: ChannelInvitation[] = [];
let inboxInvitations: ChannelInvitation[] = [
  {
    id: 901,
    repository_id: 84,
    organization_id: 7,
    owner: "acme",
    repository_name: "design-skills",
    html_url: "https://github.com/acme/design-skills",
    invitee: { id: 99, login: "demo-user" },
    inviter: { id: 7, login: "alice" },
    role: "subscriber",
    effective_role: "subscriber",
    status: "pending",
    created_at: new Date().toISOString(),
  },
];
let channelSubscriptions: ChannelSubscription[] = [];

export const SHARED_CHANNEL_HANDLERS: DevMockHandlers = {
  list_shared_channel_organizations: () => [
    { id: 7, login: "acme", avatar_url: null, viewer_is_admin: true },
    { id: 8, login: "design-lab", avatar_url: null, viewer_is_admin: false },
  ],
  list_shared_channels: () => channels,
  create_shared_channel: (args) => {
    const request = (args?.request ?? {}) as {
      organization?: string;
      repository_name?: string;
    };
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
      authorization: {
        repository_selection: "selected",
        administration: "write",
        contents: "write",
      },
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
    const active = {
      ...current,
      status: "active" as const,
      updated_at: new Date().toISOString(),
    };
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
      skills: [
        {
          id: "writer",
          folder_path: "skills/writer",
          description: "Write clearly",
        },
      ],
      non_skill_files: ["README.md", ".github/workflows/ci.yml"],
      total_files: 5,
      exposure: {
        full_repository_contents_readable: true,
        full_history_readable: true,
      },
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
      authorization: {
        repository_selection: "selected",
        administration: "write",
        contents: "write",
      },
      created_at: now,
      updated_at: now,
    };
    channels = [channel, ...channels.filter((item) => item.repository_id !== channel.repository_id)];
    registrationPreviews.delete(sessionId);
    return channel;
  },
  cancel_existing_shared_channel_registration: (args) => registrationPreviews.delete(String(args?.sessionId ?? "")),
  preview_shared_channel_publish: (args) => {
    const sessionId = String(args?.sessionId ?? crypto.randomUUID());
    const repositoryId = Number(args?.repositoryId ?? 42);
    const nextRevision = publishedRevision + 1;
    const preview: ChannelPublishPreview = {
      session_id: sessionId,
      repository_id: repositoryId,
      commit_sha: "0123456789abcdef0123456789abcdef01234567",
      next_revision: nextRevision,
      tag_name: `channel-v${String(nextRevision).padStart(6, "0")}`,
      changes: [
        {
          id: "writer",
          content_root: "skills/writer",
          content_hash: "sha256:6e8b30c29c269c5375c2149f4834f8f6d289e5842b6d75f0f912749605a537f7",
          content_hash_version: 2,
          status: publishedRevision === 0 ? "added" : "updated",
        },
      ],
    };
    publicationPreviews.set(sessionId, preview);
    return preview;
  },
  publish_shared_channel: (args) => {
    const sessionId = String(args?.sessionId ?? "");
    const preview = publicationPreviews.get(sessionId);
    if (!preview) throw new Error("registration_session_not_found: Scan the channel draft again");
    const channel = channels.find((item) => item.repository_id === preview.repository_id);
    const result: ChannelPublishResult = {
      manifest: {
        schema_version: 1,
        repository_id: preview.repository_id,
        organization_id: channel?.organization_id ?? 7,
        revision: preview.next_revision,
        tag_name: preview.tag_name,
        commit_sha: preview.commit_sha,
        publisher: { id: 99, login: "demo-user" },
        published_at: new Date().toISOString(),
        title: String(args?.title ?? "Shared Skills"),
        notes: String(args?.notes ?? ""),
        skills: preview.changes,
      },
      release: {
        id: 500 + preview.next_revision,
        html_url: `${channel?.html_url ?? "https://github.com/acme/shared"}/releases/tag/${preview.tag_name}`,
      },
    };
    publishedRevision = preview.next_revision;
    publicationPreviews.delete(sessionId);
    return result;
  },
  cancel_shared_channel_publish: (args) => publicationPreviews.delete(String(args?.sessionId ?? "")),
  list_shared_channel_membership: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const snapshot: ChannelMembershipSnapshot = {
      repository_id: repositoryId,
      members: [
        {
          user: { id: 7, login: "alice" },
          role: "owner",
          github_role_name: "admin",
          status: "accepted",
        },
      ],
      invitations: repositoryInvitations.filter((invitation) => invitation.repository_id === repositoryId),
    };
    return snapshot;
  },
  invite_shared_channel_member: (args) => {
    const request = (args?.request ?? {}) as {
      repository_id?: number;
      username?: string;
      role?: ChannelInviteRole;
    };
    const channel = channels.find((item) => item.repository_id === Number(request.repository_id));
    const invitation: ChannelInvitation = {
      id: nextInvitationId++,
      repository_id: channel?.repository_id ?? 42,
      organization_id: channel?.organization_id ?? 7,
      owner: channel?.owner ?? "acme",
      repository_name: channel?.name ?? "skillstar-shared",
      html_url: channel?.html_url ?? "https://github.com/acme/skillstar-shared",
      invitee: {
        id: nextInvitationId,
        login: request.username || "collaborator",
      },
      inviter: { id: 7, login: "alice" },
      role: request.role ?? "subscriber",
      effective_role: request.role ?? "subscriber",
      status: "pending",
      created_at: new Date().toISOString(),
    };
    repositoryInvitations = [invitation, ...repositoryInvitations];
    const action: ChannelInvitationAction = {
      repository_id: invitation.repository_id,
      invitation_id: invitation.id,
      username: invitation.invitee?.login ?? "",
      role: invitation.role,
      status: "pending",
    };
    return action;
  },
  cancel_shared_channel_invitation: (args) => {
    const invitationId = Number(args?.invitationId ?? 0);
    const invitation = repositoryInvitations.find((item) => item.id === invitationId);
    repositoryInvitations = repositoryInvitations.filter((item) => item.id !== invitationId);
    return {
      repository_id: invitation?.repository_id ?? Number(args?.repositoryId ?? 0),
      invitation_id: invitationId,
      username: invitation?.invitee?.login ?? "",
      role: invitation?.role ?? "subscriber",
      status: "cancelled",
    } satisfies ChannelInvitationAction;
  },
  resend_shared_channel_invitation: (args) => {
    const invitationId = Number(args?.invitationId ?? 0);
    const previous = repositoryInvitations.find((item) => item.id === invitationId);
    if (!previous) throw new Error("invitation_not_found: Refresh and try again");
    repositoryInvitations = repositoryInvitations.filter((item) => item.id !== invitationId);
    const replacement = {
      ...previous,
      id: nextInvitationId++,
      created_at: new Date().toISOString(),
    };
    repositoryInvitations = [replacement, ...repositoryInvitations];
    return {
      repository_id: replacement.repository_id,
      invitation_id: replacement.id,
      username: replacement.invitee?.login ?? "",
      role: replacement.role,
      status: "pending",
    } satisfies ChannelInvitationAction;
  },
  list_shared_channel_invitation_inbox: () => inboxInvitations,
  accept_shared_channel_invitation: (args) => {
    const invitationId = Number(args?.invitationId ?? 0);
    const invitation = inboxInvitations.find((item) => item.id === invitationId);
    if (!invitation) throw new Error("invitation_not_found: Refresh and try again");
    const now = new Date().toISOString();
    const descriptor: SharedChannelDescriptor = {
      descriptor_version: 1,
      repository_id: invitation.repository_id,
      organization_id: invitation.organization_id,
      owner: invitation.owner,
      name: invitation.repository_name,
      html_url: invitation.html_url,
      clone_url: `${invitation.html_url}.git`,
      role: invitation.role,
      status: "active",
      authorization: {
        repository_selection: "selected",
        administration: "write",
        contents: "write",
      },
      created_at: now,
      updated_at: now,
    };
    channels = [descriptor, ...channels.filter((item) => item.repository_id !== descriptor.repository_id)];
    inboxInvitations = inboxInvitations.filter((item) => item.id !== invitationId);
    return descriptor;
  },
  decline_shared_channel_invitation: (args) => {
    const invitationId = Number(args?.invitationId ?? 0);
    const invitation = inboxInvitations.find((item) => item.id === invitationId);
    if (!invitation) throw new Error("invitation_not_found: Refresh and try again");
    inboxInvitations = inboxInvitations.filter((item) => item.id !== invitationId);
    return {
      repository_id: invitation.repository_id,
      invitation_id: invitation.id,
      username: invitation.invitee?.login ?? "",
      role: invitation.role,
      status: "cancelled",
    } satisfies ChannelInvitationAction;
  },
  resume_accepted_shared_channel: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const pending = channels.find((item) => item.repository_id === repositoryId);
    if (!pending) throw new Error("repository_not_found: Accepted invitation recovery marker not found");
    const active = { ...pending, status: "active" as const, updated_at: new Date().toISOString() };
    channels = channels.map((item) => (item.repository_id === repositoryId ? active : item));
    return active;
  },
  list_shared_channel_subscriptions: () =>
    channelSubscriptions.map((subscription) => ({
      schema_version: 1,
      descriptor_version: subscription.descriptor_version,
      repository_id: subscription.repository_id,
      organization_id: subscription.organization_id,
      target: subscription.target,
      selected_skill_ids: subscription.skills.map((skill) => skill.id),
      auto_update: subscription.auto_update,
      read_only: false,
    })),
  review_shared_channel_subscription: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const channel = channels.find((item) => item.repository_id === repositoryId);
    if (!channel) throw new Error("repository_not_found: Shared channel not found");
    const existing = channelSubscriptions.find((item) => item.repository_id === repositoryId);
    const revision = Math.max(publishedRevision, 1);
    const selected = new Set(existing?.skills.map((skill) => skill.id) ?? ["reader", "writer"]);
    return {
      channel,
      target: {
        revision,
        tag_name: `channel-v${String(revision).padStart(6, "0")}`,
        commit_sha: "0123456789abcdef0123456789abcdef01234567",
      },
      title: "Shared Skills",
      notes: "Review this immutable release before installing it.",
      publisher: { id: 99, login: "demo-user" },
      published_at: new Date().toISOString(),
      exposure: {
        private_repository: true,
        full_repository_contents_readable: true,
        full_history_readable: true,
      },
      skills: [
        {
          id: "reader",
          content_root: "skills/reader",
          content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          content_hash_version: 2,
          selected: selected.has("reader"),
        },
        {
          id: "writer",
          content_root: "skills/writer",
          content_hash: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          content_hash_version: 2,
          selected: selected.has("writer"),
        },
      ],
      read_only: false,
    } satisfies ChannelSubscriptionReview;
  },
  subscribe_shared_channel: (args) => {
    const request = (args?.request ?? {}) as {
      repository_id?: number;
      target?: ChannelSubscription["target"];
      selected_skill_ids?: string[];
    };
    const repositoryId = Number(request.repository_id ?? 0);
    const channel = channels.find((item) => item.repository_id === repositoryId);
    if (!channel) throw new Error("repository_not_found: Shared channel not found");
    const revision = Number(request.target?.revision ?? 1);
    const commit = "0123456789abcdef0123456789abcdef01234567";
    const selected = request.selected_skill_ids ?? [];
    const now = new Date().toISOString();
    const subscription: ChannelSubscription = {
      descriptor_version: 3,
      repository_id: repositoryId,
      organization_id: channel.organization_id,
      target: {
        revision,
        tag_name: `channel-v${String(revision).padStart(6, "0")}`,
        commit_sha: request.target?.commit_sha ?? commit,
      },
      skills: selected.map((id) => ({
        id,
        content_root: `skills/${id}`,
        release_content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        release_content_hash_version: 2,
        baseline_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        baseline_hash_version: 2,
        provenance: {
          repository_id: repositoryId,
          repository_url: channel.clone_url,
          git_ref: commit,
          source_folder: `skills/${id}`,
        },
      })),
      known_skill_ids: selected,
      pins: [],
      last_update: null,
      auto_update: { enabled: false, next_check_at: null, last_run: null },
      created_at: now,
      updated_at: now,
    };
    channelSubscriptions = [
      subscription,
      ...channelSubscriptions.filter((item) => item.repository_id !== repositoryId),
    ];
    return subscription;
  },
  get_shared_channel_update_state: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    return channelSubscriptions.find((item) => item.repository_id === repositoryId)?.last_update ?? null;
  },
  check_shared_channel_update: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const subscription = channelSubscriptions.find((item) => item.repository_id === repositoryId);
    if (!subscription) throw new Error("subscription_not_found: Subscribe to this channel first");
    const revision = subscription.target.revision + 1;
    const snapshot: ChannelUpdateSnapshot = {
      target: {
        revision,
        tag_name: `channel-v${String(revision).padStart(6, "0")}`,
        commit_sha: "fedcba9876543210fedcba9876543210fedcba98",
      },
      title: `Shared Skills ${revision}`,
      notes: "A reviewed channel update is ready.",
      publisher: { id: 99, login: "demo-user" },
      published_at: new Date().toISOString(),
      checked_at: new Date().toISOString(),
      status: "update_available",
      acknowledgement_required: true,
      items: [
        ...subscription.skills.map((skill) => ({
          id: skill.id,
          change: "updated" as const,
          state: "available" as const,
          selected: true,
          from_content_hash: skill.release_content_hash,
          to_content_hash: "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          block_reason: null,
          suggested_local_name: null,
          error: null,
        })),
        {
          id: "new-in-release",
          change: "added",
          state: "notification",
          selected: false,
          from_content_hash: null,
          to_content_hash: "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
          block_reason: null,
          suggested_local_name: null,
          error: null,
        },
      ],
      check_error: null,
      check_error_code: null,
    };
    subscription.last_update = snapshot;
    return snapshot;
  },
  apply_shared_channel_update: (args) => {
    const request = (args?.request ?? {}) as {
      repository_id?: number;
      target?: ChannelSubscription["target"];
    };
    const subscription = channelSubscriptions.find((item) => item.repository_id === Number(request.repository_id));
    if (!subscription?.last_update) throw new Error("release_conflict: Check updates again");
    const applied = subscription.last_update.items.filter((item) => item.state === "available").map((item) => item.id);
    const snapshot: ChannelUpdateSnapshot = {
      ...subscription.last_update,
      status: "up_to_date",
      acknowledgement_required: false,
      checked_at: new Date().toISOString(),
      items: subscription.last_update.items
        .filter((item) => item.state !== "notification")
        .map((item) => (applied.includes(item.id) ? { ...item, state: "applied" as const } : item)),
    };
    subscription.target = request.target ?? snapshot.target;
    subscription.last_update = snapshot;
    return { snapshot, applied_skill_ids: applied };
  },
  get_shared_channel_auto_update_state: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const subscription = channelSubscriptions.find((item) => item.repository_id === repositoryId);
    if (!subscription) throw new Error("subscription_not_found: Subscribe to this channel first");
    return subscription.auto_update;
  },
  set_shared_channel_auto_update_enabled: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const subscription = channelSubscriptions.find((item) => item.repository_id === repositoryId);
    if (!subscription) throw new Error("subscription_not_found: Subscribe to this channel first");
    const enabled = Boolean(args?.enabled);
    const hardPaused =
      subscription.auto_update.last_run?.status === "paused" &&
      subscription.auto_update.last_run.pauses.some(
        (pause) =>
          !pause.skill_id && ["permission_changed", "integrity_error", "unresolved_failure"].includes(pause.reason),
      );
    subscription.auto_update = {
      ...subscription.auto_update,
      enabled,
      next_check_at: enabled
        ? new Date().toISOString()
        : hardPaused
          ? null
          : new Date(Date.now() + 60 * 60 * 1000).toISOString(),
    };
    return subscription.auto_update;
  },
  run_shared_channel_auto_updates: () => {
    const now = new Date().toISOString();
    return channelSubscriptions.flatMap((subscription) => {
      const enabled = subscription.auto_update.enabled;
      const snapshot = subscription.last_update;
      const applied = enabled
        ? (snapshot?.items.filter((item) => item.state === "available").map((item) => item.id) ?? [])
        : [];
      const pauses =
        snapshot?.items
          .filter((item) => item.state === "notification" || item.state === "blocked" || item.state === "failed")
          .map((item) => ({
            skill_id: item.id,
            reason:
              item.state === "notification"
                ? ("new_skill_requires_review" as const)
                : item.state === "failed"
                  ? ("unresolved_failure" as const)
                  : item.block_reason === "removed_upstream"
                    ? ("removed_upstream" as const)
                    : (item.block_reason ?? "unresolved_failure"),
            detail: item.error,
          })) ?? [];
      const run: ChannelAutoUpdateRun = {
        started_at: now,
        completed_at: now,
        status: !enabled
          ? "checked"
          : applied.length > 0
            ? pauses.length > 0
              ? "partially_applied"
              : "applied"
            : pauses.length > 0
              ? "paused"
              : "up_to_date",
        target: snapshot?.target ?? subscription.target,
        applied_skill_ids: applied,
        pauses,
        error: null,
        retryable: false,
      };
      if (enabled && snapshot && applied.length > 0) {
        const fullyApplied = pauses.length === 0;
        subscription.last_update = {
          ...snapshot,
          status: fullyApplied ? "up_to_date" : "partially_upgraded",
          acknowledgement_required: !fullyApplied,
          checked_at: now,
          items: snapshot.items
            .filter((item) => !fullyApplied || item.state !== "notification")
            .map((item) => (applied.includes(item.id) ? { ...item, state: "applied" as const } : item)),
        };
        if (fullyApplied) subscription.target = snapshot.target;
      }
      subscription.auto_update = {
        enabled,
        next_check_at: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
        last_run: run,
      };
      return [{ repository_id: subscription.repository_id, run }];
    });
  },
};
