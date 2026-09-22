import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import type { ProjectSkillPlanDiff } from "../../../types";

interface PendingPlanDiffViewProps {
  plans: ProjectSkillPlanDiff[];
  approving: boolean;
  onApprove: (planId: string) => void;
}

export function PendingPlanDiffView({ plans, approving, onApprove }: PendingPlanDiffViewProps) {
  const { t } = useTranslation();
  if (plans.length === 0) return null;
  return (
    <div className="border-b border-border/80 bg-background/70" data-testid="pending-plan">
      {plans.map((plan) => (
        <section key={plan.plan_id} className="px-4 py-3 space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-sm font-semibold">{t("projects.pendingPlanTitle")}</h2>
            <Button size="sm" disabled={approving} onClick={() => onApprove(plan.plan_id)}>
              {t("projects.pendingPlanApprove")}
            </Button>
          </div>
          <p className="text-xs font-mono break-all">
            {t("projects.pendingPlanRoot")}: {plan.root}
          </p>
          <p className="text-xs">
            {plan.will_register ? t("projects.pendingPlanRegister") : t("projects.pendingPlanExisting")}
          </p>
          <p className="text-xs">
            {t("projects.pendingPlanOwner")}: {plan.owner_id}
          </p>
          <p className="text-xs break-words">
            {t("projects.pendingPlanAffected")}: {plan.affected_agents.join(", ")}
          </p>
          <ul className="text-xs space-y-1">
            {plan.changes.map((change) => (
              <li key={`${plan.plan_id}-${change.name}`} className="break-all">
                {change.action} {change.name} {change.skill_path}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

export function PendingPlanApproval({ projectPath }: { projectPath: string | null }) {
  const { t } = useTranslation();
  const [plans, setPlans] = useState<ProjectSkillPlanDiff[]>([]);
  const [approving, setApproving] = useState(false);

  const reload = useCallback(async () => {
    if (!projectPath) {
      setPlans([]);
      return;
    }
    const next = await tauriInvoke("list_pending_project_skill_plans", { projectPath });
    setPlans(next);
  }, [projectPath]);

  useEffect(() => {
    let cancelled = false;
    void reload().catch(() => {
      if (!cancelled) setPlans([]);
    });
    return () => {
      cancelled = true;
    };
  }, [reload]);

  const approve = async (planId: string) => {
    setApproving(true);
    try {
      await tauriInvoke("approve_project_skill_plan", { planId });
      toast.success(t("projects.pendingPlanApproved"));
      await reload();
    } catch {
      toast.error(t("projects.pendingPlanFailed"));
    } finally {
      setApproving(false);
    }
  };

  if (!projectPath) return null;
  return <PendingPlanDiffView plans={plans} approving={approving} onApprove={(planId) => void approve(planId)} />;
}
