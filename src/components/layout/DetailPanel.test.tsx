import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invalidateAiConfigCache } from "../../hooks/useAiConfig";
import type { LocalFirstResult, MarketplaceSkillDetails, Skill } from "../../types";
import { DetailPanel } from "./DetailPanel";

type DetailResult = LocalFirstResult<MarketplaceSkillDetails>;

function detail(summary: string | null): DetailResult {
  return {
    data: {
      summary,
      readme: summary ? `# ${summary} manual` : null,
      weekly_installs: null,
      github_stars: null,
      first_seen: null,
      security_audits: [],
    },
    snapshot_status: "fresh",
    snapshot_updated_at: null,
    error: null,
  };
}

function defaultResponse(command: string) {
  if (command === "get_ai_config") return { enabled: false };
  if (command === "get_skill_deploy_status") return [];
  if (command === "get_skill_detail_local") return detail(null);
  throw new Error(`Unexpected IPC: ${command}`);
}

beforeEach(() => {
  invalidateAiConfigCache();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => defaultResponse(command));
});

const SKILL: Skill = {
  name: "triage",
  description: "",
  skill_type: "hub",
  stars: 619200,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/mattpocock/skills",
  tree_hash: "hash123",
  category: "None",
  author: "mattpocock",
  topics: [],
  source: "mattpocock/skills",
  rank: 11,
};

describe("DetailPanel", () => {
  it("keeps the new selection's description and reader when an old read finishes late", async () => {
    let resolveOld!: (value: DetailResult) => void;
    const oldRead = new Promise<DetailResult>((resolve) => {
      resolveOld = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_skill_detail_local")
        return (args as { name: string }).name === "triage" ? oldRead : detail("Current");
      return defaultResponse(command);
    });
    const props = { onClose: vi.fn(), onInstall: vi.fn(), onUpdate: vi.fn(), onUninstall: vi.fn() };
    const { rerender } = render(<DetailPanel {...props} skill={SKILL} />);
    rerender(<DetailPanel {...props} skill={{ ...SKILL, name: "current" }} />);
    expect(await screen.findByText("Current")).toBeInTheDocument();
    await act(async () => {
      resolveOld(detail("Previous"));
    });
    expect(screen.getByText("Current")).toBeInTheDocument();
    expect(screen.queryByText("Previous")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "阅读 SKILL.md" }));
    expect(await screen.findByRole("heading", { name: "Current manual" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "current" })).toBeInTheDocument();
  });

  it("refreshes the current snapshot but stops a previous source's refresh continuation", async () => {
    let finishOldSync!: () => void;
    let finishCurrentSync!: () => void;
    const oldSync = new Promise<void>((resolve) => {
      finishOldSync = resolve;
    });
    const currentSync = new Promise<void>((resolve) => {
      finishCurrentSync = resolve;
    });
    let oldReads = 0;
    let currentReads = 0;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_skill_detail_local") {
        const isOld = (args as { source: string }).source === SKILL.source;
        const reads = isOld ? ++oldReads : ++currentReads;
        const result = detail(isOld ? "Previous snapshot" : reads === 1 ? "Current snapshot" : "Refreshed");
        return reads === 1 ? { ...result, snapshot_status: "stale" } : result;
      }
      if (command === "sync_marketplace_scope") {
        return (args as { scope: string }).scope.includes(SKILL.source ?? "") ? oldSync : currentSync;
      }
      return defaultResponse(command);
    });
    const props = { onClose: vi.fn(), onInstall: vi.fn(), onUpdate: vi.fn(), onUninstall: vi.fn() };
    const { rerender } = render(<DetailPanel {...props} skill={SKILL} />);
    expect(await screen.findByText("Previous snapshot")).toBeInTheDocument();
    rerender(<DetailPanel {...props} skill={{ ...SKILL, source: "another/repo" }} />);
    expect(await screen.findByText("Current snapshot")).toBeInTheDocument();
    await act(async () => {
      finishOldSync();
    });
    expect(oldReads).toBe(1);
    expect(screen.getByText("Current snapshot")).toBeInTheDocument();
    await act(async () => {
      finishCurrentSync();
    });
    expect(await screen.findByText("Refreshed")).toBeInTheDocument();
    expect(screen.queryByText("Previous snapshot")).not.toBeInTheDocument();
  });

  it("renders the selected skill's name and actions instead of an empty surface", async () => {
    const onClose = vi.fn();

    await act(async () => {
      render(
        <DetailPanel skill={SKILL} onClose={onClose} onInstall={vi.fn()} onUpdate={vi.fn()} onUninstall={vi.fn()} />,
      );
    });

    expect(screen.getByRole("heading", { name: "triage" })).toBeInTheDocument();
    expect(screen.getByText("暂无描述。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "卸载" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
