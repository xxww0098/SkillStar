import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GuideDto } from "../../../types/generated/Guide";
import type { GuideSummaryDto } from "../../../types/generated/GuideSummary";
import type { LearningProgressDto } from "../../../types/generated/LearningProgress";
import { LearnContent } from "./LearnContent";

vi.mock("react-i18next", () => {
  const messages: Record<string, string> = {
    "common.retry": "Try Again",
    "common.cancel": "Cancel",
    "common.installing": "Installing...",
    "learn.title": "Learn",
    "learn.subtitle": "Read without installing.",
    "learn.featured": "Featured",
    "learn.installed": "Installed",
    "learn.notInstalled": "Not installed",
    "learn.revisionBound": "Revision-bound",
    "learn.read": "Read",
    "learn.start": "Start",
    "learn.continue": "Continue",
    "learn.resumeTitle": "Continue here",
    "learn.resumeBody": "Step {{step}} · {{done}}/{{total}} complete",
    "learn.resumeEmpty": "No local progress on this Guide revision yet.",
    "learn.emptyTitle": "No Guides yet",
    "learn.emptyBody": "Guides appear here when they are bundled or converted locally.",
    "learn.loadFailed": "Couldn't load Learn",
    "learn.back": "Back",
    "learn.practice": "Practice",
    "learn.skillDrift": "Installed Skill differs from this Guide revision",
    "learn.completeStep": "Mark step complete",
    "learn.practiceHint": "Reading did not install anything. Confirm the exact revision before practice.",
    "learn.previewInstall": "Preview install",
    "learn.installPreviewTitle": "Exact revision install",
    "learn.noAuthorCommands": "This install does not run author commands.",
    "learn.confirmInstall": "Install this revision",
    "learn.staleTitle": "This progress belongs to an older Guide revision",
    "learn.staleBody": "It will not be applied silently to the current Guide.",
    "learn.continueOld": "Keep old progress",
    "learn.restart": "Start this revision",
    "learn.kind.reading": "Reading",
    "learn.kind.practice": "Practice",
    "learn.kind.verify": "Verify",
  };
  return {
    useTranslation: () => ({
      i18n: { language: "en", resolvedLanguage: "en" },
      t: (key: string, values?: Record<string, unknown>) =>
        (messages[key] ?? key).replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name] ?? "")),
    }),
  };
});

const identity = {
  key: "ski:v1:seedfrontenddesign0000000000000000000000000000000000000000",
  source: {
    type: "git" as const,
    repository: "https://github.com/anthropics/skills",
    trackingRef: { kind: "defaultBranch" as const },
    contentRoot: "skills/frontend-design",
  },
};

const revision = {
  key: "skr:v1:seedfrontenddesignrev00000000000000000000000000000000000000",
  skillKey: identity.key,
  content: {
    hashVersion: 2,
    contentHash: "sha256:08c665c6594b90d2bc781094e7afd6a9cd9296fde61e6ff8e7b53e61b9b1fe1f",
  },
  source: {
    type: "git" as const,
    commitSha: "53048666b05b4799081517d00e09e0a2dd688678",
    treeHash: "0d5b74a14bdf3ebcd64f352d06376a2ef05ed296",
  },
};

const seedGuide: GuideDto = {
  id: "guide:frontend-design-first-success",
  title: "第一次用 frontend-design 做出可用界面",
  locale: "zh-CN",
  summary: "阅读不需要安装。",
  schemaVersion: "1",
  skillIdentity: identity,
  skillRevision: revision,
  revisionKey: "gkr:v1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  installed: false,
  skillDrift: false,
  steps: [
    {
      id: "s1-when",
      kind: "reading",
      title: "适用场景与边界",
      requiresSkill: false,
      blocks: [{ type: "paragraph", text: "阅读这份 Guide 不要求本地已安装 Skill。" }],
    },
    {
      id: "s2-how",
      kind: "reading",
      title: "怎么做",
      requiresSkill: false,
      blocks: [{ type: "paragraph", text: "先钉 brief。" }],
    },
    {
      id: "s3-practice",
      kind: "practice",
      title: "在真实项目里改一处界面",
      requiresSkill: true,
      blocks: [{ type: "paragraph", text: "确认后才安装精确 revision。" }],
    },
    {
      id: "s4-verify",
      kind: "verify",
      title: "对照验收清单",
      requiresSkill: false,
      blocks: [{ type: "list", ordered: false, items: ["页面能运行"] }],
    },
  ],
};

const summary: GuideSummaryDto = {
  id: seedGuide.id,
  title: seedGuide.title,
  locale: seedGuide.locale,
  summary: seedGuide.summary,
  displayName: "frontend-design",
  skillIdentity: identity,
  skillRevision: revision,
  revisionKey: seedGuide.revisionKey,
  stepCount: seedGuide.steps.length,
  firstStepId: seedGuide.steps[0].id,
  installed: false,
  skillDrift: false,
};

function progress(overrides: Partial<LearningProgressDto> = {}): LearningProgressDto {
  return {
    guideId: seedGuide.id,
    guideRevisionKey: seedGuide.revisionKey,
    currentStepId: "s2-how",
    completedStepIds: ["s1-when"],
    updatedAt: "2026-09-01T00:00:00Z",
    ...overrides,
  };
}

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: { retry: false },
    },
  });
}

function renderLearn() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <LearnContent />
    </QueryClientProvider>,
  );
}

describe("LearnContent", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("opens the uninstalled seed Guide without installing", async () => {
    const calls: string[] = [];
    vi.mocked(invoke).mockImplementation(async (command) => {
      calls.push(String(command));
      if (command === "list_guides") return [summary];
      if (command === "load_learning_progress") return { current: null, stale: null };
      if (command === "get_guide") return seedGuide;
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();

    expect(await screen.findByText("Not installed")).toBeInTheDocument();
    expect(screen.getByText(seedGuide.title)).toBeInTheDocument();
    expect(screen.getByText(/ski:v1:seedfrontenddesign/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(await screen.findByRole("heading", { name: "适用场景与边界" })).toBeInTheDocument();
    expect(screen.getByText("阅读这份 Guide 不要求本地已安装 Skill。")).toBeInTheDocument();
    expect(calls).not.toContain("install_skill");
  });

  it("restores local progress on the matching Guide revision", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_guides") return [summary];
      if (command === "load_learning_progress") return { current: progress(), stale: null };
      if (command === "get_guide") return seedGuide;
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();

    expect(await screen.findByText("Step s2-how · 1/4 complete")).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "Continue" })[0]);
    expect(await screen.findByRole("heading", { name: "怎么做" })).toBeInTheDocument();
  });

  it("keeps stale revision progress from being applied silently", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_guides") return [summary];
      if (command === "load_learning_progress") {
        return {
          current: null,
          stale: progress({
            guideRevisionKey: "gkr:v1:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            currentStepId: "s1-when",
          }),
        };
      }
      if (command === "get_guide") return seedGuide;
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();

    expect(await screen.findByText("This progress belongs to an older Guide revision")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep old progress" }));
    expect(await screen.findByRole("heading", { name: "适用场景与边界" })).toBeInTheDocument();
  });

  it("previews practice install only after an explicit action", async () => {
    const calls: string[] = [];
    vi.mocked(invoke).mockImplementation(async (command) => {
      calls.push(String(command));
      if (command === "list_guides") return [summary];
      if (command === "load_learning_progress") return { current: null, stale: null };
      if (command === "get_guide") return seedGuide;
      if (command === "save_learning_progress") {
        return progress({ currentStepId: "s3-practice", completedStepIds: ["s1-when", "s2-how"] });
      }
      if (command === "preview_practice_install") {
        return {
          required: true,
          skillIdentity: identity,
          skillRevision: revision,
          displayName: "frontend-design",
          installUrl: "https://github.com/anthropics/skills",
          contentRoot: "skills/frontend-design",
          runsAuthorCommands: false,
          installed: false,
          skillDrift: false,
        };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    fireEvent.click(await screen.findByRole("button", { name: /在真实项目里改一处界面/ }));

    expect(
      await screen.findByText("Reading did not install anything. Confirm the exact revision before practice."),
    ).toBeInTheDocument();
    expect(calls).not.toContain("install_skill");
    expect(calls).not.toContain("preview_practice_install");

    fireEvent.click(screen.getByRole("button", { name: "Preview install" }));
    expect(await screen.findByText("Exact revision install")).toBeInTheDocument();
    expect(screen.getByText("This install does not run author commands.")).toBeInTheDocument();
    expect(calls).not.toContain("install_skill");
  });

  it("shows a recoverable empty state when no Guide exists", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_guides") return [];
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();
    expect(await screen.findByText("No Guides yet")).toBeInTheDocument();
  });

  it("keeps progress writable failures visible without leaving the Guide", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_guides") return [summary];
      if (command === "load_learning_progress") return { current: null, stale: null };
      if (command === "get_guide") return seedGuide;
      if (command === "save_learning_progress") throw new Error("disk full");
      throw new Error(`Unexpected command: ${command}`);
    });

    renderLearn();
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    fireEvent.click(await screen.findByRole("button", { name: "Mark step complete" }));
    expect(await screen.findByText(/disk full/)).toBeInTheDocument();
    expect(screen.getByText("阅读这份 Guide 不要求本地已安装 Skill。")).toBeInTheDocument();
  });
});
