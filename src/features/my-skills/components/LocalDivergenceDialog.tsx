import { AlertTriangle, Archive, Loader2, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { ModalCloseButton, ModalShell } from "../../../components/ui/ModalShell";
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
  const normalizedNames = Object.fromEntries(blocked.map((item) => [item.name, (localNames[item.name] ?? "").trim()]));
  // While the batch is applying, a copy that just landed would flag its own
  // (correct) name as taken. Nothing can be submitted then anyway, so the
  // inline errors stay out of the way until the user can act on them again.
  const nameIssues = busy ? {} : validateLocalCopyNames(copyable, localNames, takenNames ?? []);
  const canPreserve = blocked.length > 0 && copyable.length === blocked.length && Object.keys(nameIssues).length === 0;
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

      <div className="space-y-4 px-6 py-5">
        <div className="max-h-[min(52vh,30rem)] space-y-3 overflow-y-auto pr-1">
          {blocked.map((item, index) => {
            const inputId = `local-divergence-copy-name-${index}`;
            const issue = nameIssues[item.name];
            const contentGone = item.reason === "source_missing";
            return (
              <div
                key={item.name}
                className={isBatch ? "space-y-2 rounded-xl border border-border/70 bg-muted/20 p-3" : "space-y-2"}
              >
                <div className="flex flex-wrap items-center gap-2">
                  {isBatch && <span className="truncate text-sm font-semibold text-foreground">{item.name}</span>}
                  <span className="rounded-full border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-micro text-amber-700 dark:text-amber-400">
                    {t(REASON_LABELS[item.reason])}
                  </span>
                </div>
                {contentGone ? (
                  <p className="text-micro leading-4 text-muted-foreground">{t("mySkills.sourceMissingHint")}</p>
                ) : (
                  <>
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
                    {issue && <p className="text-micro leading-4 text-destructive">{t(NAME_ISSUE_LABELS[issue])}</p>}
                    {!isBatch && (
                      <p className="text-micro leading-4 text-muted-foreground">
                        {t(sourceGone ? "mySkills.sourceGoneCopyHint" : "mySkills.localCopyHint")}
                      </p>
                    )}
                  </>
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

        {isBatch && (
          <p className="text-micro leading-4 text-muted-foreground">
            {t(sourceGone ? "mySkills.sourceGoneCopyHint" : "mySkills.localCopyHint")}
          </p>
        )}

        <div className="rounded-xl border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {t(
            sourceGone
              ? isBatch
                ? "mySkills.removeAllDroppedWarning"
                : "mySkills.removeDroppedWarning"
              : isBatch
                ? "mySkills.discardAllDivergenceWarning"
                : "mySkills.discardDivergenceWarning",
          )}
        </div>

        {error && (
          <div className="rounded-xl border border-destructive/20 px-3 py-2 text-xs text-destructive">{error}</div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
        {busy && progress && progress.total > 1 && (
          <span className="mr-auto text-micro tabular-nums text-muted-foreground">
            {t("mySkills.resolvingProgress", { done: progress.done, total: progress.total })}
          </span>
        )}
        <Button variant="destructive" size="sm" onClick={sourceGone ? onUninstall : onDiscard} disabled={busy}>
          <Trash2 className="h-3.5 w-3.5" />
          {t(
            sourceGone
              ? isBatch
                ? "mySkills.removeAllAndUpdate"
                : "mySkills.removeAndUpdate"
              : isBatch
                ? "mySkills.discardAllAndUpdate"
                : "mySkills.discardAndUpdate",
          )}
        </Button>
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
      </div>
    </ModalShell>
  );
}
