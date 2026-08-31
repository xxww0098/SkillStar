import { globalSkillsTargetKey } from "./agentSkillSync";

export interface GlobalSkillsTargetReadToken {
  targetKey: string;
  epoch: number;
  request: number;
}

/**
 * Fences async reads of a physical Global skills directory.
 *
 * A new read supersedes older reads for the same target. A mutation bumps
 * the epoch before it starts, so a read issued before that mutation cannot
 * overwrite the authoritative post-mutation refresh when it resolves late.
 */
export class GlobalSkillsTargetReadGuard {
  private readonly epochs = new Map<string, number>();
  private readonly requests = new Map<string, number>();

  begin(path: string): GlobalSkillsTargetReadToken {
    const targetKey = globalSkillsTargetKey(path);
    const request = (this.requests.get(targetKey) ?? 0) + 1;
    this.requests.set(targetKey, request);

    return {
      targetKey,
      epoch: this.epochs.get(targetKey) ?? 0,
      request,
    };
  }

  invalidate(path: string): void {
    const targetKey = globalSkillsTargetKey(path);
    this.epochs.set(targetKey, (this.epochs.get(targetKey) ?? 0) + 1);
  }

  accepts(token: GlobalSkillsTargetReadToken): boolean {
    return (
      (this.epochs.get(token.targetKey) ?? 0) === token.epoch && this.requests.get(token.targetKey) === token.request
    );
  }
}
