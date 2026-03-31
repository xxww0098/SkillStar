import { useTranslation } from "react-i18next";
import { AlertTriangle, RotateCcw } from "lucide-react";
import { Button } from "../../ui/button";

export interface ErrorPhaseProps {
  message: string;
  onRetry: () => void;
}

export function ErrorPhase({
  message,
  onRetry,
}: ErrorPhaseProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-14 gap-4 px-6">
      <div className="w-14 h-14 rounded-2xl bg-amber-500/10 flex items-center justify-center">
        <AlertTriangle className="w-7 h-7 text-amber-500" />
      </div>
      <div className="text-center space-y-1">
        <h3 className="text-heading-sm">{t("githubImportModal.somethingWrong")}</h3>
        <p className="text-sm text-muted-foreground max-w-xs">{message}</p>
      </div>
      <Button variant="ghost" size="sm" onClick={onRetry} className="mt-1">
        <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
        {t("githubImportModal.tryAgain")}
      </Button>
    </div>
  );
}
