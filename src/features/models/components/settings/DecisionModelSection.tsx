import { BrainCircuit, Download, Loader2, Play, ShieldCheck, Trash2, X } from "lucide-react";
import { type ChangeEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { SettingsSectionHeader } from "../../../../components/ui/SettingsSectionHeader";
import { Textarea } from "../../../../components/ui/textarea";
import { cn } from "../../../../lib/utils";
import type { DecisionAnswer, DecisionModelStatus } from "../../../../types";
import {
  decisionStateLabel,
  engineLabel,
  useDecisionDownloadProgress,
  useDecisionEvaluate,
  useDecisionEngineInfo,
  useDecisionModelActions,
  useDecisionModelStatus,
} from "../../api/decision";

type QuestionKind = "boolean" | "choice" | "score";

const KINDS: QuestionKind[] = ["boolean", "choice", "score"];

/**
 * Settings panel for the local decision model.
 *
 * Three jobs in one place because they share one subject — the checkpoint:
 * show whether it is on disk, get it there, and let the user see what the
 * model answers. The panel never fakes progress or success: while a download
 * runs the numbers come from the backend's event stream, and a failed run
 * shows the backend's message.
 */
export function DecisionModelSection() {
  const { t } = useTranslation();
  const status = useDecisionModelStatus();
  const engine = useDecisionEngineInfo();
  const actions = useDecisionModelActions();
  const progress = useDecisionDownloadProgress();
  const evaluate = useDecisionEvaluate();

  const [stateText, setStateText] = useState("");
  const [questionText, setQuestionText] = useState("");
  const [kind, setKind] = useState<QuestionKind>("choice");
  const [optionsText, setOptionsText] = useState("");

  const ready = status.data?.state === "ready";
  const percent = progress
    ? Math.min(100, Math.max(0, Math.round((progress.downloaded / Math.max(1, progress.total)) * 100)))
    : 0;

  const payload = useMemo(() => {
    const options = optionsText
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const question: Record<string, unknown> = {
      id: "probe",
      type: kind,
      question: questionText,
    };
    if (kind === "choice") question.options = options;
    if (kind === "score") question.levels = options;
    return { state: stateText, questions: [question] };
  }, [kind, optionsText, questionText, stateText]);

  const canRun =
    ready && stateText.trim().length > 0 && questionText.trim().length > 0 && optionsText.trim().length > 0;

  const onRun = () => {
    evaluate.mutate(payload);
  };

  return (
    <div className="space-y-3">
      <SettingsSectionHeader
        icon={<BrainCircuit className="h-4 w-4" />}
        title={t("settings.decision.title")}
        meta={
          <span className="flex items-center gap-2 text-xs text-muted-foreground">
            <StatusPill status={status.data} />
            {engineLabel(engine.data) ? <span className="font-mono">{engineLabel(engine.data)}</span> : null}
          </span>
        }
      />

      <div className="rounded-xl border border-border bg-card p-4">
        <p className="mb-3 text-xs text-muted-foreground">{t("settings.decision.description")}</p>

        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-muted-foreground">{t("settings.decision.location")}</dt>
          <dd className="truncate font-mono">{status.data?.dir ?? "…"}</dd>
          <dt className="text-muted-foreground">{t("settings.decision.size")}</dt>
          <dd className="font-mono">
            {status.data ? `${formatBytes(status.data.presentBytes)} / ${formatBytes(status.data.totalBytes)}` : "…"}
          </dd>
          <dt className="text-muted-foreground">{t("settings.decision.endpoint")}</dt>
          <dd className="truncate font-mono">{status.data?.endpoint ?? "…"}</dd>
        </dl>

        {progress && actions.isDownloading ? (
          <div className="mt-3 space-y-1">
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div className="h-full bg-primary transition-[width]" style={{ width: `${percent}%` }} />
            </div>
            <p className="text-[11px] text-muted-foreground">
              {t("settings.decision.downloading", {
                percent,
                file: progress.file,
                done: formatBytes(progress.downloaded),
                total: formatBytes(progress.total),
              })}
            </p>
          </div>
        ) : null}

        <div className="mt-3 flex flex-wrap gap-2">
          {actions.isDownloading ? (
            <Button size="sm" variant="outline" onClick={() => void actions.cancelDownload()}>
              <X className="mr-1 h-3.5 w-3.5" />
              {t("settings.decision.cancel")}
            </Button>
          ) : (
            <Button size="sm" disabled={ready} onClick={() => void actions.download()}>
              <Download className="mr-1 h-3.5 w-3.5" />
              {status.data?.state === "partial"
                ? t("settings.decision.resumeDownload")
                : t("settings.decision.download")}
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            disabled={!ready || actions.isVerifying}
            onClick={() => void actions.verify()}
          >
            {actions.isVerifying ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="mr-1 h-3.5 w-3.5" />
            )}
            {t("settings.decision.verify")}
          </Button>
          {engine.data ? (
            <Button size="sm" variant="outline" onClick={() => void actions.unload()}>
              <Trash2 className="mr-1 h-3.5 w-3.5" />
              {t("settings.decision.unload")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="outline"
              disabled={!ready || actions.isLoading}
              onClick={() => void actions.load()}
            >
              {actions.isLoading ? (
                <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="mr-1 h-3.5 w-3.5" />
              )}
              {t("settings.decision.load")}
            </Button>
          )}
        </div>

        {!ready ? (
          <p className="mt-3 text-[11px] text-muted-foreground">{t("settings.decision.notReadyHint")}</p>
        ) : null}

        <div className="mt-4 space-y-2 border-t border-border pt-3">
          <p className="text-xs font-medium">{t("settings.decision.tryIt")}</p>
          <Textarea
            value={stateText}
            onChange={(event: ChangeEvent<HTMLTextAreaElement>) => setStateText(event.target.value)}
            placeholder={t("settings.decision.statePlaceholder")}
            className="min-h-[72px] font-mono text-xs"
          />
          <div className="flex gap-2">
            <div className="flex shrink-0 gap-1">
              {KINDS.map((candidate) => (
                <Button
                  key={candidate}
                  size="sm"
                  variant={kind === candidate ? "default" : "outline"}
                  onClick={() => setKind(candidate)}
                >
                  {t(`settings.decision.kind.${candidate}`)}
                </Button>
              ))}
            </div>
            <Input
              value={questionText}
              onChange={(event: ChangeEvent<HTMLInputElement>) => setQuestionText(event.target.value)}
              placeholder={t("settings.decision.questionPlaceholder")}
            />
          </div>
          <Textarea
            value={optionsText}
            onChange={(event: ChangeEvent<HTMLTextAreaElement>) => setOptionsText(event.target.value)}
            placeholder={t(
              kind === "score" ? "settings.decision.levelsPlaceholder" : "settings.decision.optionsPlaceholder",
            )}
            className="min-h-[56px] text-xs"
          />
          <div className="flex items-center gap-2">
            <Button size="sm" disabled={!canRun || evaluate.isPending} onClick={onRun}>
              {evaluate.isPending ? (
                <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="mr-1 h-3.5 w-3.5" />
              )}
              {t("settings.decision.run")}
            </Button>
            <span className="text-[11px] text-muted-foreground">{t("settings.decision.runHint")}</span>
          </div>

          {evaluate.isError ? (
            <p className="text-xs text-destructive">
              {evaluate.error instanceof Error ? evaluate.error.message : String(evaluate.error)}
            </p>
          ) : null}

          {evaluate.data ? (
            <div className="space-y-3 pt-2">
              {evaluate.data.results.flatMap((result) =>
                result.answers.map((answer) => <AnswerBlock key={`${result.id}:${answer.id}`} answer={answer} />),
              )}
              <p className="text-[11px] text-muted-foreground">
                {t("settings.decision.usage", {
                  questions: evaluate.data.usage.questions,
                  paths: evaluate.data.usage.candidatePaths,
                  backbone: evaluate.data.usage.backboneInputTokens,
                  wall: evaluate.data.usage.wallMs,
                })}
              </p>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: DecisionModelStatus | undefined }) {
  const label = decisionStateLabel(status);
  const tone =
    status?.state === "ready"
      ? "border-success/30 bg-success/10 text-success"
      : status?.state === "partial"
        ? "border-warning/30 bg-warning/10 text-warning"
        : "border-border bg-muted text-muted-foreground";
  return <span className={cn("rounded-full border px-2 py-0.5 font-medium", tone)}>{label}</span>;
}

function AnswerBlock({ answer }: { answer: DecisionAnswer }) {
  const { t } = useTranslation();
  return (
    <div className="rounded-lg border border-border bg-background/40 p-3">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs font-medium">
          {answer.id}
          <span className="ml-2 font-normal text-muted-foreground">{t(`settings.decision.kind.${answer.kind}`)}</span>
        </span>
        <span className="font-mono text-xs">
          {answer.selectedDescription} · {answer.topProbability.toFixed(3)}
        </span>
      </div>
      <div className="mt-2 space-y-1">
        {answer.distribution.map((entry) => (
          <div key={entry.key} className="flex items-center gap-2 text-[11px]">
            <span className="w-28 shrink-0 truncate text-muted-foreground" title={entry.key}>
              {entry.key}
            </span>
            <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
              <span className="block h-full bg-primary" style={{ width: `${Math.round(entry.probability * 100)}%` }} />
            </span>
            <span className="w-12 shrink-0 text-right font-mono">{entry.probability.toFixed(3)}</span>
          </div>
        ))}
      </div>
      {answer.kind === "score" && answer.score !== null ? (
        <p className="mt-2 text-[11px] text-muted-foreground">
          {t("settings.decision.expectedScore", { score: answer.score.toFixed(2) })}
        </p>
      ) : null}
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const mib = bytes / 1_048_576;
  if (mib < 1024) return `${mib.toFixed(1)} MiB`;
  return `${(mib / 1024).toFixed(2)} GiB`;
}
