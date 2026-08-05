import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelSubscriptionRemoteStatus, ChannelUpdateSnapshot, SharedChannelDescriptor } from "../../../types";
import { SharedChannelsContent } from "./SharedChannelsContent";

const api = vi.hoisted(() => ({
  listOrganizations: vi.fn(),
  listChannels: vi.fn(),
  create: vi.fn(),
  resume: vi.fn(),
  listExisting: vi.fn(),
  scanExisting: vi.fn(),
  confirmExisting: vi.fn(),
  cancelExisting: vi.fn(),
  previewPublish: vi.fn(),
  publish: vi.fn(),
  cancelPublish: vi.fn(),
  listMembership: vi.fn(),
  inviteMember: vi.fn(),
  revokeMember: vi.fn(),
  cancelInvitation: vi.fn(),
  resendInvitation: vi.fn(),
  listInbox: vi.fn(),
  acceptInvitation: vi.fn(),
  declineInvitation: vi.fn(),
  resumeAccepted: vi.fn(),
  listSubscriptions: vi.fn(),
  reviewSubscription: vi.fn(),
  subscribe: vi.fn(),
  getUpdateState: vi.fn(),
  checkUpdate: vi.fn(),
  applyUpdate: vi.fn(),
  getAutoUpdateState: vi.fn(),
  setAutoUpdateEnabled: vi.fn(),
  runAutoUpdates: vi.fn(),
  listRollbackTargets: vi.fn(),
  rollbackSkill: vi.fn(),
  resumeFollowing: vi.fn(),
  uninstallRemoved: vi.fn(),
  convertRemoved: vi.fn(),
  installChannelSkill: vi.fn(),
  uninstallRevoked: vi.fn(),
  convertRevoked: vi.fn(),
}));

vi.mock("../api/channels", () => ({
  listSharedChannelOrganizations: api.listOrganizations,
  listSharedChannels: api.listChannels,
  createSharedChannel: api.create,
  resumeSharedChannel: api.resume,
  listExistingChannelRepositories: api.listExisting,
  scanExistingSharedChannel: api.scanExisting,
  confirmExistingSharedChannel: api.confirmExisting,
  cancelExistingSharedChannelRegistration: api.cancelExisting,
  previewSharedChannelPublish: api.previewPublish,
  publishSharedChannel: api.publish,
  cancelSharedChannelPublish: api.cancelPublish,
  listSharedChannelMembership: api.listMembership,
  inviteSharedChannelMember: api.inviteMember,
  revokeSharedChannelMember: api.revokeMember,
  cancelSharedChannelInvitation: api.cancelInvitation,
  resendSharedChannelInvitation: api.resendInvitation,
  listSharedChannelInvitationInbox: api.listInbox,
  acceptSharedChannelInvitation: api.acceptInvitation,
  declineSharedChannelInvitation: api.declineInvitation,
  resumeAcceptedSharedChannel: api.resumeAccepted,
  listSharedChannelSubscriptions: api.listSubscriptions,
  reviewSharedChannelSubscription: api.reviewSubscription,
  subscribeSharedChannel: api.subscribe,
  getSharedChannelUpdateState: api.getUpdateState,
  checkSharedChannelUpdate: api.checkUpdate,
  applySharedChannelUpdate: api.applyUpdate,
  getSharedChannelAutoUpdateState: api.getAutoUpdateState,
  setSharedChannelAutoUpdateEnabled: api.setAutoUpdateEnabled,
  runSharedChannelAutoUpdates: api.runAutoUpdates,
  listSharedChannelSkillRollbackTargets: api.listRollbackTargets,
  rollbackSharedChannelSkill: api.rollbackSkill,
  resumeSharedChannelSkillFollowing: api.resumeFollowing,
  uninstallRemovedSharedChannelSkill: api.uninstallRemoved,
  convertRemovedSharedChannelSkillToLocal: api.convertRemoved,
  installSharedChannelSkill: api.installChannelSkill,
  uninstallRevokedSharedChannelSkill: api.uninstallRevoked,
  convertRevokedSharedChannelSkillToLocal: api.convertRevoked,
}));

function channel(status: SharedChannelDescriptor["status"] = "active"): SharedChannelDescriptor {
  return {
    descriptor_version: 1,
    repository_id: 42,
    organization_id: 7,
    owner: "acme",
    name: "skillstar-shared",
    html_url: "https://github.com/acme/skillstar-shared",
    clone_url: "https://github.com/acme/skillstar-shared.git",
    role: "owner",
    status,
    authorization: {
      repository_selection: "selected",
      administration: "write",
      contents: "write",
    },
    created_at: "2026-08-05T00:00:00Z",
    updated_at: "2026-08-05T00:00:00Z",
  };
}

function existingChannel(): SharedChannelDescriptor {
  return {
    ...channel(),
    repository_id: 84,
    name: "existing-skills",
    html_url: "https://github.com/acme/existing-skills",
    clone_url: "https://github.com/acme/existing-skills.git",
  };
}

function existingPreview(sessionId: string) {
  return {
    session_id: sessionId,
    repository: {
      repository_id: 84,
      organization_id: 7,
      owner: "acme",
      name: "existing-skills",
      html_url: "https://github.com/acme/existing-skills",
      clone_url: "https://github.com/acme/existing-skills.git",
      role: "owner" as const,
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
}

function publicationPreview(sessionId: string) {
  return {
    session_id: sessionId,
    repository_id: 42,
    commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    next_revision: 1,
    tag_name: "channel-v000001",
    changes: [
      {
        id: "writer",
        content_root: "skills/writer",
        content_hash: "sha256:6e8b30c29c269c5375c2149f4834f8f6d289e5842b6d75f0f912749605a537f7",
        content_hash_version: 2,
        status: "added" as const,
      },
      {
        id: "legacy",
        content_root: "skills/legacy",
        content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        content_hash_version: 2,
        status: "removed" as const,
      },
    ],
  };
}

function subscriptionReview() {
  return {
    channel: { ...channel(), role: "subscriber" as const },
    target: {
      revision: 1,
      tag_name: "channel-v000001",
      commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    },
    title: "First release",
    notes: "Choose what belongs on this device.",
    publisher: { id: 99, login: "alice" },
    published_at: "2026-08-05T00:00:00Z",
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
        selected: true,
      },
      {
        id: "writer",
        content_root: "skills/writer",
        content_hash: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        content_hash_version: 2,
        selected: true,
      },
    ],
    read_only: false,
  };
}

function revokedSubscriptionView() {
  return {
    schema_version: 1,
    descriptor_version: 4,
    repository_id: 42,
    organization_id: 7,
    target: subscriptionReview().target,
    selected_skill_ids: ["writer"],
    auto_update: { enabled: false, next_check_at: null, last_run: null },
    remote_state: {
      status: "revoked" as const,
      checked_at: "2026-08-06T00:00:00Z",
      message: "GitHub repository access was revoked",
    },
    read_only: false,
  };
}

function frozenSubscriptionView(status: Exclude<ChannelSubscriptionRemoteStatus, "active" | "revoked">) {
  return {
    ...revokedSubscriptionView(),
    remote_state: {
      status,
      checked_at: "2026-08-06T00:00:00Z",
      message: `${status} detail`,
    },
  };
}

function updateSnapshot(overrides: Partial<ChannelUpdateSnapshot> = {}): ChannelUpdateSnapshot {
  return {
    target: {
      revision: 2,
      tag_name: "channel-v000002",
      commit_sha: "dddddddddddddddddddddddddddddddddddddddd",
    },
    title: "Second release",
    notes: "Safer prompts and a new optional Skill.",
    publisher: { id: 99, login: "alice" },
    published_at: "2026-08-06T00:00:00Z",
    checked_at: "2026-08-06T01:00:00Z",
    status: "update_available",
    acknowledgement_required: true,
    items: [
      {
        id: "newcomer",
        change: "added",
        state: "notification",
        selected: false,
        from_content_hash: null,
        to_content_hash: "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        block_reason: null,
        suggested_local_name: null,
        error: null,
      },
      {
        id: "writer",
        change: "updated",
        state: "available",
        selected: true,
        from_content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        to_content_hash: "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        block_reason: null,
        suggested_local_name: null,
        error: null,
      },
    ],
    check_error: null,
    ...overrides,
  };
}

describe("SharedChannelsContent", () => {
  beforeEach(() => {
    api.listOrganizations
      .mockReset()
      .mockResolvedValue([{ id: 7, login: "acme", avatar_url: null, viewer_is_admin: true }]);
    api.listChannels.mockReset().mockResolvedValue([]);
    api.create.mockReset().mockResolvedValue(channel());
    api.resume.mockReset().mockResolvedValue(channel());
    api.listExisting.mockReset().mockResolvedValue([
      {
        repository_id: 84,
        organization_id: 7,
        owner: "acme",
        name: "existing-skills",
        html_url: "https://github.com/acme/existing-skills",
        clone_url: "https://github.com/acme/existing-skills.git",
        role: "owner",
        already_registered: false,
      },
    ]);
    api.scanExisting
      .mockReset()
      .mockImplementation((_request, sessionId: string) => Promise.resolve(existingPreview(sessionId)));
    api.confirmExisting.mockReset().mockResolvedValue(existingChannel());
    api.cancelExisting.mockReset().mockResolvedValue(true);
    api.previewPublish
      .mockReset()
      .mockImplementation((_repositoryId, sessionId: string) => Promise.resolve(publicationPreview(sessionId)));
    api.publish.mockReset().mockImplementation((sessionId: string, title: string, notes: string) =>
      Promise.resolve({
        manifest: {
          schema_version: 1,
          repository_id: 42,
          organization_id: 7,
          revision: 1,
          tag_name: "channel-v000001",
          commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          publisher: { id: 99, login: "alice" },
          published_at: "2026-08-05T00:00:00Z",
          title,
          notes,
          skills: publicationPreview(sessionId).changes,
        },
        release: {
          id: 501,
          html_url: "https://github.com/acme/skillstar-shared/releases/tag/channel-v000001",
        },
      }),
    );
    api.cancelPublish.mockReset().mockResolvedValue(true);
    api.listMembership.mockReset().mockResolvedValue({
      repository_id: 42,
      members: [
        {
          user: { id: 7, login: "alice" },
          role: "owner",
          github_role_name: "admin",
          status: "accepted",
        },
      ],
      invitations: [],
    });
    api.inviteMember.mockReset().mockResolvedValue({
      repository_id: 42,
      invitation_id: 91,
      username: "bob",
      role: "subscriber",
      status: "pending",
    });
    api.revokeMember.mockReset().mockResolvedValue({
      repository_id: 42,
      username: "bob",
      status: "revoked",
      effective_role: null,
      access_source: null,
    });
    api.cancelInvitation.mockReset().mockResolvedValue({
      repository_id: 42,
      invitation_id: 91,
      username: "bob",
      role: "subscriber",
      status: "cancelled",
    });
    api.resendInvitation.mockReset().mockResolvedValue({
      repository_id: 42,
      invitation_id: 92,
      username: "bob",
      role: "subscriber",
      status: "pending",
    });
    api.listInbox.mockReset().mockResolvedValue([]);
    api.acceptInvitation.mockReset();
    api.declineInvitation.mockReset().mockResolvedValue({
      repository_id: 84,
      invitation_id: 901,
      username: "demo-user",
      role: "subscriber",
      status: "cancelled",
    });
    api.resumeAccepted.mockReset();
    api.listSubscriptions.mockReset().mockResolvedValue([]);
    api.reviewSubscription.mockReset().mockResolvedValue(subscriptionReview());
    api.subscribe.mockReset().mockImplementation((request) =>
      Promise.resolve({
        descriptor_version: 4,
        repository_id: request.repository_id,
        organization_id: 7,
        target: subscriptionReview().target,
        skills: request.selected_skill_ids.map((id: string) => ({
          id,
          content_root: `skills/${id}`,
          release_content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          release_content_hash_version: 2,
          baseline_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          baseline_hash_version: 2,
          provenance: {
            repository_id: 42,
            repository_url: "https://github.com/acme/skillstar-shared.git",
            git_ref: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            source_folder: `skills/${id}`,
          },
        })),
        known_skill_ids: subscriptionReview().skills.map((skill) => skill.id),
        pins: [],
        last_update: null,
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        remote_state: { status: "active", checked_at: null, message: null },
        created_at: "2026-08-05T00:00:00Z",
        updated_at: "2026-08-05T00:00:00Z",
      }),
    );
    api.getUpdateState.mockReset().mockResolvedValue(null);
    api.checkUpdate.mockReset().mockResolvedValue(
      updateSnapshot({
        status: "up_to_date",
        items: [
          {
            id: "writer",
            change: "unchanged",
            state: "current",
            selected: true,
            from_content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            to_content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            block_reason: null,
            suggested_local_name: null,
            error: null,
          },
        ],
      }),
    );
    api.applyUpdate.mockReset().mockResolvedValue({
      snapshot: updateSnapshot(),
      applied_skill_ids: ["writer"],
    });
    api.getAutoUpdateState.mockReset().mockResolvedValue({ enabled: false, next_check_at: null, last_run: null });
    api.setAutoUpdateEnabled.mockReset().mockImplementation((_repositoryId, enabled) =>
      Promise.resolve({
        enabled,
        next_check_at: enabled ? "2026-08-06T02:00:00Z" : null,
        last_run: null,
      }),
    );
    api.runAutoUpdates.mockReset().mockResolvedValue([]);
    api.listRollbackTargets.mockReset().mockResolvedValue([
      {
        target: {
          revision: 1,
          tag_name: "channel-v000001",
          commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        },
        title: "First release",
        published_at: "2026-08-05T00:00:00Z",
        content_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      },
    ]);
    api.rollbackSkill.mockReset().mockImplementation((request) => {
      const pinned = updateSnapshot({
        items: updateSnapshot().items.map((item) =>
          item.id === request.skill_id ? { ...item, pinned_target: request.target } : item,
        ),
      });
      return Promise.resolve({
        snapshot: pinned,
        pin: { skill_id: request.skill_id, target: request.target },
      });
    });
    api.resumeFollowing.mockReset().mockResolvedValue(updateSnapshot());
    api.uninstallRemoved.mockReset().mockResolvedValue({
      skill_id: "writer",
      local_name: null,
      snapshot: updateSnapshot({ status: "up_to_date", acknowledgement_required: false, items: [] }),
    });
    api.convertRemoved.mockReset().mockResolvedValue({
      skill_id: "writer",
      local_name: "writer.local",
      snapshot: updateSnapshot({ status: "up_to_date", acknowledgement_required: false, items: [] }),
    });
    api.installChannelSkill.mockReset().mockResolvedValue({
      subscription: {},
      snapshot: updateSnapshot(),
    });
    api.uninstallRevoked.mockReset();
    api.convertRevoked.mockReset();
  });

  it("moves from the create wizard into an empty channel detail with role and scope", async () => {
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Authorization boundary")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Review GitHub App installation" })).toHaveAttribute(
      "href",
      "https://github.com/organizations/acme/settings/installations",
    );
    fireEvent.click(screen.getByRole("button", { name: "Create private repository" }));

    await waitFor(() => expect(api.create).toHaveBeenCalled());
    expect(await screen.findByText("owner")).toBeInTheDocument();
    expect(screen.getByText("GitHub App scope: selected repository only.")).toBeInTheDocument();
    expect(screen.getByText("Channel ready — normal commits remain drafts.")).toBeInTheDocument();
  });

  it("retries a persisted pending repository by numeric repository id", async () => {
    api.listChannels.mockResolvedValue([channel("awaiting_app_installation")]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByRole("link", { name: "Open GitHub App settings" })).toHaveAttribute(
      "href",
      "https://github.com/organizations/acme/settings/installations",
    );
    fireEvent.click(await screen.findByRole("button", { name: "Retry authorization" }));

    await waitFor(() => expect(api.resume).toHaveBeenCalledWith(42));
    expect(await screen.findByText("Channel ready — normal commits remain drafts.")).toBeInTheDocument();
  });

  it("previews every exposure category before confirming an existing repository", async () => {
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("tab", { name: "Existing repository" }));
    await waitFor(() => expect(api.listExisting).toHaveBeenCalledWith(7));
    fireEvent.click(screen.getByRole("button", { name: "Scan exposure" }));

    expect(await screen.findByText("Complete repository exposure")).toBeInTheDocument();
    expect(screen.getByText(/complete Git history/)).toBeInTheDocument();
    expect(screen.getByText("writer · skills/writer")).toBeInTheDocument();
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText(".github/workflows/ci.yml")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", {
        name: "Confirm complete exposure and register",
      }),
    );

    await waitFor(() => expect(api.confirmExisting).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("heading", { name: "acme/existing-skills" })).toBeInTheDocument();
  });

  it("keeps the same preview when confirmation fails so the user can retry", async () => {
    api.confirmExisting.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(existingChannel());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Existing repository" }));
    fireEvent.click(await screen.findByRole("button", { name: "Scan exposure" }));
    const confirm = await screen.findByRole("button", {
      name: "Confirm complete exposure and register",
    });

    fireEvent.click(confirm);
    expect(await screen.findByText("offline")).toBeInTheDocument();
    expect(screen.getByText("Complete repository exposure")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: "Confirm complete exposure and register",
      }),
    );

    await waitFor(() => expect(api.confirmExisting).toHaveBeenCalledTimes(2));
    const firstSession = api.confirmExisting.mock.calls[0][0];
    expect(api.confirmExisting.mock.calls[1][0]).toBe(firstSession);
    expect(await screen.findByRole("heading", { name: "acme/existing-skills" })).toBeInTheDocument();
  });

  it("discards a scan response that arrives after cancellation", async () => {
    let finishScan: ((value: ReturnType<typeof existingPreview>) => void) | undefined;
    api.scanExisting.mockImplementation(
      () =>
        new Promise((resolve) => {
          finishScan = resolve;
        }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Existing repository" }));
    fireEvent.click(await screen.findByRole("button", { name: "Scan exposure" }));
    await waitFor(() => expect(api.scanExisting).toHaveBeenCalledTimes(1));
    const sessionId = api.scanExisting.mock.calls[0][1];

    fireEvent.click(screen.getByRole("button", { name: "Cancel scan" }));
    finishScan?.(existingPreview(sessionId));

    await waitFor(() => expect(api.cancelExisting).toHaveBeenCalledWith(sessionId));
    expect(screen.queryByText("Complete repository exposure")).not.toBeInTheDocument();
  });

  it("previews every publication change and sends title and notes only on explicit publish", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.change(await screen.findByLabelText("Release title"), {
      target: { value: "Writing tools" },
    });
    fireEvent.change(screen.getByLabelText("Release notes"), {
      target: { value: "First stable version" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Preview publication" }));

    expect(await screen.findByText("writer · skills/writer")).toBeInTheDocument();
    expect(screen.getByText("legacy · skills/legacy")).toBeInTheDocument();
    expect(screen.getByText("1 added")).toBeInTheDocument();
    expect(screen.getByText("1 removed")).toBeInTheDocument();
    expect(api.publish).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Publish channel-v000001" }));
    await waitFor(() =>
      expect(api.publish).toHaveBeenCalledWith(expect.any(String), "Writing tools", "First stable version"),
    );
    expect(await screen.findByText("Published channel-v000001")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open GitHub Release" })).toHaveAttribute(
      "href",
      "https://github.com/acme/skillstar-shared/releases/tag/channel-v000001",
    );
  });

  it("keeps an exact-commit publication preview retryable after GitHub rejection", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.publish.mockRejectedValueOnce(new Error("workflow authorization is not granted"));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.change(await screen.findByLabelText("Release title"), {
      target: { value: "Writing tools" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Preview publication" }));
    const publishButton = await screen.findByRole("button", {
      name: "Publish channel-v000001",
    });
    fireEvent.click(publishButton);

    expect(await screen.findByText("workflow authorization is not granted")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Publish channel-v000001" })).toBeInTheDocument();
    expect(api.previewPublish).toHaveBeenCalledTimes(1);
  });

  it("lets an owner invite a publisher and shows GitHub membership as the source of truth", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.inviteMember.mockResolvedValueOnce({
      repository_id: 42,
      invitation_id: 91,
      username: "bob",
      role: "publisher",
      status: "pending",
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Members and invitations")).toBeInTheDocument();
    expect(
      screen.getByText("GitHub is the source of truth, including team and organization access."),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("GitHub username"), {
      target: { value: "bob" },
    });
    fireEvent.change(screen.getByLabelText("Channel role"), {
      target: { value: "publisher" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Invite" }));

    await waitFor(() =>
      expect(api.inviteMember).toHaveBeenCalledWith({
        repository_id: 42,
        username: "bob",
        role: "publisher",
      }),
    );
    expect(await screen.findByText("bob: pending")).toBeInTheDocument();
  });

  it("shows a failed member invitation without inventing remote membership state", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.inviteMember.mockRejectedValueOnce(new Error("Organization policy blocks this collaborator"));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    await screen.findByText("Members and invitations");
    fireEvent.change(screen.getByLabelText("GitHub username"), { target: { value: "bob" } });
    fireEvent.click(screen.getByRole("button", { name: "Invite" }));

    expect(await screen.findByText("bob: failed")).toBeInTheDocument();
    expect(screen.getByText("Organization policy blocks this collaborator")).toBeInTheDocument();
    expect(screen.getByLabelText("GitHub username")).toHaveValue("bob");
  });

  it("removes only direct access and explains inherited GitHub access that remains", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.listMembership.mockResolvedValue({
      repository_id: 42,
      members: [
        {
          user: { id: 8, login: "bob" },
          role: "subscriber",
          github_role_name: "read",
          status: "accepted",
        },
      ],
      invitations: [],
    });
    api.revokeMember.mockResolvedValueOnce({
      repository_id: 42,
      username: "bob",
      status: "access_remains",
      effective_role: "subscriber",
      access_source: "inherited",
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Remove direct access for @bob" }));

    await waitFor(() => expect(api.revokeMember).toHaveBeenCalledWith(42, "bob"));
    expect(await screen.findByText(/still has effective subscriber access through GitHub/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Review effective access on GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/acme/skillstar-shared/settings/access",
    );
  });

  it("confirms when a removed direct member has no effective GitHub access left", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.listMembership.mockResolvedValue({
      repository_id: 42,
      members: [
        {
          user: { id: 8, login: "bob" },
          role: "subscriber",
          github_role_name: "read",
          status: "accepted",
        },
      ],
      invitations: [],
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Remove direct access for @bob" }));

    expect(await screen.findByText("@bob no longer has effective GitHub access.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Review effective access on GitHub" })).not.toBeInTheDocument();
  });

  it("freezes a revoked subscription while keeping uninstall and convert-to-local actions", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockRejectedValueOnce({
      code: "subscription_access_revoked",
      message: "GitHub repository access was revoked",
    });
    api.listSubscriptions.mockResolvedValue([revokedSubscriptionView()]);
    api.convertRevoked.mockResolvedValue({
      skill_id: "writer",
      local_name: "writer.archive.local",
      subscription: null,
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Remote access revoked; installed Skills are preserved.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall writer" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Local copy name for writer"), {
      target: { value: "writer.archive.local" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Keep writer as local copy" }));

    await waitFor(() =>
      expect(api.convertRevoked).toHaveBeenCalledWith({
        repository_id: 42,
        skill_id: "writer",
        local_name: "writer.archive.local",
      }),
    );
  });

  it("uninstalls a frozen channel Skill only after the subscriber chooses it", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockRejectedValue({
      code: "subscription_access_revoked",
      message: "GitHub repository access was revoked",
    });
    api.listSubscriptions.mockResolvedValue([revokedSubscriptionView()]);
    api.uninstallRevoked.mockResolvedValue({ skill_id: "writer", local_name: null, subscription: null });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Uninstall writer" }));

    await waitFor(() => expect(api.uninstallRevoked).toHaveBeenCalledWith(42, "writer"));
    expect(screen.getByText("No channel Skills remain tracked.")).toBeInTheDocument();
  });

  it("lets a frozen subscriber explicitly recheck restored repository access", async () => {
    const restored = {
      ...revokedSubscriptionView(),
      remote_state: { status: "active" as const, checked_at: "2026-08-05T03:00:00Z", message: null },
    };
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockRejectedValueOnce({
      code: "subscription_access_revoked",
      message: "GitHub repository access was revoked",
    });
    api.listSubscriptions.mockResolvedValueOnce([revokedSubscriptionView()]).mockResolvedValue([restored]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Check restored GitHub access" }));

    await waitFor(() => expect(api.checkUpdate).toHaveBeenCalledWith(42));
    await waitFor(() =>
      expect(screen.queryByText("Remote access revoked; installed Skills are preserved.")).not.toBeInTheDocument(),
    );
    expect(await screen.findByText("Review published release")).toBeInTheDocument();
    expect(screen.queryByText("GitHub repository access was revoked")).not.toBeInTheDocument();
  });

  it("refreshes stale local revoked state after release review proves access is restored", async () => {
    const restored = {
      ...revokedSubscriptionView(),
      remote_state: { status: "active" as const, checked_at: "2026-08-05T03:00:00Z", message: null },
    };
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValueOnce([revokedSubscriptionView()]).mockResolvedValue([restored]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Review published release")).toBeInTheDocument();
    await waitFor(() => expect(api.listSubscriptions.mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(screen.queryByText("Remote access revoked; installed Skills are preserved.")).not.toBeInTheDocument();
  });

  it.each([
    ["offline", "Shared channel is offline"],
    ["recoverable_failure", "Shared channel check needs attention"],
    ["integrity_error", "Shared channel integrity check failed"],
  ] as const)("freezes %s subscriptions while retaining local content and only offering retry", async (status, title) => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([frozenSubscriptionView(status)]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(title)).toBeInTheDocument();
    expect(screen.getByText(/installed Skills and deployments remain unchanged/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry remote validation" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Apply safe updates" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Uninstall writer" })).not.toBeInTheDocument();
    expect(api.getUpdateState).not.toHaveBeenCalled();
  });

  it("reloads the persisted frozen state when review fails after a stale active list response", async () => {
    const staleActive = {
      ...frozenSubscriptionView("offline"),
      remote_state: { status: "active" as const, checked_at: null, message: null },
    };
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValueOnce([staleActive]).mockResolvedValue([frozenSubscriptionView("offline")]);
    api.reviewSubscription.mockRejectedValueOnce({ code: "network", message: "offline release review" });

    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Shared channel is offline")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Apply safe updates" })).not.toBeInTheDocument();
    expect(api.getUpdateState).not.toHaveBeenCalled();
  });

  it("returns an offline subscription to the normal update flow after validation recovers", async () => {
    const frozen = frozenSubscriptionView("offline");
    const restored = {
      ...frozen,
      remote_state: { status: "active" as const, checked_at: "2026-08-06T01:00:00Z", message: null },
    };
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValueOnce([frozen]).mockResolvedValueOnce([frozen]).mockResolvedValue([restored]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Retry remote validation" }));

    await waitFor(() => expect(api.checkUpdate).toHaveBeenCalledWith(42));
    await waitFor(() => expect(screen.queryByText("Shared channel is offline")).not.toBeInTheDocument());
    expect(await screen.findByRole("region", { name: "Channel updates" })).toBeInTheDocument();
  });

  it("supports the explicit non-atomic re-invite flow and cancellation", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.listMembership.mockResolvedValue({
      repository_id: 42,
      members: [],
      invitations: [
        {
          id: 91,
          repository_id: 42,
          organization_id: 7,
          owner: "acme",
          repository_name: "skillstar-shared",
          html_url: "https://github.com/acme/skillstar-shared",
          invitee: { id: 8, login: "bob" },
          inviter: { id: 7, login: "alice" },
          role: "subscriber",
          effective_role: "subscriber",
          status: "pending",
          created_at: "2026-08-05T00:00:00Z",
        },
      ],
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Re-invite" }));
    await waitFor(() => expect(api.resendInvitation).toHaveBeenCalledWith(42, 91));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(api.cancelInvitation).toHaveBeenCalledWith(42, 91));
  });

  it("refreshes GitHub truth when re-invite fails after cancelling the old invitation", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.listMembership
      .mockResolvedValueOnce({
        repository_id: 42,
        members: [],
        invitations: [
          {
            id: 91,
            repository_id: 42,
            organization_id: 7,
            owner: "acme",
            repository_name: "skillstar-shared",
            html_url: "https://github.com/acme/skillstar-shared",
            invitee: { id: 8, login: "bob" },
            inviter: { id: 7, login: "alice" },
            role: "subscriber",
            effective_role: "subscriber",
            status: "pending",
            created_at: "2026-08-05T00:00:00Z",
          },
        ],
      })
      .mockResolvedValue({ repository_id: 42, members: [], invitations: [] });
    api.resendInvitation.mockRejectedValueOnce(new Error("GitHub rejected the replacement invitation"));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Re-invite" }));

    expect(await screen.findByText("No pending invitations.")).toBeInTheDocument();
    expect(screen.getByText("GitHub rejected the replacement invitation")).toBeInTheDocument();
  });

  it("does not render or query member management for a non-owner", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "publisher" }]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("publisher")).toBeInTheDocument();
    expect(screen.queryByText("Members and invitations")).not.toBeInTheDocument();
    expect(api.listMembership).not.toHaveBeenCalled();
  });

  it("hides stale owner controls after GitHub reports the user is no longer an admin", async () => {
    api.listChannels.mockResolvedValue([channel()]);
    api.listMembership.mockRejectedValue({
      code: "permission_denied",
      message: "Current GitHub Admin permission is required",
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByRole("heading", { name: "acme/skillstar-shared" })).toBeInTheDocument();
    await waitFor(() => expect(api.listMembership).toHaveBeenCalledWith(42));
    expect(screen.queryByText("Members and invitations")).not.toBeInTheDocument();
  });

  it("accepts a GitHub repository invitation and automatically opens the imported channel", async () => {
    const invitation = {
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
      created_at: "2026-08-05T00:00:00Z",
    };
    const accepted = {
      ...channel(),
      repository_id: 84,
      name: "design-skills",
      html_url: invitation.html_url,
      clone_url: `${invitation.html_url}.git`,
      role: "subscriber",
    };
    api.listInbox.mockResolvedValue([invitation]);
    api.acceptInvitation.mockResolvedValue(accepted);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Invitation inbox" }));
    expect(await screen.findByText("acme/design-skills")).toBeInTheDocument();
    expect(screen.getByText("Invited by @alice · subscriber · pending")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Accept and import" }));

    await waitFor(() => expect(api.acceptInvitation).toHaveBeenCalledWith(901));
    expect(await screen.findByRole("heading", { name: "acme/design-skills" })).toBeInTheDocument();
    expect(screen.getByText("subscriber")).toBeInTheDocument();
  });

  it("declines an inbox invitation and shows the cancelled state", async () => {
    api.listInbox.mockResolvedValue([
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
        created_at: "2026-08-05T00:00:00Z",
      },
    ]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Invitation inbox" }));
    fireEvent.click(await screen.findByRole("button", { name: "Decline" }));

    await waitFor(() => expect(api.declineInvitation).toHaveBeenCalledWith(901));
    expect(await screen.findByText("acme/design-skills: cancelled")).toBeInTheDocument();
  });

  it("refreshes local channels when GitHub accepted but the final import save needs recovery", async () => {
    const invitation = {
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
      created_at: "2026-08-05T00:00:00Z",
    };
    const pending = {
      ...channel("awaiting_invitation_acceptance"),
      repository_id: 84,
      name: "design-skills",
      role: "subscriber" as const,
    };
    api.listChannels.mockResolvedValueOnce([]).mockResolvedValue([pending]);
    api.listInbox.mockResolvedValue([invitation]);
    api.acceptInvitation.mockRejectedValue(new Error("Retry accepted invitation import"));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Invitation inbox" }));
    fireEvent.click(await screen.findByRole("button", { name: "Accept and import" }));

    await waitFor(() => expect(api.listChannels).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Accepted invitation import pending")).toBeInTheDocument();
  });

  it("retries a locally pending import after GitHub already accepted the invitation", async () => {
    const pending = { ...channel("awaiting_invitation_acceptance"), role: "subscriber" as const };
    const active = { ...pending, status: "active" as const };
    api.listChannels.mockResolvedValue([pending]);
    api.resumeAccepted.mockResolvedValue(active);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("GitHub accepted this invitation")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry accepted invitation import" }));

    await waitFor(() => expect(api.resumeAccepted).toHaveBeenCalledWith(42));
    expect(await screen.findByText("Channel ready — normal commits remain drafts.")).toBeInTheDocument();
  });

  it("reviews a subscriber release with every Skill selected and installs only the retained selection", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockResolvedValue({
      ...subscriptionReview(),
      channel: {
        ...subscriptionReview().channel,
        owner: "renamed-acme",
        name: "renamed-channel",
        html_url: "https://github.com/renamed-acme/renamed-channel",
      },
    });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("First release")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "renamed-acme/renamed-channel" })).toHaveAttribute(
      "href",
      "https://github.com/renamed-acme/renamed-channel",
    );
    expect(screen.getByText("Choose what belongs on this device.")).toBeInTheDocument();
    expect(screen.getByText(/2026-08-05 00:00:00 UTC/)).toBeInTheDocument();
    expect(screen.getByText(/complete contents and full Git history/)).toBeInTheDocument();
    const reader = screen.getByRole("checkbox", { name: /reader/ });
    const writer = screen.getByRole("checkbox", { name: /writer/ });
    expect(reader).toBeChecked();
    expect(writer).toBeChecked();
    fireEvent.click(reader);
    fireEvent.click(screen.getByRole("button", { name: "Install selected & subscribe" }));
    expect(writer).toBeDisabled();

    await waitFor(() =>
      expect(api.subscribe).toHaveBeenCalledWith(
        {
          repository_id: 42,
          target: subscriptionReview().target,
          selected_skill_ids: ["writer"],
        },
        expect.any(String),
      ),
    );
    expect(await screen.findByText(/Subscribed to revision 1 with 1 selected Skills/)).toBeInTheDocument();
  });

  it("restores a persisted subscription without silently selecting a newly published Skill", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockResolvedValue({
      ...subscriptionReview(),
      skills: subscriptionReview().skills.map((skill) => ({
        ...skill,
        selected: skill.id === "writer",
      })),
    });
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 1,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/Subscribed to revision 1 with 1 selected Skills/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /reader/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /writer/ })).toBeChecked();
    expect(screen.queryByRole("button", { name: "Install selected & subscribe" })).not.toBeInTheDocument();
  });

  it("shows an unknown subscription schema read-only and never attempts installation", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.reviewSubscription.mockResolvedValue({ ...subscriptionReview(), read_only: true });
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 99,
        descriptor_version: 4,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["reader", "writer"],
        read_only: true,
      },
    ]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/created by a newer SkillStar schema/)).toBeInTheDocument();
    expect(screen.getAllByRole("checkbox").every((checkbox) => checkbox.hasAttribute("disabled"))).toBe(true);
    expect(api.subscribe).not.toHaveBeenCalled();
  });

  it("keeps the reviewed selection and actionable error when installation rolls back", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.subscribe.mockRejectedValue(new Error("Installed Skills were rolled back"));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    const reader = await screen.findByRole("checkbox", { name: /reader/ });
    fireEvent.click(reader);
    fireEvent.click(screen.getByRole("button", { name: "Install selected & subscribe" }));

    expect(await screen.findByText("Installed Skills were rolled back")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /reader/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /writer/ })).toBeChecked();
    expect(api.listSubscriptions).toHaveBeenCalledTimes(2);
  });

  it("shows release diffs and never applies a channel update without an explicit click", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(updateSnapshot());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/Second release · published by @alice/)).toBeInTheDocument();
    expect(screen.getByText("2026-08-06 00:00:00 UTC")).toBeInTheDocument();
    expect(screen.getByText("Safer prompts and a new optional Skill.")).toBeInTheDocument();
    expect(screen.getByText("1 added")).toBeInTheDocument();
    expect(screen.getByText("1 updated")).toBeInTheDocument();
    expect(screen.getByText(/Last checked:/)).toBeInTheDocument();
    expect(screen.getByText(/was not selected or installed/)).toBeInTheDocument();
    expect(api.applyUpdate).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Apply safe updates" }));
    await waitFor(() =>
      expect(api.applyUpdate).toHaveBeenCalledWith(
        {
          repository_id: 42,
          target: updateSnapshot().target,
          resolutions: [],
        },
        expect.any(String),
      ),
    );
  });

  it("replaces update actions with the persisted freeze state when apply detects tampering", async () => {
    const active = {
      ...frozenSubscriptionView("integrity_error"),
      remote_state: { status: "active" as const, checked_at: null, message: null },
    };
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions
      .mockResolvedValueOnce([active])
      .mockResolvedValueOnce([active])
      .mockResolvedValue([frozenSubscriptionView("integrity_error")]);
    api.checkUpdate.mockResolvedValue(updateSnapshot());
    api.applyUpdate.mockRejectedValueOnce({ code: "integrity", message: "release content was tampered" });
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Apply safe updates" }));

    expect(await screen.findByText("Shared channel integrity check failed")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Apply safe updates" })).not.toBeInTheDocument();
    expect(screen.getByText(/installed Skills and deployments remain unchanged/i)).toBeInTheDocument();
  });

  it("allows an unchanged release to be acknowledged explicitly", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: [],
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "update_available",
        acknowledgement_required: true,
        items: [],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/This release has no Skill content changes/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Acknowledge release" }));

    await waitFor(() => expect(api.applyUpdate).toHaveBeenCalled());
  });

  it("lets a subscriber select a verified historical release and pin the rolled-back Skill", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(updateSnapshot());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "View history for writer" }));
    expect(await screen.findByRole("combobox", { name: "Historical release for writer" })).toHaveValue("1");
    fireEvent.click(screen.getByRole("button", { name: "Roll back writer" }));

    await waitFor(() =>
      expect(api.rollbackSkill).toHaveBeenCalledWith(
        {
          repository_id: 42,
          skill_id: "writer",
          target: {
            revision: 1,
            tag_name: "channel-v000001",
            commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          },
          resolution: null,
        },
        expect.any(String),
      ),
    );
    expect(await screen.findByText("Pinned to revision 1")).toBeVisible();
  });

  it("keeps a removed channel Skill installed and converts it to an editable local copy", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            change: "removed",
            state: "removed_from_channel",
            to_content_hash: null,
            block_reason: "removed_upstream",
            suggested_local_name: "writer.local",
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/no longer in the channel.*installed copy is unchanged/i)).toBeVisible();
    const localName = screen.getByRole("textbox", { name: "Local copy name for writer" });
    expect(localName).toHaveValue("writer.local");
    fireEvent.change(localName, { target: { value: "writer.notes.local" } });
    fireEvent.click(screen.getByRole("button", { name: "Convert writer to local" }));

    await waitFor(() =>
      expect(api.convertRemoved).toHaveBeenCalledWith({
        repository_id: 42,
        skill_id: "writer",
        local_name: "writer.notes.local",
      }),
    );
  });

  it("uninstalls a removed channel Skill only after an explicit choice", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            change: "removed",
            state: "removed_from_channel",
            to_content_hash: null,
            block_reason: "removed_upstream",
            suggested_local_name: "writer.local",
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByRole("button", { name: "Uninstall writer" })).toBeEnabled();
    expect(api.uninstallRemoved).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Uninstall writer" }));
    await waitFor(() => expect(api.uninstallRemoved).toHaveBeenCalledWith(42, "writer"));
  });

  it("requires an explicit install before tracking a reintroduced channel Skill", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(updateSnapshot());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    const install = await screen.findByRole("button", { name: "Install and track newcomer" });
    expect(api.installChannelSkill).not.toHaveBeenCalled();
    fireEvent.click(install);
    await waitFor(() => expect(api.installChannelSkill).toHaveBeenCalledWith(42, "newcomer", expect.any(String)));
  });

  it("offers verified history when a subscribed Skill was removed upstream", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            change: "removed",
            state: "removed_from_channel",
            to_content_hash: null,
            block_reason: "removed_upstream",
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "View history for writer" }));
    expect(await screen.findByRole("combobox", { name: "Historical release for writer" })).toHaveValue("1");
    expect(api.listRollbackTargets).toHaveBeenCalledWith(42, "writer");
  });

  it("shows a persisted pin and resumes following the newest channel release", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        items: [
          {
            ...updateSnapshot().items[1],
            pinned_target: {
              revision: 1,
              tag_name: "channel-v000001",
              commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            },
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Pinned to revision 1")).toBeVisible();
    expect(screen.getByRole("button", { name: "Apply safe updates" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Resume following writer" }));

    await waitFor(() => expect(api.resumeFollowing).toHaveBeenCalledWith(42, "writer"));
    await waitFor(() => expect(screen.queryByText("Pinned to revision 1")).not.toBeInTheDocument());
  });

  it("requires resume before resolving or applying a pinned divergent Skill", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            state: "blocked",
            block_reason: "local_content_changed",
            suggested_local_name: "writer.local",
            pinned_target: {
              revision: 1,
              tag_name: "channel-v000001",
              commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            },
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Pinned to revision 1")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Preserve as .local" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Discard changes" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply safe updates" })).toBeDisabled();
  });

  it("clears a local divergence choice after rollback and resume", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            state: "blocked",
            block_reason: "local_content_changed",
            suggested_local_name: "writer.local",
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Discard changes" }));
    fireEvent.click(screen.getByRole("button", { name: "View history for writer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Roll back writer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Resume following writer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Apply safe updates" }));

    await waitFor(() =>
      expect(api.applyUpdate).toHaveBeenCalledWith(expect.objectContaining({ resolutions: [] }), expect.any(String)),
    );
  });

  it("offers preserve-as-local and discard choices for a divergent subscribed Skill", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    api.checkUpdate.mockResolvedValue(
      updateSnapshot({
        status: "blocked",
        items: [
          {
            ...updateSnapshot().items[1],
            state: "blocked",
            block_reason: "local_content_changed",
            suggested_local_name: "writer.local",
          },
        ],
      }),
    );
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    const name = await screen.findByRole("textbox", { name: "Local copy name for writer" });
    expect(screen.getByRole("button", { name: "Apply safe updates" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Preserve as .local" }));
    fireEvent.change(name, { target: { value: "writer.notes.local" } });
    expect(screen.getByRole("button", { name: "Discard changes" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Apply safe updates" }));

    await waitFor(() =>
      expect(api.applyUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          resolutions: [
            {
              skill_id: "writer",
              resolution: { kind: "preserve", local_name: "writer.notes.local" },
            },
          ],
        }),
        expect.any(String),
      ),
    );
  });

  it("clears a stale local resolution after checking the channel again", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    api.checkUpdate
      .mockResolvedValueOnce(
        updateSnapshot({
          status: "blocked",
          items: [
            {
              ...updateSnapshot().items[1],
              state: "blocked",
              block_reason: "local_content_changed",
              suggested_local_name: "writer.local",
            },
          ],
        }),
      )
      .mockResolvedValueOnce(updateSnapshot());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Preserve as .local" }));
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(api.checkUpdate).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole("button", { name: "Apply safe updates" }));

    await waitFor(() =>
      expect(api.applyUpdate).toHaveBeenCalledWith(expect.objectContaining({ resolutions: [] }), expect.any(String)),
    );
  });

  it("keeps the last verified update visible while offline and supports retry", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    api.getUpdateState.mockResolvedValue(updateSnapshot());
    api.checkUpdate
      .mockResolvedValueOnce(updateSnapshot({ check_error: "offline" }))
      .mockResolvedValueOnce(updateSnapshot({ check_error: null }));
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(
      await screen.findByText(/Showing the last verified result because this check failed: offline/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply safe updates" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(api.checkUpdate).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.queryByText(/Showing the last verified result because this check failed/)).not.toBeInTheDocument(),
    );
  });

  it("shows persisted subscription updates when release review is offline after restart", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        read_only: false,
      },
    ]);
    api.reviewSubscription.mockRejectedValue({ code: "network", message: "offline release review" });
    api.getUpdateState.mockResolvedValue(updateSnapshot());
    api.checkUpdate.mockResolvedValue(updateSnapshot({ check_error: "offline" }));

    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/The local subscription is still available with 1 selected Skills/)).toBeVisible();
    expect(await screen.findByText("Shared channel is offline")).toBeVisible();
    expect(screen.getByRole("button", { name: "Retry remote validation" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Apply safe updates" })).not.toBeInTheDocument();
    expect(api.getUpdateState).not.toHaveBeenCalled();
  });

  it("opts into protected automatic upgrades and shows partial results with pause reasons", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: false, next_check_at: null, last_run: null },
        read_only: false,
      },
    ]);
    const run = {
      started_at: "2026-08-06T02:00:00Z",
      completed_at: "2026-08-06T02:00:01Z",
      status: "partially_applied" as const,
      target: updateSnapshot().target,
      applied_skill_ids: ["writer"],
      pauses: [{ skill_id: "newcomer", reason: "new_skill_requires_review" as const, detail: null }],
      error: null,
      retryable: false,
    };
    api.runAutoUpdates.mockResolvedValue([{ repository_id: 42, run }]);
    api.getAutoUpdateState
      .mockResolvedValueOnce({ enabled: false, next_check_at: null, last_run: null })
      .mockResolvedValueOnce({ enabled: true, next_check_at: "2026-08-06T03:00:00Z", last_run: run });
    api.getUpdateState.mockResolvedValue(updateSnapshot());

    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    const toggle = await screen.findByRole("switch", { name: "Protected automatic upgrades" });
    fireEvent.click(toggle);

    await waitFor(() => expect(api.setAutoUpdateEnabled).toHaveBeenCalledWith(42, true));
    await waitFor(() => expect(api.runAutoUpdates).toHaveBeenCalledWith(expect.any(String)));
    expect(await screen.findByText(/partially applied/)).toBeVisible();
    expect(screen.getByText("Applied: writer")).toBeVisible();
    expect(screen.getByText(/newcomer: new Skill requires review/)).toBeVisible();
  });

  it("shows a persisted automatic pause and allows the channel preference to be disabled", async () => {
    api.listChannels.mockResolvedValue([{ ...channel(), role: "subscriber" }]);
    api.listSubscriptions.mockResolvedValue([
      {
        schema_version: 1,
        descriptor_version: 3,
        repository_id: 42,
        organization_id: 7,
        target: subscriptionReview().target,
        selected_skill_ids: ["writer"],
        auto_update: { enabled: true, next_check_at: "2026-08-06T03:00:00Z", last_run: null },
        read_only: false,
      },
    ]);
    api.getAutoUpdateState.mockResolvedValue({
      enabled: true,
      next_check_at: "2026-08-06T03:00:00Z",
      last_run: {
        started_at: "2026-08-06T02:00:00Z",
        completed_at: "2026-08-06T02:00:01Z",
        status: "paused",
        target: updateSnapshot().target,
        applied_skill_ids: [],
        pauses: [{ skill_id: "writer", reason: "unresolved_failure", detail: "retry manually" }],
        error: null,
        retryable: false,
      },
    });
    api.setAutoUpdateEnabled.mockResolvedValue({
      enabled: false,
      next_check_at: null,
      last_run: null,
    });

    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText(/writer: previous failure needs manual retry/)).toBeVisible();
    fireEvent.click(screen.getByRole("switch", { name: "Protected automatic upgrades" }));
    await waitFor(() => expect(api.setAutoUpdateEnabled).toHaveBeenCalledWith(42, false));
    expect(api.runAutoUpdates).not.toHaveBeenCalled();
  });
});
