/**
 * The one place a blocked Skill update turns into a user decision.
 *
 * A blocked update is never a single Skill: the backend stops a whole physical
 * checkout, so one "update" can come back with several Skills that must all be
 * resolved before any of them moves. This hook owns that queue — it asks once
 * for the whole batch, applies the answer Skill by Skill, keeps the dialog up
 * with progress while it works, and re-asks with whatever is still blocked if a
 * resolution fails halfway. Callers get one promise and a summary; they never
 * drive `resolve_skill_update` themselves.
 *
 * Two kinds of stop can arrive together and they do not share an answer: a
 * local edit can be kept or discarded, while a Skill its source dropped can
 * only be kept or removed. Asking about both in one dialog would offer buttons
 * that cannot work for half the list, so the queue is drained one kind at a
 * time and each pass gets a dialog whose actions all apply.
 *
 * Both entry points (a card's "update" and the toolbar's "update all") share
 * this instance through the Skills context, so there is exactly one dialog and
 * one behaviour in the app.
 */

import { createElement, useCallback, useMemo, useRef, useState } from "react";
import i18n from "../../../i18n";
import {
  isSourceGone,
  type LocalDivergenceResolution,
  type ResolveSkillUpdateResult,
  type Skill,
  type SkillUpdateBlocked,
  type SkillUpdateFailure,
  type UpdateResult,
} from "../../../types";
import { LocalDivergenceDialog } from "../components/LocalDivergenceDialog";
import { reconcileBlockedUpdates } from "../lib/localDivergenceQueue";

export type LocalDivergenceChoice =
  | { kind: "preserve"; localNames: Record<string, string> }
  | { kind: "discard" }
  | { kind: "uninstall" };

export interface LocalDivergenceOutcome {
  updated: UpdateResult[];
  localCopies: Skill[];
  /** Skills removed because their source no longer ships them. */
  uninstalled: string[];
  /** Still blocked when the user closed the dialog — nothing was changed. */
  unresolved: SkillUpdateBlocked[];
  failed: SkillUpdateFailure[];
}

interface DialogState {
  blocked: SkillUpdateBlocked[];
  busy: boolean;
  error: string | null;
  progress: { done: number; total: number } | null;
}

const IDLE: DialogState = { blocked: [], busy: false, error: null, progress: null };

const EMPTY_OUTCOME: LocalDivergenceOutcome = {
  updated: [],
  localCopies: [],
  uninstalled: [],
  unresolved: [],
  failed: [],
};

/** Which dialog a stop belongs to — the two kinds never share one. */
function queueKind(blocked: SkillUpdateBlocked): "source_gone" | "diverged" {
  return isSourceGone(blocked.reason) ? "source_gone" : "diverged";
}

function mergeQueues(...lists: SkillUpdateBlocked[][]): SkillUpdateBlocked[] {
  const seen = new Set<string>();
  const merged: SkillUpdateBlocked[] = [];
  for (const list of lists) {
    for (const item of list) {
      if (seen.has(item.name)) continue;
      seen.add(item.name);
      merged.push(item);
    }
  }
  return merged;
}

function resolutionFor(choice: LocalDivergenceChoice, blocked: SkillUpdateBlocked): LocalDivergenceResolution {
  if (choice.kind === "preserve") {
    return { kind: "preserve", local_name: choice.localNames[blocked.name] ?? blocked.suggested_local_name };
  }
  return choice.kind === "uninstall" ? { kind: "uninstall" } : { kind: "discard" };
}

interface ResolverOptions {
  /** Applies one resolution and writes the result into the Skills cache. */
  resolveSkillUpdate: (name: string, resolution: LocalDivergenceResolution) => Promise<ResolveSkillUpdateResult>;
  /** Names already in the library, so a preserved copy cannot collide. */
  takenNames: string[];
}

export function useLocalDivergenceResolver({ resolveSkillUpdate, takenNames }: ResolverOptions) {
  const [dialog, setDialog] = useState<DialogState>(IDLE);
  const answerRef = useRef<((choice: LocalDivergenceChoice | null) => void) | null>(null);

  const ask = useCallback((blocked: SkillUpdateBlocked[], error: string | null) => {
    setDialog({ blocked, busy: false, error, progress: null });
    return new Promise<LocalDivergenceChoice | null>((resolve) => {
      answerRef.current = resolve;
    });
  }, []);

  const answer = useCallback((choice: LocalDivergenceChoice | null) => {
    const pending = answerRef.current;
    answerRef.current = null;
    pending?.(choice);
  }, []);

  /**
   * Drive `blocked` to completion. Resolves when the queue is empty or the
   * user backed out; never rejects — the outcome says what actually happened.
   */
  const resolveBlocked = useCallback(
    async (blocked: SkillUpdateBlocked[]): Promise<LocalDivergenceOutcome> => {
      if (blocked.length === 0) {
        return { ...EMPTY_OUTCOME };
      }
      if (answerRef.current) {
        return {
          ...EMPTY_OUTCOME,
          unresolved: blocked,
          failed: [{ name: blocked[0].name, error: i18n.t("mySkills.divergenceDialogBusy") }],
        };
      }

      const outcome: LocalDivergenceOutcome = { ...EMPTY_OUTCOME };
      let queue = blocked;

      while (queue.length > 0) {
        // One kind per dialog, so every button it shows can actually be used.
        const kind = queueKind(queue[0]);
        let remaining = queue.filter((item) => queueKind(item) === kind);
        let deferred = queue.filter((item) => queueKind(item) !== kind);
        let error: string | null = null;

        while (remaining.length > 0) {
          const choice = await ask(remaining, error);
          if (!choice) {
            setDialog(IDLE);
            outcome.unresolved = mergeQueues(remaining, deferred);
            // Only a failure the user walked away from is a failure; one they
            // retried into success must not be reported after the fact.
            if (error) outcome.failed.push({ name: remaining[0].name, error });
            return outcome;
          }

          const total = remaining.length;
          setDialog({ blocked: remaining, busy: true, error: null, progress: { done: 0, total } });
          error = null;

          try {
            while (remaining.length > 0) {
              const next = remaining[0];
              const result = await resolveSkillUpdate(next.name, resolutionFor(choice, next));
              if (result.update) outcome.updated.push(result.update);
              if (result.local_copy) outcome.localCopies.push(result.local_copy);
              outcome.uninstalled.push(...result.uninstalled);

              const reconciled = reconcileBlockedUpdates(remaining, next.name, result);
              // A backend that answered with the same Skill still blocked would
              // otherwise re-send the identical resolution forever.
              if (reconciled.some((item) => item.name === next.name)) {
                throw new Error(i18n.t("mySkills.divergenceNotResolved", { name: next.name }));
              }
              // A resolution can surface a stop of the other kind; it waits for
              // its own dialog instead of joining this one.
              remaining = reconciled.filter((item) => queueKind(item) === kind);
              deferred = mergeQueues(
                deferred,
                reconciled.filter((item) => queueKind(item) !== kind),
              );
              setDialog((current) => ({
                ...current,
                blocked: remaining,
                progress: { done: total - remaining.length, total },
              }));
            }
          } catch (thrown) {
            error = thrown instanceof Error ? thrown.message : String(thrown);
          }
        }

        queue = deferred;
      }

      setDialog(IDLE);
      return outcome;
    },
    [ask, resolveSkillUpdate],
  );

  const dialogElement = useMemo(
    () =>
      createElement(LocalDivergenceDialog, {
        blocked: dialog.blocked,
        busy: dialog.busy,
        progress: dialog.progress,
        error: dialog.error,
        takenNames,
        onClose: () => answer(null),
        onPreserve: (localNames: Record<string, string>) => answer({ kind: "preserve", localNames }),
        onDiscard: () => answer({ kind: "discard" }),
        onUninstall: () => answer({ kind: "uninstall" }),
      }),
    [dialog, takenNames, answer],
  );

  return { resolveBlocked, dialogElement };
}
