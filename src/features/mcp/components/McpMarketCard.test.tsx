import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpMarketEntry } from "../../../types";
import type { McpEntryStatus } from "../lib/installState";
import { McpMarketCard } from "./McpMarketCard";

function entry(patch: Partial<McpMarketEntry> = {}): McpMarketEntry {
  return {
    id: "row-1",
    name: "filesystem",
    namespace: "io.github.modelcontextprotocol/server-filesystem",
    description: "Local file access",
    repoUrl: "https://github.com/modelcontextprotocol/servers",
    stars: 1200,
    license: "MIT",
    version: "1.2.0",
    kind: "stdio",
    runtimes: ["npx", "docker"],
    updatedAt: null,
    recommended: true,
    source: null,
    title: null,
    websiteUrl: null,
    iconUrl: null,
    status: "active",
    isLatest: true,
    registrySource: "official",
    ...patch,
  };
}

function status(patch: Partial<McpEntryStatus> = {}): McpEntryStatus {
  return {
    state: "notInstalled",
    installed: null,
    installedVersion: null,
    latestVersion: "1.2.0",
    deprecated: false,
    superseded: false,
    matchedByName: false,
    ...patch,
  };
}

describe("McpMarketCard", () => {
  it("keeps install + identity, and leaves runtime/version/repo/details to the drawer", () => {
    const onInstall = vi.fn();
    const onOpenDetail = vi.fn();
    render(<McpMarketCard entry={entry()} status={status()} onInstall={onInstall} onOpenDetail={onOpenDetail} />);

    expect(screen.getByText("filesystem")).toBeInTheDocument();
    expect(screen.getByText("Local file access")).toBeInTheDocument();
    expect(screen.getByTitle("推荐")).toBeInTheDocument();
    expect(screen.getByText("1.2K")).toBeInTheDocument();

    expect(screen.queryByText("Repo")).not.toBeInTheDocument();
    expect(screen.queryByText("详情")).not.toBeInTheDocument();
    expect(screen.queryByText("npx")).not.toBeInTheDocument();
    expect(screen.queryByText("v1.2.0")).not.toBeInTheDocument();
    expect(screen.queryByText("STDIO")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /安装/i }));
    expect(onInstall).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByText("filesystem"));
    expect(onOpenDetail).toHaveBeenCalled();
  });

  it("surfaces deprecated as the exception, not as extra chrome", () => {
    render(
      <McpMarketCard
        entry={entry({ recommended: false, stars: 0 })}
        status={status({ deprecated: true })}
        onInstall={vi.fn()}
        onOpenDetail={vi.fn()}
      />,
    );
    expect(screen.getByText("已弃用")).toBeInTheDocument();
    expect(screen.queryByTitle("推荐")).not.toBeInTheDocument();
  });
});
