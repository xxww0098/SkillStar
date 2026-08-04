/**
 * PROTOTYPE — throwaway UI for wayfinder ticket
 * 「原型：独立「更新 N 项」主按钮与单卡更新降级」
 *
 * Plan: three structurally different Update-All + demoted card-update layouts
 * on the existing `#skills` route via `?variant=UA1|UA2|UA3`.
 */

export type UpdateAllVariant = "UA1" | "UA2" | "UA3";

export type ProtoSkill = {
  name: string;
  description: string;
  source: string;
  update_available: boolean;
};

export type UpdateAllProtoState = {
  variant: UpdateAllVariant;
  pendingCount: number;
  snapshotNames: string[];
  busy: boolean;
  lastAction: string;
  filterUpdateOnly: boolean;
};
