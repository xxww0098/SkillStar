import { tauriInvoke } from "../../lib/ipc";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertTriangle,
  ArrowRightLeft,
  BookMarked,
  BookOpen,
  Calendar,
  Download,
  Edit3,
  ExternalLink,
  GitBranch,
  RefreshCw,
  Sparkles,
  Square,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { degradedDeploys, useDeployStatus } from "../../features/my-skills/hooks/useDeployStatus";
import { useAiStream } from "../../hooks/useAiStream";
import { formatAiErrorMessage, formatInstalls, navigateToAiSettings } from "../../lib/utils";
import type { MarketplaceSkillDetails, Skill, SkillContent } from "../../types";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { ExternalAnchor } from "../ui/ExternalAnchor";
import { Github as GitHub } from "../ui/icons/Github";
import { InfoTip } from "../ui/InfoTip";
import { LoadingLogo } from "../ui/LoadingLogo";
import { Markdown } from "../ui/Markdown";

const SkillEditor = lazy(() => import("../shared/SkillEditor").then((mod) => ({ default: mod.SkillEditor })));

const SkillReader = lazy(() => import("../shared/SkillReader").then((mod) => ({ default: mod.SkillReader })));

const SkillTutorialPanel = lazy(() =>
  import("../shared/SkillTutorialPanel").then((mod) => ({ default: mod.SkillTutorialPanel })),
);

interface DetailPanelProps {
  skill: Skill | null;
  onClose: () => void;
  onInstall: (url: string, name: string, agentId?: string) => void;
  onUpdate: (name: string) => void;
  onUninstall: (name: string) => void;
  uninstalling?: boolean;
  /** Upstream dropped the Skill with no successor: keep a local copy or remove. */
  onResolveRemoved?: (name: string) => void;
  /** Upstream renamed the Skill: install the successor, carry deployments over, remove this one. */
  onMigrate?: (name: string) => void;
  migrating?: boolean;
  onReinstall?: (url: string, name: string) => void;
  onReadContent?: (name: string) => Promise<SkillContent>;
  onSaveContent?: (name: string, content: string) => Promise<void>;
  onPublish?: (skillName: string) => void;
}

export function DetailPanel({
  skill,
  onClose,
  onInstall,
  onUpdate,
  onUninstall,
  uninstalling,
  onResolveRemoved,
  onMigrate,
  migrating,
  onReinstall,
  onReadContent,
  onSaveContent,
  onPublish,
}: DetailPanelProps) {
  const { t } = useTranslation();
  const upstreamChange = skill?.installed && skill.skill_type !== "local" ? (skill.upstream_change ?? null) : null;
  const upstreamSuccessor = upstreamChange?.kind === "removed" ? upstreamChange.successor : null;
  const prefersReducedMotion = useReducedMotion();
  const [editing, setEditing] = useState(false);
  const [reading, setReading] = useState(false);
  const [tutorialOpen, setTutorialOpen] = useState(false);

  // Close on Escape key
  useEffect(() => {
    if (!skill || editing || reading || tutorialOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [skill, editing, reading, tutorialOpen, onClose]);

  // ── AI Quick Read ─────────────────────────────────────────────
  // Streaming state machine, AI readiness, cancellation and the safety
  // timeout all live in the shared hook (same surface as SkillReader/Editor).
  const quickRead = useAiStream({
    command: "ai_summarize_skill_stream",
    eventChannel: "ai://summarize-stream",
  });
  const summaryAiConfigured = quickRead.aiConfigured;
  const locale = quickRead.locale;

  // Marketplace detail fetching
  const [skillDetails, setSkillDetails] = useState<MarketplaceSkillDetails | null>(null);
  const quickReadCacheRef = useRef<Map<string, string>>(new Map());

  // Guard async setState after component unmount
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Fetch marketplace details for remote skills
  const fetchDetails = useCallback(async (source: string, name: string) => {
    try {
      const readLocal = () =>
        tauriInvoke("get_skill_detail_local", {
          source,
          name,
        });
      const result = await readLocal();
      if (!mountedRef.current) return;
      setSkillDetails(result.data);
      if (result.snapshot_status === "stale") {
        void (async () => {
          try {
            await tauriInvoke("sync_marketplace_scope", {
              scope: `skill_detail:${source}/${name}`.toLowerCase(),
            });
            const fresh = await readLocal();
            if (!mountedRef.current) return;
            setSkillDetails(fresh.data);
          } catch (e) {
            if (import.meta.env.DEV) console.warn("[DetailPanel] Failed to refresh local skill detail:", e);
          }
        })();
      }
    } catch (e) {
      if (import.meta.env.DEV) console.warn("[DetailPanel] Failed to fetch skill details:", e);
      if (!mountedRef.current) return;
      setSkillDetails(null);
    }
  }, []);

  // Reset state when skill changes
  useEffect(() => {
    // Drop any in-flight stream for the previous skill, then restore the
    // cached quick-read for this one.
    quickRead.cancel();
    const cacheKey = `${locale}::${skill?.name ?? ""}`;
    quickRead.hydrate(quickReadCacheRef.current.get(cacheKey) ?? null, null);
    quickRead.setVisible(false);
    quickRead.setError(null);

    setSkillDetails(null);
    setReading(false);
    setTutorialOpen(false);

    // Fetch details for remote marketplace skills
    if (skill && skill.source) {
      fetchDetails(skill.source, skill.name);
    }
  }, [
    skill?.name,
    skill?.description,
    skill?.localized_description,
    skill?.installed,
    skill?.source,
    locale,
    fetchDetails,
    quickRead.cancel,
    quickRead.hydrate,
    quickRead.setVisible,
    quickRead.setError,
  ]);

  const handleQuickRead = async () => {
    // Cancel in-progress
    if (quickRead.loading) {
      quickRead.cancel();
      if (!quickRead.content) quickRead.setVisible(false);
      return;
    }

    if (quickRead.visible) {
      quickRead.dismiss();
      return;
    }

    if (quickRead.content) {
      quickRead.setVisible(true);
      return;
    }

    if (!skill || !onReadContent || !summaryAiConfigured) return;

    try {
      const skillContent = await onReadContent(skill.name);
      const result = await quickRead.execute(skillContent.content);
      if (result != null) {
        // Cache completed summary (language-aware)
        quickReadCacheRef.current.set(`${locale}::${skill.name}`, result);
      }
    } catch (e) {
      // onReadContent failed before the stream started; execute reports its
      // own errors through the hook state.
      quickRead.setError(String(e));
    }
  };

  const canEdit = skill?.installed && onReadContent && onSaveContent;

  // Per-agent deploy kind — fetched lazily when the panel opens for an installed
  // skill. Only degraded deployments (copy fallback / dangling link) get a badge.
  const deployStatus = useDeployStatus(skill?.installed ? skill.name : null);
  const degraded = degradedDeploys(deployStatus);

  // skills.sh URL
  const skillsShUrl = skill?.source ? `https://skills.sh/${skill.source}/${skill.name}` : null;
  const rawDescription = skill?.description?.trim() || "";
  // Use enriched summary from detail fetch when available
  const enrichedDescription = skillDetails?.summary?.trim() || rawDescription;
  const hasDescription = enrichedDescription.length > 0;
  const localizedQuickReadError = formatAiErrorMessage(quickRead.error, t);
  const displayDescription = enrichedDescription;

  return (
    <AnimatePresence mode="sync">
      {editing && skill && onReadContent && onSaveContent && (
        <motion.div
          key="skill-editor"
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: 20 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          className="absolute inset-0 z-50"
        >
          <Suspense
            fallback={
              <div className="absolute right-0 top-0 bottom-0 w-full max-w-xl h-full border-l border-border bg-background shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
                <LoadingLogo size="md" label={t("detailPanel.reading")} />
              </div>
            }
          >
            <SkillEditor
              skillName={skill.name}
              onClose={() => setEditing(false)}
              onRead={onReadContent}
              onSave={onSaveContent}
            />
          </Suspense>
        </motion.div>
      )}

      {reading && skill && skillDetails?.readme && (
        <motion.div
          key="skill-reader"
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: 20 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          className="absolute inset-0 z-50"
        >
          <Suspense
            fallback={
              <div className="absolute right-0 top-0 bottom-0 w-full max-w-xl h-full border-l border-border bg-background shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
                <LoadingLogo size="md" label={t("detailPanel.reading")} />
              </div>
            }
          >
            <SkillReader skillName={skill.name} content={skillDetails.readme} onClose={() => setReading(false)} />
          </Suspense>
        </motion.div>
      )}

      {tutorialOpen && skill?.installed && (
        <motion.div
          key="skill-tutorial"
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: 20 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          className="absolute inset-0 z-50"
        >
          <Suspense
            fallback={
              <div className="absolute right-0 top-0 bottom-0 z-50 flex h-full w-full max-w-xl items-center justify-center overflow-hidden rounded-bl-xl rounded-tl-xl border-l border-border bg-background shadow-2xl">
                <LoadingLogo size="md" label={t("skillTutorial.loading")} />
              </div>
            }
          >
            <SkillTutorialPanel skillName={skill.name} onClose={() => setTutorialOpen(false)} />
          </Suspense>
        </motion.div>
      )}

      {skill && !editing && !reading && !tutorialOpen && (
        <motion.aside
          key="skill-detail"
          initial={{ x: prefersReducedMotion ? 0 : "100%", opacity: prefersReducedMotion ? 0 : 1 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: prefersReducedMotion ? 0 : "100%", opacity: prefersReducedMotion ? 0 : 1 }}
          transition={{ duration: prefersReducedMotion ? 0.01 : 0.28, ease: [0.22, 1, 0.36, 1] }}
          // Opaque card: backdrop-filter + this slide transform paints a blank
          // compositor layer in WKWebView, especially on the paper theme.
          className="absolute right-0 top-0 bottom-0 z-50 flex h-full w-full max-w-md flex-col overflow-hidden rounded-l-xl border-l border-border bg-card shadow-[0_24px_80px_-48px_var(--color-shadow)]"
        >
          {/* Header — pinned */}
          <div className="flex shrink-0 items-center justify-between border-b border-border p-4">
            <h2 className="text-heading-sm truncate">{skill.name}</h2>
            <button
              type="button"
              onClick={onClose}
              aria-label={t("common.close")}
              className="p-2 rounded-md hover:bg-muted text-muted-foreground transition-colors cursor-pointer focus-ring"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Scrollable content */}
          <div className="flex-1 overflow-y-auto overscroll-y-contain">
            <div className="p-5 space-y-5">
              {/* Meta */}
              <div className="flex items-center gap-3 flex-wrap">
                {skill.rank && (
                  <Badge variant="outline" className="tabular-nums font-semibold">
                    {skill.rank}
                  </Badge>
                )}
                {skill.category !== "None" && (
                  <Badge
                    variant={
                      skill.category === "Hot"
                        ? "hot"
                        : skill.category === "Popular"
                          ? "popular"
                          : skill.category === "Rising"
                            ? "rising"
                            : "new"
                    }
                  >
                    {skill.category}
                  </Badge>
                )}
                {skill.stars > 0 && (
                  <div className="flex items-center gap-1 text-caption">
                    <Download className="w-3.5 h-3.5 text-primary/60" />
                    {skillDetails?.weekly_installs
                      ? `${skillDetails.weekly_installs} / week`
                      : `${formatInstalls(skill.stars)} installs`}
                  </div>
                )}
                {skill.skill_type === "local" && <span className="text-caption">local</span>}
                {skill.source && <span className="text-caption break-all">by {skill.source}</span>}
                {!skill.source && skill.author && <span className="text-caption break-all">by {skill.author}</span>}
                {skillDetails?.github_stars != null && skillDetails.github_stars > 0 && (
                  <div className="flex items-center gap-1 text-caption">
                    <Star className="w-3.5 h-3.5 text-amber-400/70" />
                    {skillDetails.github_stars}
                  </div>
                )}
                {skillDetails?.first_seen && (
                  <div className="flex items-center gap-1 text-caption">
                    <Calendar className="w-3.5 h-3.5 text-muted-foreground" />
                    {skillDetails.first_seen}
                  </div>
                )}
              </div>

              {/* Degraded deploys — copy fallback (e.g. Windows without Developer Mode) or dangling link */}
              {degraded.length > 0 && (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                  {degraded.map((row) => (
                    <div key={row.agent_id} className="flex items-center gap-1" title={row.target_path}>
                      <span className="text-caption">{row.agent_name}</span>
                      {row.kind === "copy" ? (
                        <>
                          <Badge variant="outline" className="text-micro px-1.5 py-0 h-4 font-normal">
                            {t("projects.deployCopy")}
                          </Badge>
                          <InfoTip content={t("projects.deployModeCopyHint")} />
                        </>
                      ) : (
                        <Badge variant="warning" className="text-micro px-1.5 py-0 h-4 font-normal">
                          <AlertTriangle className="w-3 h-3" />
                          {t("projects.deploySymlink")}
                        </Badge>
                      )}
                    </div>
                  ))}
                </div>
              )}

              {/* Description */}
              <div className="space-y-2">
                <div className="rounded-xl border border-border/80 bg-muted/25 px-4 py-3">
                  {hasDescription ? (
                    <Markdown
                      streaming={false}
                      className="text-body leading-relaxed [&_p]:my-0 [&_p]:whitespace-pre-wrap [&_p+ul]:mt-3 [&_p+ol]:mt-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1.5 [&_strong]:text-foreground"
                    >
                      {displayDescription}
                    </Markdown>
                  ) : (
                    <p className="text-body leading-relaxed">{t("detailPanel.noDescription")}</p>
                  )}
                </div>
                {/* AI Actions Row */}
                <div className="flex items-center gap-2">
                  {skill.installed && onReadContent && summaryAiConfigured && (
                    <button
                      onClick={handleQuickRead}
                      className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition duration-300 cursor-pointer shadow-sm relative overflow-hidden group focus-ring ${
                        quickRead.loading
                          ? "bg-destructive/10 text-destructive border border-destructive/20"
                          : quickRead.visible
                            ? "bg-primary/10 text-primary border border-primary/20"
                            : "bg-gradient-to-br from-background to-muted/50 border border-border hover:border-primary/40 text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      <div className="absolute inset-0 bg-gradient-to-r from-primary/0 via-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
                      {quickRead.loading ? (
                        <Square className="w-3.5 h-3.5 fill-current animate-pulse relative z-10" />
                      ) : (
                        <Sparkles className="w-3.5 h-3.5 relative z-10" />
                      )}
                      <span className="relative z-10">
                        {quickRead.loading
                          ? t("common.cancel")
                          : quickRead.visible
                            ? t("detailPanel.hideQuickRead")
                            : t("detailPanel.aiQuickRead")}
                      </span>
                    </button>
                  )}
                </div>
              </div>

              {/* AI Quick Read Content */}
              {skill.installed &&
                onReadContent &&
                summaryAiConfigured &&
                (quickRead.error || quickRead.loading || quickRead.visible) && (
                  <div className="space-y-2">
                    {localizedQuickReadError && (
                      <div className="text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2">
                        {localizedQuickReadError}
                      </div>
                    )}

                    {!quickRead.loading && quickRead.visible && quickRead.content && quickRead.wasNonStreaming && (
                      <div className="text-xs text-muted-foreground bg-muted/40 rounded-md px-3 py-2 border border-border">
                        {t("detailPanel.nonStreamingQuickReadNotice")}
                      </div>
                    )}

                    {quickRead.visible && quickRead.content && (
                      <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
                        <Markdown
                          streaming={quickRead.loading}
                          className="text-xs [&_p]:my-1 [&_strong]:text-primary/90"
                        >
                          {quickRead.content}
                        </Markdown>
                      </div>
                    )}
                  </div>
                )}

              {skill.installed && onReadContent && !summaryAiConfigured && (
                <div className="rounded-lg border border-border bg-card px-3 py-2 flex items-center gap-2">
                  <p className="text-xs text-muted-foreground flex-1">{t("detailPanel.aiPromptHint")}</p>
                  <button
                    onClick={navigateToAiSettings}
                    className="px-2 py-1 rounded-md text-micro font-medium border border-border hover:bg-muted transition-colors cursor-pointer focus-ring"
                  >
                    {t("detailPanel.goToAiConfig")}
                  </button>
                </div>
              )}

              {/* skills.sh link */}
              {skillsShUrl && (
                <ExternalAnchor
                  href={skillsShUrl}
                  className="flex items-center gap-2 text-xs text-primary/70 hover:text-primary transition-colors"
                >
                  <ExternalLink className="w-3.5 h-3.5" />
                  {t("detailPanel.viewOnSkillsSh")}
                </ExternalAnchor>
              )}

              {/* Git info — only for hub (git-backed) skills */}
              {skill.skill_type !== "local" && skill.git_url && (
                <div className="space-y-2">
                  <ExternalAnchor
                    href={skill.git_url.startsWith("http") ? skill.git_url : `https://${skill.git_url}`}
                    className="flex items-center gap-2 text-xs text-primary/70 hover:text-primary transition-colors"
                  >
                    <GitBranch className="w-3.5 h-3.5 shrink-0" />
                    <span className="truncate font-mono">{skill.git_url}</span>
                  </ExternalAnchor>

                  <div className="text-caption">
                    {t("detailPanel.updated")} {new Date(skill.last_updated).toLocaleDateString()}
                  </div>
                </div>
              )}

              {/* Topics */}
              {skill.topics.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {skill.topics.map((topic) => (
                    <Badge key={topic} variant="outline">
                      {topic}
                    </Badge>
                  ))}
                </div>
              )}

              {/* SKILL.md — reader uses marketplace snapshot; skip when editor is available (same AI preview there). */}
              {skill.installed && (
                <Button
                  variant="outline"
                  className="w-full border-primary/30 text-primary hover:bg-primary/10 hover:text-primary"
                  onClick={() => setTutorialOpen(true)}
                >
                  <BookMarked className="mr-2 h-4 w-4" />
                  {t("skillTutorial.open")}
                </Button>
              )}

              {skillDetails?.readme && !canEdit && (
                <Button variant="outline" className="w-full" onClick={() => setReading(true)}>
                  <BookOpen className="w-4 h-4 mr-2" />
                  {t("detailPanel.readSkillMd")}
                </Button>
              )}

              {/* Edit Button (only for installed skills) */}
              {canEdit && (
                <Button variant="outline" className="w-full" onClick={() => setEditing(true)}>
                  <Edit3 className="w-4 h-4 mr-2" />
                  {t("detailPanel.editSkillMd")}
                </Button>
              )}

              {/* Publish Button — for local skills */}
              {skill.installed && skill.skill_type === "local" && onPublish && (
                <Button
                  variant="outline"
                  className="w-full border-primary/30 text-primary hover:bg-primary/15 hover:text-primary"
                  onClick={() => onPublish(skill.name)}
                >
                  <GitHub className="w-4 h-4 mr-2" />
                  {t("detailPanel.publishToGithub")}
                </Button>
              )}
            </div>
          </div>

          {/* Sticky action bar */}
          <div className="shrink-0 space-y-2 border-t border-border bg-card p-4">
            {skill.installed ? (
              <>
                {upstreamChange?.kind === "removed" && (
                  <div
                    className={
                      upstreamSuccessor
                        ? "rounded-xl border border-violet-500/30 bg-violet-500/[0.06] px-3 py-2.5 space-y-2"
                        : "rounded-xl border border-rose-500/30 bg-rose-500/[0.06] px-3 py-2.5 space-y-2"
                    }
                  >
                    <div className="flex items-start gap-2">
                      {upstreamSuccessor ? (
                        <ArrowRightLeft className="w-4 h-4 mt-0.5 shrink-0 text-violet-500 dark:text-violet-300" />
                      ) : (
                        <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0 text-rose-500 dark:text-rose-300" />
                      )}
                      <div className="min-w-0 space-y-1">
                        <p className="text-sm font-semibold text-foreground">
                          {t(
                            upstreamSuccessor ? "detailPanel.upstreamRenamedTitle" : "detailPanel.upstreamRemovedTitle",
                          )}
                        </p>
                        {upstreamSuccessor ? (
                          <p className="text-xs text-muted-foreground leading-5 break-words">
                            {t("detailPanel.upstreamRenamedDesc", {
                              name: upstreamSuccessor.skill_id,
                              folder: upstreamSuccessor.folder_path,
                            })}{" "}
                            {upstreamSuccessor.similarity === null
                              ? t("detailPanel.upstreamSameName")
                              : t("detailPanel.upstreamSimilarity", { similarity: upstreamSuccessor.similarity })}
                          </p>
                        ) : (
                          <p className="text-xs text-muted-foreground leading-5 break-words">
                            {t("detailPanel.upstreamRemovedDesc", {
                              source: skill.source ?? skill.git_url,
                              folder: skill.name,
                            })}
                          </p>
                        )}
                      </div>
                    </div>
                    {upstreamSuccessor && onMigrate ? (
                      <>
                        <Button className="w-full" disabled={migrating} onClick={() => onMigrate(skill.name)}>
                          <ArrowRightLeft className="w-4 h-4 mr-2" />
                          {migrating
                            ? t("skillCard.migrating")
                            : t("detailPanel.migrateToSuccessor", { name: upstreamSuccessor.skill_id })}
                        </Button>
                        <p className="text-[11px] leading-4 text-muted-foreground">{t("detailPanel.migrateHint")}</p>
                      </>
                    ) : null}
                    {onResolveRemoved && (
                      <Button
                        variant={upstreamSuccessor ? "outline" : "default"}
                        className="w-full"
                        onClick={() => onResolveRemoved(skill.name)}
                      >
                        {t("detailPanel.resolveRemoved")}
                      </Button>
                    )}
                  </div>
                )}

                {skill.update_available && skill.skill_type !== "local" && (
                  <Button className="w-full" onClick={() => onUpdate(skill.name)}>
                    <RefreshCw className="w-4 h-4 mr-2" />
                    {t("detailPanel.updateAvailable")}
                  </Button>
                )}

                <div className="flex gap-2">
                  {onReinstall && skill.skill_type !== "local" && !upstreamChange && (
                    <Button
                      variant="secondary"
                      className="flex-1"
                      onClick={() => onReinstall(skill.git_url, skill.name)}
                    >
                      <RefreshCw className="w-4 h-4 mr-2" />
                      {t("detailPanel.reinstall")}
                    </Button>
                  )}
                  <Button
                    variant="destructive"
                    className="flex-1"
                    disabled={uninstalling}
                    onClick={() => onUninstall(skill.name)}
                  >
                    <Trash2 className="w-4 h-4 mr-2" />
                    {uninstalling ? t("common.uninstalling") : t("common.uninstall")}
                  </Button>
                </div>
              </>
            ) : (
              <Button className="w-full" onClick={() => onInstall(skill.git_url, skill.name)}>
                <Download className="w-4 h-4" />
                {t("common.install")}
              </Button>
            )}
          </div>
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
