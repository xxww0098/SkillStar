import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Skill } from "../../types";
import { DetailPanel } from "./DetailPanel";

vi.mock("../../hooks/useAiStream", () => ({
  useAiStream: () => ({
    content: null,
    visible: false,
    loading: false,
    hasDelta: false,
    wasNonStreaming: false,
    error: null,
    source: null,
    aiConfigured: false,
    locale: "zh-CN",
    execute: vi.fn(),
    cancel: vi.fn(),
    dismiss: vi.fn(),
    hydrate: vi.fn(),
    setVisible: vi.fn(),
    setContent: vi.fn(),
    setError: vi.fn(),
  }),
}));

vi.mock("../../features/my-skills/hooks/useDeployStatus", () => ({
  useDeployStatus: () => null,
  degradedDeploys: () => [],
}));

vi.mock("../../lib/ipc", () => ({
  tauriInvoke: vi.fn().mockResolvedValue({ data: null, snapshot_status: "fresh" }),
}));

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
  it("renders the selected skill's name and actions instead of an empty surface", () => {
    const onClose = vi.fn();

    render(
      <DetailPanel skill={SKILL} onClose={onClose} onInstall={vi.fn()} onUpdate={vi.fn()} onUninstall={vi.fn()} />,
    );

    expect(screen.getByRole("heading", { name: "triage" })).toBeInTheDocument();
    expect(screen.getByText("暂无描述。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "卸载" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
