// 更新批次的结果映射 — my_skills/updates.rs 对位（report_notification /
// report_lines / report_needs_attention / skill_update_in_flight /
// maybe_check_updates 的 30 分钟节流）。文案键与 updates.rs 逐条对齐，
// 经 t/tf 在调用点解析（此处接 translator 回调，纯函数可测）。

import type { SkillRecord } from "./filters";

/** update_skills 桥返回的报告（facade update_report_json 的 TS 形状）。 */
export interface UpdateReport {
  updated: { name: string; agentLinkFailures: string[]; projectsUpdated: string[] }[];
  failed: { name: string; error: string }[];
  skipped: string[];
  channelManaged: { name: string }[];
  identityChanged: { name: string; upstreamName: string }[];
  notUpdatable: string[];
  projectsUpdated: string[];
  projectFailures: string[];
}

export interface Translator {
  t(key: string): string;
  tf(key: string, vars: Record<string, string | number>): string;
}

/** updates.rs report_needs_attention：任何需要用户注意的尾部都让 toast
 * 变 warning。 */
export function reportNeedsAttention(report: UpdateReport): boolean {
  return (
    report.failed.length > 0 ||
    report.skipped.length > 0 ||
    report.identityChanged.length > 0 ||
    report.channelManaged.length > 0 ||
    report.notUpdatable.length > 0 ||
    report.projectFailures.length > 0 ||
    report.updated.some((result) => result.agentLinkFailures.length > 0)
  );
}

/** updates.rs report_lines：一次批次更新的全部行。 */
export function reportLines(report: UpdateReport, i18n: Translator): string[] {
  const updated = report.updated.length;
  const failed = report.failed.length;
  const lines: string[] = [];
  if (failed === 0) {
    if (updated > 0) {
      lines.push(i18n.tf("mySkills.batchUpdateSuccess", { count: updated }));
    }
  } else {
    lines.push(
      i18n.tf("mySkills.batchUpdatePartial", { success: updated, failed })
    );
    for (const failure of report.failed) {
      lines.push(`${failure.name}: ${failure.error}`);
    }
  }
  if (report.skipped.length > 0) {
    lines.push(
      i18n.tf("mySkills.updateSkippedToast", { names: report.skipped.join(", ") })
    );
  }
  for (const change of report.identityChanged) {
    lines.push(
      i18n.tf("mySkills.updateIdentityChanged", {
        name: change.name,
        upstream: change.upstreamName,
      })
    );
  }
  if (report.channelManaged.length > 0) {
    lines.push(
      i18n.tf("mySkills.batchUpdateChannelManaged", {
        count: report.channelManaged.length,
      })
    );
  }
  if (report.notUpdatable.length > 0) {
    lines.push(
      i18n.tf("mySkills.updateNotUpdatable", {
        names: report.notUpdatable.join(", "),
      })
    );
  }
  const linkFailures = report.updated.flatMap((result) => result.agentLinkFailures);
  if (linkFailures.length > 0) {
    lines.push(`${i18n.t("mySkills.agentRelinkFailed")} ${linkFailures.join(", ")}`);
  }
  if (report.projectsUpdated.length > 0) {
    lines.push(
      i18n.tf("projects.copiesUpdated", { names: report.projectsUpdated.join(", ") })
    );
  }
  if (report.projectFailures.length > 0) {
    lines.push(
      `${i18n.t("mySkills.updateProjectFailed")} ${report.projectFailures.join(", ")}`
    );
  }
  return lines;
}

export type NoticeTone = "success" | "warning" | "error";

/** updates.rs report_notification：一个批次的单一 toast（内容 + 级别）；
 * 空报告返回 null（一条也不弹）。 */
export function reportNotification(
  report: UpdateReport,
  i18n: Translator
): { tone: NoticeTone; message: string; pinned: boolean; actionLabelKey?: string } | null {
  const lines = reportLines(report, i18n);
  if (lines.length === 0) return null;
  const updated = report.updated.length;
  const failed = report.failed.length;
  const hasProjectFailures = report.projectFailures.length > 0;
  return {
    tone:
      failed > 0 && updated === 0
        ? "error"
        : !reportNeedsAttention(report) && updated > 0
          ? "success"
          : "warning",
    message: lines.join("\n"),
    pinned: hasProjectFailures,
    actionLabelKey: hasProjectFailures ? "mySkills.handleProjectCopies" : undefined,
  };
}

/** updates.rs skill_update_in_flight：单卡更新把技能名写进 busy；工具栏
 * 与选择条写批次 token，卡片仍要按点击时的名字快照出省略号。 */
export function skillUpdateInFlight(
  skill: SkillRecord,
  busy: string | null,
  updatingBatch: ReadonlySet<string>
): boolean {
  if (busy === null) return false;
  if (busy === "update_all" || busy === "batch_update") {
    return updatingBatch.has(skill.name);
  }
  return busy === skill.name;
}

/** updates.rs arm_batch_update 的准入判定：空名单或已有变更在跑时不再
 * 起第二批（一次只跑一个变更）。 */
export function canArmBatch(names: string[], busy: string | null): boolean {
  return names.length > 0 && busy === null;
}

/** updates.rs BACKGROUND_CHECK_INTERVAL：两次后台检查的最小间隔。 */
export const BACKGROUND_CHECK_INTERVAL_MS = 30 * 60 * 1000;

/** updates.rs maybe_check_updates：列表装载后，有上游可查、距上次检查
 * 够久且没有变更在跑时，静默起一次检查。 */
export function shouldBackgroundCheck(
  skills: SkillRecord[],
  lastCheckAt: number | null,
  nowMs: number,
  busy: string | null
): boolean {
  const hasUpstream = skills.some((skill) => skill.skillType !== "local");
  const due =
    lastCheckAt === null || nowMs - lastCheckAt >= BACKGROUND_CHECK_INTERVAL_MS;
  return hasUpstream && due && busy === null;
}
