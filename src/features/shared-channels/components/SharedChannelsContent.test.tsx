import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SharedChannelDescriptor } from "../../../types";
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
    authorization: { repository_selection: "selected", administration: "write", contents: "write" },
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
    skills: [{ id: "writer", folder_path: "skills/writer", description: "Write clearly" }],
    non_skill_files: ["README.md", ".github/workflows/ci.yml"],
    total_files: 5,
    exposure: { full_repository_contents_readable: true, full_history_readable: true },
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
        release: { id: 501, html_url: "https://github.com/acme/skillstar-shared/releases/tag/channel-v000001" },
      }),
    );
    api.cancelPublish.mockReset().mockResolvedValue(true);
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

    fireEvent.click(screen.getByRole("button", { name: "Confirm complete exposure and register" }));

    await waitFor(() => expect(api.confirmExisting).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("heading", { name: "acme/existing-skills" })).toBeInTheDocument();
  });

  it("keeps the same preview when confirmation fails so the user can retry", async () => {
    api.confirmExisting.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(existingChannel());
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Existing repository" }));
    fireEvent.click(await screen.findByRole("button", { name: "Scan exposure" }));
    const confirm = await screen.findByRole("button", { name: "Confirm complete exposure and register" });

    fireEvent.click(confirm);
    expect(await screen.findByText("offline")).toBeInTheDocument();
    expect(screen.getByText("Complete repository exposure")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm complete exposure and register" }));

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

    fireEvent.change(await screen.findByLabelText("Release title"), { target: { value: "Writing tools" } });
    fireEvent.change(screen.getByLabelText("Release notes"), { target: { value: "First stable version" } });
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

    fireEvent.change(await screen.findByLabelText("Release title"), { target: { value: "Writing tools" } });
    fireEvent.click(screen.getByRole("button", { name: "Preview publication" }));
    const publishButton = await screen.findByRole("button", { name: "Publish channel-v000001" });
    fireEvent.click(publishButton);

    expect(await screen.findByText("workflow authorization is not granted")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Publish channel-v000001" })).toBeInTheDocument();
    expect(api.previewPublish).toHaveBeenCalledTimes(1);
  });
});
