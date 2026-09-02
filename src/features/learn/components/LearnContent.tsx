import { BookOpen, Check, ChevronLeft, Loader2 } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { StatusChip } from "../../../components/ui/StatusChip";
import { Button } from "../../../components/ui/button";
import { tauriInvoke, useTauriMutation, useTauriQuery, useTauriQueryWithArgs } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { GuideBlockDto } from "../../../types/generated/GuideBlock";
import type { GuideSummaryDto } from "../../../types/generated/GuideSummary";
import type { LearningProgressDto } from "../../../types/generated/LearningProgress";
import type { PracticeInstallPreviewDto } from "../../../types/generated/PracticeInstallPreview";

type View = { kind: "home" } | { kind: "guide"; id: string; stepId?: string };

export function LearnContent() {
  const { t } = useTranslation();
  const [view, setView] = useState<View>({ kind: "home" });
  const listQuery = useTauriQuery("list_guides", { retry: false });

  useEffect(() => {
    console.debug("learn_open");
  }, []);

  if (listQuery.isError) {
    return (
      <EmptyState
        title={t("learn.loadFailed")}
        body={String(listQuery.error)}
        action={
          <Button size="sm" onClick={() => void listQuery.refetch()}>
            {t("common.retry")}
          </Button>
        }
      />
    );
  }

  if (listQuery.isLoading || !listQuery.data) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (view.kind === "guide") {
    return (
      <GuideReader
        guideId={view.id}
        initialStepId={view.stepId}
        onBack={() => setView({ kind: "home" })}
        onHome={() => setView({ kind: "home" })}
      />
    );
  }

  return <LearnHome guides={listQuery.data} onOpen={(id, stepId) => setView({ kind: "guide", id, stepId })} />;
}

function LearnHome({ guides, onOpen }: { guides: GuideSummaryDto[]; onOpen: (id: string, stepId?: string) => void }) {
  const { t } = useTranslation();
  const featured = guides[0];
  const progressQuery = useTauriQueryWithArgs(
    "load_learning_progress",
    featured ? { guideId: featured.id, guideRevisionKey: featured.revisionKey } : { guideId: "", guideRevisionKey: "" },
    { enabled: Boolean(featured), retry: false },
  );
  const progress = progressQuery.data?.current;
  const stale = progressQuery.data?.stale;

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto px-6 py-5">
      <h1 className="text-lg font-semibold text-foreground">{t("learn.title")}</h1>
      <p className="mt-1 max-w-2xl text-sm text-muted-foreground">{t("learn.subtitle")}</p>
      {guides.length === 0 ? (
        <EmptyState title={t("learn.emptyTitle")} body={t("learn.emptyBody")} />
      ) : (
        <div className="mt-5 grid gap-4 lg:grid-cols-[1.2fr_0.8fr]">
          {featured && (
            <InsetPanel className="p-4">
              <div className="flex flex-wrap gap-1.5">
                <StatusChip tone="info">{t("learn.featured")}</StatusChip>
                <StatusChip tone={featured.installed ? "success" : "muted"}>
                  {featured.installed ? t("learn.installed") : t("learn.notInstalled")}
                </StatusChip>
                <StatusChip tone="muted">{t("learn.revisionBound")}</StatusChip>
              </div>
              <h2 className="mt-3 text-base font-semibold text-foreground">{featured.title}</h2>
              <p className="mt-1 text-sm text-muted-foreground">{featured.summary}</p>
              <p className="mt-3 font-mono text-[11px] leading-relaxed break-all text-sky-400/90">
                {featured.skillIdentity.key}
                <br />
                {featured.revisionKey}
              </p>
              <div className="mt-4 flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => onOpen(featured.id, featured.firstStepId)}>
                  {t("learn.read")}
                </Button>
                <Button size="sm" onClick={() => onOpen(featured.id, progress?.currentStepId ?? featured.firstStepId)}>
                  {progress ? t("learn.continue") : t("learn.start")}
                </Button>
              </div>
            </InsetPanel>
          )}
          <InsetPanel className="p-4">
            <h2 className="text-sm font-semibold text-foreground">{t("learn.resumeTitle")}</h2>
            {stale && !progress ? (
              <StaleRevisionBanner
                stale={stale}
                onContinue={() => onOpen(featured.id, stale.currentStepId)}
                onRestart={() => onOpen(featured.id, featured.firstStepId)}
              />
            ) : progress ? (
              <>
                <p className="mt-2 text-sm text-muted-foreground">
                  {t("learn.resumeBody", {
                    step: progress.currentStepId,
                    done: progress.completedStepIds.length,
                    total: featured.stepCount,
                  })}
                </p>
                <div className="mt-4 flex justify-end">
                  <Button size="sm" onClick={() => onOpen(featured.id, progress.currentStepId)}>
                    {t("learn.continue")}
                  </Button>
                </div>
              </>
            ) : (
              <p className="mt-2 text-sm text-muted-foreground">{t("learn.resumeEmpty")}</p>
            )}
          </InsetPanel>
        </div>
      )}
    </div>
  );
}

function GuideReader({
  guideId,
  initialStepId,
  onBack,
  onHome,
}: {
  guideId: string;
  initialStepId?: string;
  onBack: () => void;
  onHome: () => void;
}) {
  const { t } = useTranslation();
  const guideQuery = useTauriQueryWithArgs("get_guide", { id: guideId }, { retry: false });
  const guide = guideQuery.data ?? null;
  const progressQuery = useTauriQueryWithArgs(
    "load_learning_progress",
    { guideId, guideRevisionKey: guide?.revisionKey ?? "" },
    { enabled: Boolean(guide), retry: false },
  );
  const saveProgress = useTauriMutation("save_learning_progress");
  const [stepId, setStepId] = useState(initialStepId);
  const [staleChoice, setStaleChoice] = useState<"current" | "old" | null>(null);
  const [preview, setPreview] = useState<PracticeInstallPreviewDto | null>(null);
  const [installing, setInstalling] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    if (guide) console.debug("guide_open", { identity: guide.skillIdentity.key, revision: guide.revisionKey });
  }, [guide]);

  const progress = progressQuery.data?.current;
  const stale = progressQuery.data?.stale;
  const steps = guide?.steps ?? [];
  const activeId = stepId ?? progress?.currentStepId ?? steps[0]?.id;
  const active = steps.find((step) => step.id === activeId) ?? steps[0];
  const completed = useMemo(() => new Set(progress?.completedStepIds ?? []), [progress]);

  if (guideQuery.isError) {
    return (
      <EmptyState
        title={t("learn.loadFailed")}
        body={String(guideQuery.error)}
        action={
          <Button size="sm" variant="outline" onClick={onBack}>
            {t("learn.back")}
          </Button>
        }
      />
    );
  }
  if (!guide || !active) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const persist = async (nextStep: string, completedIds: string[]) => {
    try {
      await saveProgress.mutateAsync({
        guideId: guide.id,
        guideRevisionKey: guide.revisionKey,
        currentStepId: nextStep,
        completedStepIds: completedIds,
      });
      await progressQuery.refetch();
      return true;
    } catch (error) {
      setActionError(String(error));
      return false;
    }
  };

  const markComplete = async () => {
    const nextCompleted = [...completed];
    if (!nextCompleted.includes(active.id)) nextCompleted.push(active.id);
    const index = steps.findIndex((step) => step.id === active.id);
    const next = steps[index + 1]?.id ?? active.id;
    if (await persist(next, nextCompleted)) {
      setStepId(next);
    }
  };

  const openPractice = async () => {
    setActionError(null);
    try {
      const next = await tauriInvoke("preview_practice_install", { guideId: guide.id, stepId: active.id });
      setPreview(next);
      console.debug("install_prompted", { identity: next.skillIdentity.key, revision: next.skillRevision.key });
    } catch (error) {
      setActionError(String(error));
    }
  };

  const confirmInstall = async () => {
    if (!preview) return;
    setInstalling(true);
    setActionError(null);
    try {
      await tauriInvoke("install_skill", { url: preview.installUrl, name: preview.displayName });
      console.debug("install_confirmed", { identity: preview.skillIdentity.key, revision: preview.skillRevision.key });
      setPreview(null);
      await guideQuery.refetch();
    } catch (error) {
      setActionError(String(error));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-border px-4 py-2">
        <Button variant="ghost" size="sm" onClick={onHome}>
          <ChevronLeft className="size-4" />
          {t("learn.back")}
        </Button>
        <BookOpen className="size-4 text-muted-foreground" />
        <h1 className="truncate text-sm font-semibold">{guide.title}</h1>
      </div>
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <aside className="w-64 shrink-0 overflow-auto border-r border-border p-3">
          {stale && staleChoice === null && (
            <StaleRevisionBanner
              stale={stale}
              onContinue={() => {
                setStaleChoice("old");
                setStepId(stale.currentStepId);
              }}
              onRestart={() => {
                setStaleChoice("current");
                setStepId(guide.steps[0]?.id);
              }}
            />
          )}
          <ol className="mt-2 grid gap-1">
            {steps.map((step, index) => {
              const done = completed.has(step.id);
              const on = step.id === active.id;
              return (
                <li key={step.id}>
                  <button
                    type="button"
                    onClick={() => setStepId(step.id)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px]",
                      on ? "bg-primary/15 text-foreground" : "text-muted-foreground hover:bg-accent/20",
                    )}
                  >
                    <span className="grid size-5 place-items-center rounded-full bg-muted text-[10px]">
                      {done ? <Check className="size-3 text-emerald-500" /> : index + 1}
                    </span>
                    <span className="min-w-0 flex-1 truncate">{step.title}</span>
                    {step.requiresSkill && (
                      <StatusChip tone="warning" size="sm">
                        {t("learn.practice")}
                      </StatusChip>
                    )}
                  </button>
                </li>
              );
            })}
          </ol>
        </aside>
        <section className="min-w-0 flex-1 overflow-auto p-5">
          <div className="flex flex-wrap gap-1.5">
            <StatusChip tone="info">{t(`learn.kind.${active.kind}`)}</StatusChip>
            <StatusChip tone={guide.installed ? "success" : "muted"}>
              {guide.installed ? t("learn.installed") : t("learn.notInstalled")}
            </StatusChip>
            {guide.skillDrift && <StatusChip tone="warning">{t("learn.skillDrift")}</StatusChip>}
          </div>
          <h2 className="mt-3 text-base font-semibold">{active.title}</h2>
          <div className="mt-3 space-y-3 text-sm leading-relaxed text-foreground/90">
            {active.blocks.map((block, index) => (
              <BlockView key={`${active.id}-${index}`} block={block} />
            ))}
          </div>
          {active.requiresSkill && (
            <PracticeInstallPreview
              preview={preview}
              installing={installing}
              onPreview={() => void openPractice()}
              onConfirm={() => void confirmInstall()}
              onDismiss={() => setPreview(null)}
            />
          )}
          {actionError && <p className="mt-3 text-sm text-destructive">{actionError}</p>}
          <div className="mt-6 flex justify-end gap-2">
            <Button variant="outline" size="sm" onClick={onBack}>
              {t("learn.back")}
            </Button>
            <Button size="sm" onClick={() => void markComplete()}>
              {t("learn.completeStep")}
            </Button>
          </div>
        </section>
      </div>
    </div>
  );
}

function BlockView({ block }: { block: GuideBlockDto }) {
  switch (block.type) {
    case "heading":
      return <h3 className="text-sm font-semibold">{block.text}</h3>;
    case "paragraph":
      return <p>{block.text}</p>;
    case "list":
      return block.ordered ? (
        <ol className="list-decimal space-y-1 pl-5">
          {block.items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ol>
      ) : (
        <ul className="list-disc space-y-1 pl-5">
          {block.items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      );
    case "code":
      return <pre className="overflow-auto rounded-md bg-muted p-3 font-mono text-[12px]">{block.code}</pre>;
    case "callout":
      return (
        <InsetPanel className="p-3 text-sm">
          <p>{block.text}</p>
        </InsetPanel>
      );
    default:
      return null;
  }
}

function StaleRevisionBanner({
  stale,
  onContinue,
  onRestart,
}: {
  stale: LearningProgressDto;
  onContinue: () => void;
  onRestart: () => void;
}) {
  const { t } = useTranslation();
  return (
    <InsetPanel className="mb-3 p-3">
      <p className="text-sm text-amber-500 paper:text-amber-700">{t("learn.staleTitle")}</p>
      <p className="mt-1 text-xs text-muted-foreground">{t("learn.staleBody")}</p>
      <p className="mt-1 font-mono text-[10px] break-all text-muted-foreground">{stale.guideRevisionKey}</p>
      <div className="mt-2 flex gap-2">
        <Button size="sm" variant="outline" onClick={onContinue}>
          {t("learn.continueOld")}
        </Button>
        <Button size="sm" onClick={onRestart}>
          {t("learn.restart")}
        </Button>
      </div>
    </InsetPanel>
  );
}

function PracticeInstallPreview({
  preview,
  installing,
  onPreview,
  onConfirm,
  onDismiss,
}: {
  preview: PracticeInstallPreviewDto | null;
  installing: boolean;
  onPreview: () => void;
  onConfirm: () => void;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  if (!preview) {
    return (
      <InsetPanel className="mt-4 p-3">
        <p className="text-sm text-muted-foreground">{t("learn.practiceHint")}</p>
        <Button className="mt-2" size="sm" variant="outline" onClick={onPreview}>
          {t("learn.previewInstall")}
        </Button>
      </InsetPanel>
    );
  }
  return (
    <InsetPanel className="mt-4 p-3">
      <p className="text-sm font-medium">{t("learn.installPreviewTitle")}</p>
      <p className="mt-1 font-mono text-[11px] leading-relaxed break-all text-sky-400/90">
        {preview.skillIdentity.key}
        <br />
        {preview.skillRevision.key}
        <br />
        {preview.installUrl}
      </p>
      <p className="mt-2 text-xs text-muted-foreground">
        {preview.runsAuthorCommands ? t("learn.runsCommands") : t("learn.noAuthorCommands")}
      </p>
      <div className="mt-3 flex gap-2">
        <Button size="sm" variant="outline" onClick={onDismiss}>
          {t("common.cancel")}
        </Button>
        <Button size="sm" disabled={installing} onClick={onConfirm}>
          {installing ? t("common.installing") : t("learn.confirmInstall")}
        </Button>
      </div>
    </InsetPanel>
  );
}

function EmptyState({ title, body, action }: { title: string; body: string; action?: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
      <h2 className="text-base font-semibold">{title}</h2>
      <p className="mt-2 max-w-md text-sm text-muted-foreground">{body}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
