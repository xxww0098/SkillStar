import type { BatchSkillToggleSkip } from "./ipc/commands/agents";

export const SKIP_UNMANAGED_REAL_DIRECTORY = "unmanaged_real_directory";

type Translate = (key: string, options?: Record<string, unknown>) => string;

/** Format a batch deployment skip without exposing backend-only English details. */
export function formatBatchToggleSkip(skip: BatchSkillToggleSkip, t: Translate): string {
  if (skip.code === SKIP_UNMANAGED_REAL_DIRECTORY) {
    return t("skillToggle.skipUnmanagedDirItem", {
      name: skip.skill_name,
      path: skip.path,
      defaultValue: "{{name}}: {{path}} is an unmanaged folder and was left in place",
    });
  }
  return skip.reason ? `${skip.skill_name}: ${skip.reason}` : skip.skill_name;
}

export function firstSkipPath(skips: BatchSkillToggleSkip[]): string | undefined {
  return skips.find((skip) => skip.path.trim().length > 0)?.path;
}
