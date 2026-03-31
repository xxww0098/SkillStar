import { useTranslation } from "react-i18next";
import { CheckCircle2 } from "lucide-react";
import { Button } from "../../ui/button";

export interface CompletedPhaseProps {
  count: number;
  onDone: () => void;
}

export function CompletedPhase({
  count,
  onDone,
}: CompletedPhaseProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-14 gap-4 px-6">
      <div className="w-14 h-14 rounded-2xl bg-emerald-500/10 flex items-center justify-center">
        <CheckCircle2 className="w-7 h-7 text-emerald-500" />
      </div>
      <div className="text-center space-y-1">
        <h3 className="text-heading-sm">{t("githubImportModal.titleComplete")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("githubImportModal.descComplete", { count })}
        </p>
      </div>
      <div className="flex gap-2 mt-2">
        <Button size="sm" onClick={onDone}>
          {t("githubImportModal.done")}
        </Button>
      </div>
    </div>
  );
}
