import type { GuideDto } from "../../../types/generated/Guide";
import type { GuideSummaryDto } from "../../../types/generated/GuideSummary";
import type { LearningProgressDto } from "../../../types/generated/LearningProgress";
import type { PracticeInstallPreviewDto } from "../../../types/generated/PracticeInstallPreview";
import type { ProgressSnapshotDto } from "../../../types/generated/ProgressSnapshot";
import type { DevMockHandlers } from "./shared";

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
  summary: "用 frontend-design 走完一次界面实践。阅读不需要安装；只有动手改文件时才安装精确 revision。",
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
      blocks: [
        { type: "paragraph", text: "阅读这份 Guide 不要求本地已安装 Skill。" },
        { type: "list", ordered: false, items: ["适合新页面", "安装不是主动作"] },
      ],
    },
    {
      id: "s2-how",
      kind: "reading",
      title: "怎么做",
      requiresSkill: false,
      blocks: [{ type: "paragraph", text: "先钉 brief，再生成可运行界面。" }],
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
      blocks: [{ type: "list", ordered: false, items: ["页面能运行", "没有把安装当成完成"] }],
    },
  ],
};

const summaries: GuideSummaryDto[] = [
  {
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
  },
];

let progressStore: LearningProgressDto | null = null;
const drafts: unknown[] = [];

export const LEARNING_HANDLERS: DevMockHandlers = {
  list_guides: async () => summaries,
  get_guide: async (args) => (args.id === seedGuide.id ? seedGuide : null),
  load_learning_progress: async () =>
    ({
      current: progressStore,
      stale: null,
    }) satisfies ProgressSnapshotDto,
  save_learning_progress: async (args) => {
    progressStore = {
      guideId: String(args.guideId),
      guideRevisionKey: String(args.guideRevisionKey),
      currentStepId: String(args.currentStepId),
      completedStepIds: Array.isArray(args.completedStepIds) ? (args.completedStepIds as string[]) : [],
      updatedAt: new Date().toISOString(),
    };
    return progressStore;
  },
  preview_practice_install: async () =>
    ({
      required: true,
      skillIdentity: identity,
      skillRevision: revision,
      displayName: "frontend-design",
      installUrl: "https://github.com/anthropics/skills",
      contentRoot: "skills/frontend-design",
      runsAuthorCommands: false,
      installed: false,
      skillDrift: false,
    }) satisfies PracticeInstallPreviewDto,
  preview_guide_draft: async () => {
    throw new Error("Convert a bound private tutorial from My Skills");
  },
  create_guide_draft: async () => {
    throw new Error("Convert a bound private tutorial from My Skills");
  },
  list_guide_drafts: async () => drafts,
};
