import { CheckCircle2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";

export type ShareCodeSkipReason = "repo_missing" | "no_source" | "install_failed";

export interface ShareCodeSkippedSkill {
  name: string;
  reason: ShareCodeSkipReason;
}

export interface ShareCodeInstallSummary {
  requestedCount: number;
  existingNames: string[];
  installedNames: string[];
  skipped: ShareCodeSkippedSkill[];
}

export interface CompletedPhaseProps {
  count: number;
  summary?: ShareCodeInstallSummary | null;
  onDone: () => void;
}

export function CompletedPhase({ count, summary, onDone }: CompletedPhaseProps) {
  const { t } = useTranslation();
  const existingCount = summary?.existingNames.length ?? 0;
  const installedCount = summary?.installedNames.length ?? count;
  const skippedCount = summary?.skipped.length ?? 0;

  return (
    <div className="flex flex-col items-center justify-center py-10 gap-4 px-6">
      <div className="w-14 h-14 rounded-2xl bg-emerald-500/10 flex items-center justify-center">
        <CheckCircle2 className="w-7 h-7 text-emerald-500" />
      </div>
      <div className="text-center space-y-1 w-full">
        <h3 className="text-heading-sm">{t("githubImportModal.titleComplete")}</h3>
        <p className="text-sm text-muted-foreground">
          {summary
            ? t("shareCodeImport.resultSummary", {
                total: summary.requestedCount,
                existing: existingCount,
                installed: installedCount,
                skipped: skippedCount,
              })
            : t("githubImportModal.descComplete", { count })}
        </p>
      </div>

      {summary && (
        <div className="w-full max-h-[34vh] overflow-y-auto space-y-3 text-left">
          {summary.existingNames.length > 0 && (
            <div className="space-y-1.5">
              <p className="text-xs text-muted-foreground font-medium">
                {t("shareCodeImport.alreadyHadTitle", { count: existingCount })}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {summary.existingNames.map((name) => (
                  <span
                    key={`existing-${name}`}
                    className="text-micro px-1.5 py-0.5 rounded-md bg-muted text-foreground/90"
                  >
                    {name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {summary.installedNames.length > 0 && (
            <div className="space-y-1.5">
              <p className="text-xs text-muted-foreground font-medium">
                {t("shareCodeImport.installedNowTitle", { count: installedCount })}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {summary.installedNames.map((name) => (
                  <span
                    key={`installed-${name}`}
                    className="text-micro px-1.5 py-0.5 rounded-md bg-emerald-500/10 text-emerald-600"
                  >
                    {name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {summary.skipped.length > 0 && (
            <div className="space-y-1.5">
              <p className="text-xs text-muted-foreground font-medium">
                {t("shareCodeImport.skippedTitle", { count: skippedCount })}
              </p>
              <div className="space-y-1">
                {summary.skipped.map((entry) => (
                  <div
                    key={`skipped-${entry.name}-${entry.reason}`}
                    className="text-xs rounded-md border border-amber-500/20 bg-amber-500/5 px-2 py-1.5 flex items-center justify-between gap-2"
                  >
                    <span className="truncate">{entry.name}</span>
                    <span className="text-micro text-amber-600 shrink-0">
                      {t(`shareCodeImport.skipReason.${entry.reason}`)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <div className="flex gap-2 mt-2">
        <Button size="sm" onClick={onDone}>
          {t("githubImportModal.done")}
        </Button>
      </div>
    </div>
  );
}
