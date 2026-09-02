import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, BookOpen, Bot, Loader2, RefreshCw, X } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useTauriMutation, useTauriQuery, useTauriQueryWithArgs, tauriInvoke } from "../../lib/ipc";
import { navigateToAcpSettings } from "../../lib/utils";
import type { SkillTutorial, SkillTutorialStyle } from "../../types";
import type { GuideDraftDto } from "../../types/generated/GuideDraft";
import { Button } from "../ui/button";
import { ResizablePanel } from "../ui/ResizablePanel";

interface SkillTutorialPanelProps {
  skillName: string;
  onClose: () => void;
}

const TUTORIAL_STYLE_LABELS: Record<SkillTutorialStyle, string> = {
  guided: "settings.acpStyleGuided",
  reference: "settings.acpStyleReference",
  workshop: "settings.acpStyleWorkshop",
};

function formatBytes(bytes: number, locale: string): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unit;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 }).format(value)} ${units[unit]}`;
}

function formatGeneratedAt(value: string, locale: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/**
 * Displays a backend-owned, hash-validated HTML tutorial for one installed Skill.
 * Generated HTML is isolated in a scriptless sandbox; it is never injected into
 * the SkillStar document.
 */
export function SkillTutorialPanel({ skillName, onClose }: SkillTutorialPanelProps) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const locale = i18n.resolvedLanguage || i18n.language || "en";
  const tutorialQuery = useTauriQueryWithArgs(
    "get_skill_tutorial",
    { name: skillName, locale },
    { retry: false, gcTime: 0, networkMode: "always", refetchOnMount: "always" },
  );
  const acpQuery = useTauriQuery("get_acp_config", { retry: false, staleTime: 3_000 });
  const generateTutorial = useTauriMutation("generate_skill_tutorial");
  const [tutorial, setTutorial] = useState<SkillTutorial | null>(null);
  const [showOldVersion, setShowOldVersion] = useState(false);
  const [generationError, setGenerationError] = useState<string | null>(null);

  useEffect(() => {
    setTutorial(null);
    setShowOldVersion(false);
    setGenerationError(null);
  }, [locale, skillName]);

  useEffect(() => {
    // Never promote cached data to "fresh" until this mount's backend hash
    // validation has completed successfully.
    if (!tutorialQuery.data || tutorialQuery.isFetching || tutorialQuery.isError) return;
    setTutorial(tutorialQuery.data);
    setShowOldVersion(false);
  }, [tutorialQuery.data, tutorialQuery.isError, tutorialQuery.isFetching]);

  const acpConfigured = Boolean(
    acpQuery.data?.enabled && acpQuery.data.agent_command && acpQuery.data.agent_command.trim().length > 0,
  );

  const handleGenerate = async (forceRefresh: boolean) => {
    if (!acpConfigured || generateTutorial.isPending) return;
    setGenerationError(null);
    try {
      const next = await generateTutorial.mutateAsync({
        name: skillName,
        locale,
        ...(forceRefresh ? { forceRefresh: true } : {}),
      });
      setTutorial(next);
      queryClient.setQueryData(["get_skill_tutorial", { name: skillName, locale }], next);
      setShowOldVersion(false);
    } catch (error) {
      // Keep `tutorial` untouched so a stale-but-readable artifact remains available.
      setGenerationError(String(error));
    }
  };

  const metadata = tutorial?.metadata;
  const tutorialStyleLabel = metadata?.tutorialStyle ? t(TUTORIAL_STYLE_LABELS[metadata.tutorialStyle]) : null;
  const metadataLabel = metadata
    ? t("skillTutorial.metadata", {
        fileCount: metadata.fileCount,
        totalBytes: formatBytes(metadata.totalBytes, locale),
        generatedAt: formatGeneratedAt(metadata.generatedAt, locale),
      })
    : t("skillTutorial.subtitle", { skillName });
  const html = tutorial?.html?.trim() ? tutorial.html : null;
  const staleMessage =
    tutorial?.staleReason === "generator_changed"
      ? t("skillTutorial.staleGeneratorChanged")
      : t("skillTutorial.staleContentChanged");

  const renderConfigureAcp = () => (
    <div className="rounded-xl border border-violet-500/25 bg-violet-500/10 p-4 text-left">
      <div className="flex items-start gap-3">
        <Bot className="mt-0.5 h-5 w-5 shrink-0 text-violet-400" aria-hidden />
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold text-foreground">{t("skillTutorial.acpMissingTitle")}</h3>
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
            {t("skillTutorial.acpMissingDescription")}
          </p>
          <Button className="mt-3" size="sm" onClick={navigateToAcpSettings}>
            {t("skillTutorial.configureAcp")}
          </Button>
        </div>
      </div>
    </div>
  );

  const renderGenerationError = () =>
    generationError ? (
      <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-3 py-2 text-xs text-destructive">
        {t("skillTutorial.generateFailed", { message: generationError })}
      </div>
    ) : null;

  const renderGenerating = () => (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center">
        <Loader2 className="mx-auto h-8 w-8 animate-spin text-primary motion-reduce:animate-none" aria-hidden />
        <h3 className="mt-4 text-base font-semibold text-foreground">{t("skillTutorial.generating")}</h3>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{t("skillTutorial.generatingHint")}</p>
      </div>
    </div>
  );

  const renderHtml = (documentHtml: string, oldVersion = false) => (
    <div className="min-h-0 flex-1 bg-white">
      <iframe
        className="h-full w-full border-0 bg-white"
        data-testid="skill-tutorial-frame"
        referrerPolicy="no-referrer"
        sandbox=""
        srcDoc={documentHtml}
        title={t(oldVersion ? "skillTutorial.oldIframeTitle" : "skillTutorial.iframeTitle", { skillName })}
      />
    </div>
  );

  let body: ReactNode;
  if ((tutorialQuery.isPending || tutorialQuery.isFetching) && !tutorial) {
    body = (
      <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin motion-reduce:animate-none" aria-hidden />
        {t("skillTutorial.loading")}
      </div>
    );
  } else if (generateTutorial.isPending) {
    body = renderGenerating();
  } else if (tutorialQuery.error && !tutorial) {
    body = (
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="max-w-md text-center">
          <AlertTriangle className="mx-auto h-8 w-8 text-destructive" aria-hidden />
          <h3 className="mt-3 text-base font-semibold">{t("skillTutorial.loadFailed")}</h3>
          <p className="mt-2 break-words text-sm text-muted-foreground">{String(tutorialQuery.error)}</p>
          <div className="mt-4 flex flex-wrap justify-center gap-2">
            <Button variant="outline" onClick={() => void tutorialQuery.refetch()}>
              {t("skillTutorial.retry")}
            </Button>
            {acpQuery.isPending ? null : acpConfigured ? (
              <Button onClick={() => void handleGenerate(true)}>{t("skillTutorial.regenerateLocal")}</Button>
            ) : (
              <Button onClick={navigateToAcpSettings}>{t("skillTutorial.configureAcp")}</Button>
            )}
          </div>
          <div className="mt-3 text-left">{renderGenerationError()}</div>
        </div>
      </div>
    );
  } else if (tutorial?.state === "fresh" && html) {
    body = renderHtml(html);
  } else if (tutorial?.state === "stale") {
    body = (
      <>
        <div className="shrink-0 border-b border-warning/25 bg-warning/10 px-5 py-4">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" aria-hidden />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-sm font-semibold text-foreground">{t("skillTutorial.staleTitle")}</h3>
                <span className="rounded-full border border-warning/30 bg-warning/10 px-2 py-0.5 text-[10px] font-medium text-warning">
                  {t("skillTutorial.oldVersionBadge")}
                </span>
              </div>
              <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{staleMessage}</p>
              <div className="mt-3 flex flex-wrap gap-2">
                {html ? (
                  <Button size="sm" variant="outline" onClick={() => setShowOldVersion((visible) => !visible)}>
                    {showOldVersion ? t("skillTutorial.hideOld") : t("skillTutorial.viewOld")}
                  </Button>
                ) : null}
                {acpConfigured ? (
                  <Button size="sm" onClick={() => void handleGenerate(true)}>
                    <RefreshCw className="h-3.5 w-3.5" aria-hidden />
                    {t("skillTutorial.update")}
                  </Button>
                ) : (
                  <Button size="sm" onClick={navigateToAcpSettings}>
                    {t("skillTutorial.configureAcp")}
                  </Button>
                )}
              </div>
              <div className="mt-3">{renderGenerationError()}</div>
            </div>
          </div>
        </div>
        {showOldVersion && html ? (
          renderHtml(html, true)
        ) : (
          <div className="flex flex-1 items-center justify-center p-8">
            <div className="w-full max-w-md">{!acpConfigured ? renderConfigureAcp() : null}</div>
          </div>
        )}
      </>
    );
  } else {
    body = (
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="w-full max-w-lg text-center">
          <BookOpen className="mx-auto h-10 w-10 text-primary/80" aria-hidden />
          <h3 className="mt-4 text-lg font-semibold text-foreground">{t("skillTutorial.missingTitle")}</h3>
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{t("skillTutorial.missingDescription")}</p>
          <div className="mt-5">
            {acpQuery.isPending ? (
              <Loader2
                className="mx-auto h-5 w-5 animate-spin text-muted-foreground motion-reduce:animate-none"
                aria-hidden
              />
            ) : acpConfigured ? (
              <Button onClick={() => void handleGenerate(false)}>{t("skillTutorial.generate")}</Button>
            ) : (
              renderConfigureAcp()
            )}
          </div>
          <div className="mt-3">{renderGenerationError()}</div>
        </div>
      </div>
    );
  }

  return (
    <ResizablePanel
      className="bg-background"
      defaultWidth={960}
      maxWidthPercent={96}
      minWidth={520}
      storageKey="skill-tutorial-width"
    >
      <header className="flex shrink-0 items-center gap-3 border-b border-border px-5 py-4">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10">
          <BookOpen className="h-4 w-4 text-primary" aria-hidden />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="truncate text-heading-sm">{t("skillTutorial.title")}</h2>
            {tutorial?.state === "fresh" ? (
              <span className="rounded-full border border-success/25 bg-success/10 px-2 py-0.5 text-[10px] font-medium text-success">
                {t("skillTutorial.freshBadge")}
              </span>
            ) : null}
            {tutorialStyleLabel ? (
              <span className="rounded-full border border-border bg-muted/60 px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
                {tutorialStyleLabel}
              </span>
            ) : null}
          </div>
          <p className="mt-0.5 truncate text-[11px] text-muted-foreground">{metadataLabel}</p>
        </div>
        <button
          type="button"
          aria-label={t("common.close")}
          className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-ring"
          onClick={onClose}
        >
          <X className="h-4 w-4" aria-hidden />
        </button>
      </header>
      {html ? <ConvertToDraftSection skillName={skillName} locale={locale} /> : null}
      <div className="flex min-h-0 flex-1 flex-col">{body}</div>
    </ResizablePanel>
  );
}

function ConvertToDraftSection({ skillName, locale }: { skillName: string; locale: string }) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<GuideDraftDto | null>(null);
  const [saved, setSaved] = useState<GuideDraftDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const previewDraft = async () => {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      setPreview(await tauriInvoke("preview_guide_draft", { name: skillName, locale }));
    } catch (caught) {
      setPreview(null);
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  };

  const saveDraft = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await tauriInvoke("create_guide_draft", { name: skillName, locale });
      setSaved(next);
      setPreview(next);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="shrink-0 border-b border-border px-5 py-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void previewDraft()}>
          {t("skillTutorial.convertDraft")}
        </Button>
        <p className="text-xs text-muted-foreground">{t("skillTutorial.convertLocalOnly")}</p>
      </div>
      {error ? (
        <p className="mt-2 text-xs text-destructive">{t("skillTutorial.convertFailed", { message: error })}</p>
      ) : null}
      {saved ? (
        <p className="mt-2 font-mono text-[11px] break-all text-muted-foreground">
          {t("skillTutorial.convertSaved", { revisionKey: saved.revisionKey })}
        </p>
      ) : null}
      {preview ? (
        <div className="mt-3 rounded-lg border border-border bg-muted/40 p-3">
          <p className="text-sm font-semibold text-foreground">{t("skillTutorial.convertPreviewTitle")}</p>
          <p className="mt-1 text-sm text-foreground">{preview.title}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("skillTutorial.convertSteps", { count: preview.steps.length })}
          </p>
          <p className="mt-1 font-mono text-[11px] leading-relaxed break-all text-sky-400/90">
            {preview.skillIdentity.key}
            <br />
            {preview.sourceTutorialKey}
          </p>
          <ol className="mt-2 grid gap-1 text-xs text-muted-foreground">
            {preview.steps.map((step) => (
              <li key={step.id}>
                {step.title} · {step.blocks.length} · {step.kind}
              </li>
            ))}
          </ol>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={() => setPreview(null)}>
              {t("skillTutorial.convertDismiss")}
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void saveDraft()}>
              {t("skillTutorial.convertSave")}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
