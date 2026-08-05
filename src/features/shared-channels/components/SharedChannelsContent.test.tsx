import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SharedChannelDescriptor } from "../../../types";
import { SharedChannelsContent } from "./SharedChannelsContent";

const api = vi.hoisted(() => ({
  listOrganizations: vi.fn(),
  listChannels: vi.fn(),
  create: vi.fn(),
  resume: vi.fn(),
}));

vi.mock("../api/channels", () => ({
  listSharedChannelOrganizations: api.listOrganizations,
  listSharedChannels: api.listChannels,
  createSharedChannel: api.create,
  resumeSharedChannel: api.resume,
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

describe("SharedChannelsContent", () => {
  beforeEach(() => {
    api.listOrganizations
      .mockReset()
      .mockResolvedValue([{ id: 7, login: "acme", avatar_url: null, viewer_is_admin: true }]);
    api.listChannels.mockReset().mockResolvedValue([]);
    api.create.mockReset().mockResolvedValue(channel());
    api.resume.mockReset().mockResolvedValue(channel());
  });

  it("moves from the create wizard into an empty channel detail with role and scope", async () => {
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    expect(await screen.findByText("Authorization boundary")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Create private repository" }));

    await waitFor(() => expect(api.create).toHaveBeenCalled());
    expect(await screen.findByText("owner")).toBeInTheDocument();
    expect(screen.getByText("GitHub App scope: selected repository only.")).toBeInTheDocument();
    expect(screen.getByText("Channel ready — no Skills have been published yet.")).toBeInTheDocument();
  });

  it("retries a persisted pending repository by numeric repository id", async () => {
    api.listChannels.mockResolvedValue([channel("awaiting_app_installation")]);
    render(<SharedChannelsContent scopeSwitch={<span>scope-switch</span>} />);

    fireEvent.click(await screen.findByRole("button", { name: "Retry authorization" }));

    await waitFor(() => expect(api.resume).toHaveBeenCalledWith(42));
    expect(await screen.findByText("Channel ready — no Skills have been published yet.")).toBeInTheDocument();
  });
});
