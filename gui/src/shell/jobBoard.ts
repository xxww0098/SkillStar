import { signal, untracked } from "nsh";
import { APPEAR_MS } from "../chrome/delayedFlag";

// Corner jobs. A card is a real long task: a stage sentence, an optional
// fraction the caller already counted, and cancel only when the caller
// passed a cancel function. Nothing here invents a percent.

/** 完成态任务卡片展示勾选反馈后自动收起的停留时长（ms）。 */
export const JOB_DONE_TTL_MS = 2000;

export interface JobInput {
  id: string;
  title: string;
  detail?: string;
  /** 0..1 when the work itself counted done/total. Omit for an indeterminate job. */
  fraction?: number;
  cancellable?: boolean;
  onCancel?: () => void;
}

export interface Job extends JobInput {
  startedAt: number;
  done: boolean;
  cancellable: boolean;
}

export interface JobBoard {
  jobList(): Job[];
  putJob(input: JobInput): void;
  endJob(id: string, settledTitle?: string): void;
  dropJob(id: string): void;
}

export function createJobBoard(opts?: {
  now?: () => number;
  after?: (ms: number, fn: () => void) => () => void;
}): JobBoard {
  const now = opts?.now ?? (() => Date.now());
  const after =
    opts?.after ??
    ((ms, fn) => {
      const id = setTimeout(fn, ms);
      return () => clearTimeout(id);
    });
  const jobs = signal<Job[]>([]);
  const doneTimers = new Map<string, () => void>();

  const clearDoneTimer = (id: string): void => {
    const cancel = doneTimers.get(id);
    if (cancel !== undefined) {
      cancel();
      doneTimers.delete(id);
    }
  };

  const dropJob = (id: string): void => {
    clearDoneTimer(id);
    const list = untracked(jobs);
    const next = list.filter((job) => job.id !== id);
    if (next.length !== list.length) jobs.set(next);
  };

  return {
    jobList: () => jobs(),
    putJob(input: JobInput): void {
      clearDoneTimer(input.id);
      const list = untracked(jobs);
      const existing = list.find((job) => job.id === input.id);
      if (existing) {
        jobs.set(
          list.map((job) =>
            job.id === input.id
              ? {
                  ...job,
                  title: input.title,
                  detail: input.detail,
                  fraction: input.fraction,
                  cancellable: input.cancellable ?? false,
                  onCancel: input.onCancel,
                  startedAt: existing.done ? now() : existing.startedAt,
                  done: false,
                }
              : job,
          ),
        );
        return;
      }
      jobs.set([
        ...list,
        {
          ...input,
          cancellable: input.cancellable ?? false,
          startedAt: now(),
          done: false,
        },
      ]);
    },
    endJob(id: string, settledTitle?: string): void {
      const list = untracked(jobs);
      const job = list.find((item) => item.id === id);
      if (!job || job.done) return;
      clearDoneTimer(id);
      if (now() - job.startedAt < APPEAR_MS) {
        jobs.set(list.filter((item) => item.id !== id));
        return;
      }
      jobs.set(
        list.map((item) =>
          item.id === id
            ? {
                ...item,
                title: settledTitle ?? item.title,
                detail: settledTitle !== undefined ? undefined : item.detail,
                done: true,
                fraction: item.fraction === undefined ? undefined : 1,
              }
            : item,
        ),
      );
      const cancel = after(JOB_DONE_TTL_MS, () => {
        doneTimers.delete(id);
        dropJob(id);
      });
      doneTimers.set(id, cancel);
    },
    dropJob,
  };
}

const defaultBoard = createJobBoard();

export function jobList(): Job[] {
  return defaultBoard.jobList();
}

export function putJob(input: JobInput): void {
  defaultBoard.putJob(input);
}

/**
 * Mark the job finished. A card that has been up for under 300ms is removed
 * instead, so a fast task does not flash a card (the caller can still toast);
 * longer tasks briefly show the settled check state and auto-close after JOB_DONE_TTL_MS.
 */
export function endJob(id: string, settledTitle?: string): void {
  defaultBoard.endJob(id, settledTitle);
}

export function dropJob(id: string): void {
  defaultBoard.dropJob(id);
}
