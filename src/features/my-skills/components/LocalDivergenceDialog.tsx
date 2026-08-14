import { AlertTriangle, Archive, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { ModalCloseButton, ModalShell } from "../../../components/ui/ModalShell";
import { cn } from "../../../lib/utils";
import { isSourceGone, type LocalDivergenceReason, type SkillUpdateBlocked } from "../../../types";
import { type LocalCopyNameIssue, validateLocalCopyNames } from "../lib/localCopyNames";

interface LocalDivergenceDialogProps {
  blocked: SkillUpdateBlocked[];
  busy: boolean;
  /** Resolved-so-far counter, shown while the batch is being applied. */
  progress?: { done: number; total: number } | null;
  error: string | null;
  /** Skill names already in the library — a preserved copy may not collide. */
  takenNames?: string[];
  onClose: () => void;
  onPreserve: (localNames: Record<string, string>) => void;
  onDiscard: () => void;
  /** Remove Skills whose source dropped them. Never offered for a local edit. */
  onUninstall: () => void;
}

const REASON_LABELS: Record<LocalDivergenceReason, string> = {
  content_changed: "mySkills.divergenceReasonContentChanged",
  baseline_missing: "mySkills.divergenceReasonBaselineMissing",
  snapshot_failed: "mySkills.divergenceReasonSnapshotFailed",
  source_removed: "mySkills.divergenceReasonSourceRemoved",
  source_missing: "mySkills.divergenceReasonSourceMissing",
};

/** "Your files differ" is only true for a local edit; other reasons stop the
 *  update without the user having touched anything, and must not be blamed on
 *  them. A Skill its source dropped is a different problem again — nothing
 *  diverged, the Skill simply is not published any more. */
function blockedDescription(
  t: (key: string, options?: Record<string, unknown>) => string,
  blocked: SkillUpdateBlocked[],
  isBatch: boolean,
  onlyLocalEdits: boolean,
  sourceGone: boolean,
): string {
  if (sourceGone) {
    return isBatch
      ? t("mySkills.sourceGoneBatchDescription", { count: blocked.length })
      : t("mySkills.sourceGoneDescription", { name: blocked[0]?.name });
  }
  if (!onlyLocalEdits) {
    return isBatch
      ? t("mySkills.updateBlockedBatchDescription", { count: blocked.length })
      : t("mySkills.updateBlockedDescription", { name: blocked[0]?.name });
  }
  return isBatch
    ? t("mySkills.localDivergenceBatchDescription", { count: blocked.length })
    : t("mySkills.localDivergenceDescription", { name: blocked[0]?.name });
}

const NAME_ISSUE_LABELS: Record<LocalCopyNameIssue, string> = {
  required: "mySkills.localCopyNameRequired",
  invalid: "mySkills.localCopyNameInvalid",
  duplicate: "mySkills.localCopyNameDuplicate",
  taken: "mySkills.localCopyNameTaken",
};

export function LocalDivergenceDialog({
  blocked,
  busy,
  progress,
  error,
  takenNames,
  onClose,
  onPreserve,
  onDiscard,
  onUninstall,
}: LocalDivergenceDialogProps) {
  const { t } = useTranslation();
  const [localNames, setLocalNames] = useState<Record<string, string>>({});
  const [selectedAction, setSelectedAction] = useState<"preserve" | "destructive">("preserve");

  // Seed from the backend suggestions, keeping anything the user already typed
  // for a Skill that is still blocked: a failure on one Skill re-renders the
  // queue and must not silently discard the names chosen for the others.
  useEffect(() => {
    setLocalNames((current) =>
      Object.fromEntries(blocked.map((item) => [item.name, current[item.name] ?? item.suggested_local_name])),
    );
  }, [blocked]);

  // The queue shrinks as the batch is applied; keep the batch wording while it
  // runs so the labels do not flip to singular mid-flight.
  const isBatch = blocked.length > 1 || (busy && (progress?.total ?? 0) > 1);
  // The resolver queues these separately, so a batch is never half one kind.
  const sourceGone = blocked.length > 0 && blocked.every((item) => isSourceGone(item.reason));
  const onlyLocalEdits = !sourceGone && blocked.every((item) => item.reason === "content_changed");
  // Nothing is left on disk for these, so there is nothing to copy either.
  const copyable = blocked.filter((item) => item.reason !== "source_missing");
  const canPreserveOption = copyable.length === blocked.length && blocked.length > 0;

  useEffect(() => {
    if (!canPreserveOption && blocked.length > 0) {
      setSelectedAction("destructive");
    }
  }, [canPreserveOption, blocked.length]);

  const normalizedNames = Object.fromEntries(blocked.map((item) => [item.name, (localNames[item.name] ?? "").trim()]));
  // While the batch is applying, a copy that just landed would flag its own
  // (correct) name as taken. Nothing can be submitted then anyway, so the
  // inline errors stay out of the way until the user can act on them again.
  const nameIssues = busy ? {} : validateLocalCopyNames(copyable, localNames, takenNames ?? []);
  const canPreserve = canPreserveOption && Object.keys(nameIssues).length === 0;
  const title = t(
    sourceGone
      ? "mySkills.sourceGoneTitle"
      : onlyLocalEdits
        ? "mySkills.localDivergenceTitle"
        : "mySkills.updateBlockedTitle",
  );

  return (
    <ModalShell
      open={blocked.length > 0}
      onClose={onClose}
      ariaLabel={title}
      role="alertdialog"
      panelClassName="max-w-2xl px-4"
      dismissable={!busy}
    >
      <div className="flex items-start justify-between gap-4 px-6 pt-5">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-amber-500/10 text-amber-600 dark:text-amber-400">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="space-y-1">
            <h2 className="text-heading-sm">{title}</h2>
            <p className="text-caption leading-5">
              {blockedDescription(t, blocked, isBatch, onlyLocalEdits, sourceGone)}
            </p>
            {isBatch && (
              <p className="text-micro leading-4 text-muted-foreground">{t("mySkills.sharedCheckoutHint")}</p>
            )}
          </div>
        </div>
        <ModalCloseButton onClose={onClose} disabled={busy} />
      </div>

      <div className="space-y-4 px-6 py-4">
        {error && (
          <div className="rounded-xl border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}

        <div className="text-xs font-medium text-muted-foreground">{t("mySkills.divergenceResolutionPrompt")}</div>

        <div className="space-y-3">
          {/* 方案一：转为本地副本 / 保留修改 */}
          <div
            role="button"
            tabIndex={canPreserveOption ? 0 : -1}
            onClick={() => canPreserveOption && setSelectedAction("preserve")}
            onKeyDown={(e) => {
              if (canPreserveOption && (e.key === "Enter" || e.key === " ")) {
                setSelectedAction("preserve");
              }
            }}
            className={cn(
              "relative rounded-xl border p-4 transition-all text-left outline-none",
              selectedAction === "preserve"
                ? "border-primary/60 bg-primary/[0.03] shadow-xs ring-1 ring-primary/20"
                : "border-border/70 hover:border-border hover:bg-muted/20",
              !canPreserveOption ? "opacity-60 cursor-not-allowed" : "cursor-pointer",
            )}
          >
            <div className="flex items-start gap-3">
              <div
                className={cn(
                  "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors",
                  selectedAction === "preserve"
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-muted-foreground/40",
                )}
              >
                {selectedAction === "preserve" && <div className="h-1.5 w-1.5 rounded-full bg-background" />}
              </div>
              <div className="flex-1 space-y-1">
                <div className="flex items-center gap-2">
                  <Archive className="h-4 w-4 text-primary" />
                  <span className="text-sm font-semibold text-foreground">
                    {t(
                      sourceGone
                        ? "mySkills.divergenceOptionPreserveSourceGoneTitle"
                        : "mySkills.divergenceOptionPreserveDivergedTitle",
                    )}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t(
                    sourceGone
                      ? "mySkills.divergenceOptionPreserveSourceGoneDesc"
                      : "mySkills.divergenceOptionPreserveDivergedDesc",
                  )}
                </p>

                {!canPreserveOption && (
                  <p className="pt-1 text-micro text-destructive">{t("mySkills.sourceMissingHint")}</p>
                )}

                {/* 展开的本地副本配置输入框 */}
                {selectedAction === "preserve" && canPreserveOption && (
                  <div
                    className="mt-3 space-y-3 pt-2 border-t border-border/50"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <div className="max-h-[min(38vh,22rem)] space-y-3 overflow-y-auto pr-1">
                      {blocked.map((item, index) => {
                        const inputId = `local-divergence-copy-name-${index}`;
                        const issue = nameIssues[item.name];
                        return (
                          <div
                            key={item.name}
                            className={
                              isBatch ? "space-y-2 rounded-lg border border-border/60 bg-muted/20 p-3" : "space-y-2"
                            }
                          >
                            <div className="flex flex-wrap items-center gap-2">
                              {isBatch && (
                                <span className="truncate text-sm font-medium text-foreground">{item.name}</span>
                              )}
                              <Badge
                                variant="outline"
                                className="text-micro border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-400"
                              >
                                {t(REASON_LABELS[item.reason])}
                              </Badge>
                            </div>
                            <label htmlFor={inputId} className="block text-xs font-medium text-foreground">
                              {t("mySkills.localCopyName")}
                            </label>
                            <Input
                              id={inputId}
                              value={localNames[item.name] ?? ""}
                              onChange={(event) =>
                                setLocalNames((current) => ({
                                  ...current,
                                  [item.name]: event.target.value,
                                }))
                              }
                              disabled={busy}
                              aria-invalid={issue ? true : undefined}
                              autoComplete="off"
                            />
                            {issue && (
                              <p className="text-micro leading-4 text-destructive">{t(NAME_ISSUE_LABELS[issue])}</p>
                            )}
                            {!isBatch && (
                              <p className="text-micro leading-4 text-muted-foreground">
                                {t(sourceGone ? "mySkills.sourceGoneCopyHint" : "mySkills.localCopyHint")}
                              </p>
                            )}
                            {item.error && item.error !== error && (
                              <div className="rounded-lg border border-destructive/20 px-3 py-2 text-xs text-destructive">
                                {item.error}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* 方案二：彻底移除 / 丢弃本地修改 */}
          <div
            role="button"
            tabIndex={0}
            onClick={() => setSelectedAction("destructive")}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                setSelectedAction("destructive");
              }
            }}
            className={cn(
              "relative rounded-xl border p-4 transition-all text-left cursor-pointer outline-none",
              selectedAction === "destructive"
                ? "border-destructive/60 bg-destructive/[0.03] shadow-xs ring-1 ring-destructive/20"
                : "border-border/70 hover:border-border hover:bg-muted/20",
            )}
          >
            <div className="flex items-start gap-3">
              <div
                className={cn(
                  "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors",
                  selectedAction === "destructive"
                    ? "border-destructive bg-destructive text-destructive-foreground"
                    : "border-muted-foreground/40",
                )}
              >
                {selectedAction === "destructive" && <div className="h-1.5 w-1.5 rounded-full bg-background" />}
              </div>
              <div className="flex-1 space-y-1">
                <div className="flex items-center gap-2">
                  {sourceGone ? (
                    <Trash2 className="h-4 w-4 text-destructive" />
                  ) : (
                    <RotateCcw className="h-4 w-4 text-destructive" />
                  )}
                  <span className="text-sm font-semibold text-foreground">
                    {t(
                      sourceGone
                        ? isBatch
                          ? "mySkills.divergenceOptionUninstallBatchTitle"
                          : "mySkills.divergenceOptionUninstallTitle"
                        : isBatch
                          ? "mySkills.divergenceOptionDiscardBatchTitle"
                          : "mySkills.divergenceOptionDiscardTitle",
                      { count: blocked.length },
                    )}
                  </span>
                </div>
                <p className="text-xs text-muted-foreground leading-5">
                  {t(
                    sourceGone
                      ? isBatch
                        ? "mySkills.removeAllDroppedWarning"
                        : "mySkills.removeDroppedWarning"
                      : isBatch
                        ? "mySkills.discardAllDivergenceWarning"
                        : "mySkills.discardDivergenceWarning",
                  )}
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2.5 border-t border-border/60 px-6 py-3.5">
        {busy && progress && progress.total > 1 && (
          <span className="mr-auto text-micro tabular-nums text-muted-foreground">
            {t("mySkills.resolvingProgress", { done: progress.done, total: progress.total })}
          </span>
        )}
        <Button variant="outline" size="sm" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </Button>
        {selectedAction === "destructive" ? (
          <Button variant="destructive" size="sm" onClick={sourceGone ? onUninstall : onDiscard} disabled={busy}>
            {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
            {t(
              sourceGone
                ? isBatch
                  ? "mySkills.removeAllSkills"
                  : "mySkills.removeSkill"
                : isBatch
                  ? "mySkills.discardAllAndUpdate"
                  : "mySkills.discardAndUpdate",
            )}
          </Button>
        ) : (
          <Button size="sm" onClick={() => onPreserve(normalizedNames)} disabled={busy || !canPreserve}>
            {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Archive className="h-3.5 w-3.5" />}
            {t(
              sourceGone
                ? isBatch
                  ? "mySkills.keepAllAsLocalCopies"
                  : "mySkills.keepAsLocalCopy"
                : isBatch
                  ? "mySkills.preserveAllAndUpdate"
                  : "mySkills.preserveAndUpdate",
            )}
          </Button>
        )}
      </div>
    </ModalShell>
  );
}
