import type { GuideDraftDto } from "../../../types/generated/GuideDraft";
import type { GuideDto } from "../../../types/generated/Guide";
import type { GuideSummaryDto } from "../../../types/generated/GuideSummary";
import type { LearningProgressDto } from "../../../types/generated/LearningProgress";
import type { PracticeInstallPreviewDto } from "../../../types/generated/PracticeInstallPreview";
import type { ProgressSnapshotDto } from "../../../types/generated/ProgressSnapshot";

export interface LearningCommands {
  list_guides: { args: Record<string, never>; result: GuideSummaryDto[] };
  get_guide: { args: { id: string }; result: GuideDto | null };
  load_learning_progress: {
    args: { guideId: string; guideRevisionKey: string };
    result: ProgressSnapshotDto;
  };
  save_learning_progress: {
    args: {
      guideId: string;
      guideRevisionKey: string;
      currentStepId: string;
      completedStepIds: string[];
    };
    result: LearningProgressDto;
  };
  preview_practice_install: {
    args: { guideId: string; stepId: string };
    result: PracticeInstallPreviewDto;
  };
  preview_guide_draft: { args: { name: string; locale: string }; result: GuideDraftDto };
  create_guide_draft: { args: { name: string; locale: string }; result: GuideDraftDto };
  list_guide_drafts: { args: Record<string, never>; result: GuideDraftDto[] };
}
