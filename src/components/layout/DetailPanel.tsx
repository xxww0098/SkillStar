import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "framer-motion";
import {
  BookOpen,
  Calendar,
  Check,
  Download,
  Edit3,
  ExternalLink,
  GitBranch,
  Languages,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Square,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAiStream } from "../../hooks/useAiStream";
import { unwrapOuterMarkdownFence } from "../../lib/frontmatter";
import { formatAiErrorMessage, formatInstalls, navigateToAiSettings } from "../../lib/utils";
import type {
  AiStreamPayload,
  LocalFirstResult,
  MarketplaceSkillDetails,
  ShortTextTranslationResult,
  Skill,
  SkillContent,
} from "../../types";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { ExternalAnchor } from "../ui/ExternalAnchor";
import { Github as GitHub } from "../ui/icons/Github";
import { LoadingLogo } from "../ui/LoadingLogo";
import { Markdown } from "../ui/Markdown";
import { Skeleton } from "../ui/Skeleton";

const AUTO_TRANSLATE_DESCRIPTION_KEY = "skillstar:auto-translate-description";
const AUTO_TRANSLATE_DESCRIPTION_TIMEOUT_MS = 6000;

const SkillEditor = lazy(() => import("../shared/SkillEditor").then((mod) => ({ default: mod.SkillEditor })));

const SkillReader = lazy(() => import("../shared/SkillReader").then((mod) => ({ default: mod.SkillReader })));

interface DetailPanelProps {
  skill: Skill | null;
  onClose: () => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  onUninstall: (name: string) => void;
  uninstalling?: boolean;
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
  onReinstall,
  onReadContent,
  onSaveContent,
  onPublish,
}: DetailPanelProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [reading, setReading] = useState(false);

  // Close on Escape key
  useEffect(() => {
    if (!skill || editing || reading) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [skill, editing, reading, onClose]);

  // ── Description Translation (via reusable hook) ─────────────
  const descriptionStream = useAiStream({
    command: "ai_translate_short_text_stream_with_source",
    eventChannel: "ai://translate-stream",
    requiresAiConfig: false,
    parseInvokeResult: (raw) => {
      const result = raw as ShortTextTranslationResult;
      return {
        text: unwrapOuterMarkdownFence(result.text).trim(),
        provider: result.source,
      };
    },
  });

  // Alias hook state for readability in JSX
  const translatedDescription = descriptionStream.content;
  const descriptionTranslationVisible = descriptionStream.visible;
  const translatingDescription = descriptionStream.loading;
  const descriptionHasDelta = descriptionStream.hasDelta;
  const descriptionTranslateError = descriptionStream.error;
  const descriptionTranslationProvider = descriptionStream.provider as "ai" | "mymemory" | null;
  const descriptionTranslationSource = descriptionStream.source;
  const aiConfigured = descriptionStream.aiConfigured;
  const shortDescriptionTranslationEnabled = true;

  // AI Quick Read
  const [quickReadContent, setQuickReadContent] = useState<string | null>(null);
  const [quickReadVisible, setQuickReadVisible] = useState(false);
  const [quickReading, setQuickReading] = useState(false);
  const [quickReadHasDelta, setQuickReadHasDelta] = useState(false);
  const [quickReadWasNonStreaming, setQuickReadWasNonStreaming] = useState(false);
  const [quickReadError, setQuickReadError] = useState<string | null>(null);
  const targetLanguage = descriptionStream.targetLanguage;

  const [descriptionDeferTimedOut, setDescriptionDeferTimedOut] = useState(false);
  const [autoTranslateDescription, setAutoTranslateDescription] = useState<boolean>(() => {
    try {
      return localStorage.getItem(AUTO_TRANSLATE_DESCRIPTION_KEY) === "true";
    } catch {
      return false;
    }
  });

  // Marketplace detail fetching
  const [skillDetails, setSkillDetails] = useState<MarketplaceSkillDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const quickReadCacheRef = useRef<Map<string, string>>(new Map());

  // Cancel refs (quick read only — description translation is managed by useAiStream)
  const activeQuickReadIdRef = useRef<string | null>(null);
  const quickReadUnlistenRef = useRef<(() => void) | null>(null);
  const autoTranslateAttemptedKeyRef = useRef<string | null>(null);
  // Guard async setState after component unmount
  const mountedRef = useRef(true);

  // Cleanup event listeners on unmount
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      activeQuickReadIdRef.current = null;
      if (quickReadUnlistenRef.current) {
        quickReadUnlistenRef.current();
        quickReadUnlistenRef.current = null;
      }
    };
  }, []);

  // Fetch marketplace details for remote skills
  const fetchDetails = useCallback(async (source: string, name: string) => {
    setDetailsLoading(true);
    try {
      const readLocal = () =>
        invoke<LocalFirstResult<MarketplaceSkillDetails>>("get_skill_detail_local", {
          source,
          name,
        });
      const result = await readLocal();
      if (!mountedRef.current) return;
      setSkillDetails(result.data);
      if (result.snapshot_status === "stale") {
        void (async () => {
          try {
            await invoke("sync_marketplace_scope", {
              scope: `skill_detail:${source}/${name}`.toLowerCase(),
            });
            const fresh = await readLocal();
            if (!mountedRef.current) return;
            setSkillDetails(fresh.data);
          } catch (e) {
            console.warn("[DetailPanel] Failed to refresh local skill detail:", e);
          }
        })();
      }
    } catch (e) {
      console.warn("[DetailPanel] Failed to fetch skill details:", e);
      if (!mountedRef.current) return;
      setSkillDetails(null);
    } finally {
      if (mountedRef.current) setDetailsLoading(false);
    }
  }, []);

  // Reset state when skill changes
  useEffect(() => {
    // Restore cached quick-read, and reset description translation state.
    const cacheKey = `${targetLanguage}::${skill?.name ?? ""}`;
    const cachedQuickRead = quickReadCacheRef.current.get(cacheKey) ?? null;
    setQuickReadContent(cachedQuickRead);
    setQuickReadVisible(false);
    setQuickReadHasDelta(false);
    setQuickReadWasNonStreaming(false);
    setQuickReadError(null);

    // Hydrate from localized_description (pre-cached in Rust)
    if (skill?.localized_description) {
      descriptionStream.hydrate(skill.localized_description, skill.description?.trim() || "");
      descriptionStream.setVisible(true);
    } else {
      descriptionStream.hydrate(null, null);
      descriptionStream.setVisible(false);
    }
    descriptionStream.setError(null);

    setDescriptionDeferTimedOut(false);
    setSkillDetails(null);
    setReading(false);

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
    targetLanguage,
    fetchDetails,
  ]);

  const handleTranslateDescription = useCallback(
    async (mode: "manual" | "auto" = "manual") => {
      const renderedDescription = skillDetails?.summary?.trim() || skill?.description?.trim() || "";
      if (!renderedDescription || !shortDescriptionTranslationEnabled) return;

      if (mode === "auto") {
        // Auto mode: skip if already translating the same text, or if visible/cached for the same text
        if (translatingDescription && descriptionTranslationSource === renderedDescription) return;
        if (descriptionTranslationVisible && descriptionTranslationSource === renderedDescription) return;
        if (translatedDescription != null && descriptionTranslationSource === renderedDescription) {
          descriptionStream.setVisible(true);
          return;
        }
        // If we are currently translating a DIFFERENT text, we should ideally cancel it and start the new one.
        if (translatingDescription) {
          descriptionStream.cancel();
        }
      }
      // Manual mode: use the hook's built-in toggle/cancel behavior
      await descriptionStream.execute(renderedDescription);
    },
    [
      shortDescriptionTranslationEnabled,
      descriptionTranslationSource,
      descriptionTranslationVisible,
      translatingDescription,
      translatedDescription,
      skill,
      skillDetails?.summary,
      descriptionStream,
    ],
  );

  const handleAiRetranslateDescription = useCallback(async () => {
    if (!aiConfigured || translatingDescription) return;
    const renderedDescription = skillDetails?.summary?.trim() || skill?.description?.trim() || "";
    if (!renderedDescription) return;

    await descriptionStream.execute(renderedDescription, {
      forceRefresh: true,
      keepVisibleWhileLoading: true,
      extraInvokeParams: { forceAi: true },
    });
  }, [aiConfigured, translatingDescription, skill, skillDetails?.summary, descriptionStream]);

  // Deferred timeout for auto-translate
  useEffect(() => {
    if (!autoTranslateDescription || !translatingDescription) return;
    const renderedDescription = skillDetails?.summary?.trim() || skill?.description?.trim() || "";
    if (!renderedDescription) return;
    if (translatedDescription != null || descriptionTranslateError) return;

    const timer = window.setTimeout(() => {
      setDescriptionDeferTimedOut(true);
    }, AUTO_TRANSLATE_DESCRIPTION_TIMEOUT_MS);

    return () => window.clearTimeout(timer);
  }, [
    autoTranslateDescription,
    descriptionTranslateError,
    descriptionTranslationSource,
    skill?.description,
    skillDetails?.summary,
    translatedDescription,
    translatingDescription,
  ]);

  // Auto-translate trigger
  useEffect(() => {
    if (!skill || !autoTranslateDescription || !shortDescriptionTranslationEnabled) return;
    // If the skill already ships with a pre-cached localized_description
    // (hydrated in the reset effect), skip auto-translate entirely.
    // We check the prop directly because effects from the same render cycle
    // can't see each other's queued state updates.
    if (skill.localized_description) return;
    const renderedDescription = skillDetails?.summary?.trim() || skill.description?.trim() || "";
    if (!renderedDescription) return;
    const autoKey = `${targetLanguage}::${skill.name}::${renderedDescription}`;
    if (autoTranslateAttemptedKeyRef.current === autoKey) return;
    autoTranslateAttemptedKeyRef.current = autoKey;
    void handleTranslateDescription("auto");
  }, [
    shortDescriptionTranslationEnabled,
    autoTranslateDescription,
    handleTranslateDescription,
    skill,
    skillDetails?.summary,
    targetLanguage,
  ]);

  const toggleAutoTranslateDescription = (enabled: boolean) => {
    setAutoTranslateDescription(enabled);
    autoTranslateAttemptedKeyRef.current = null;
    try {
      localStorage.setItem(AUTO_TRANSLATE_DESCRIPTION_KEY, String(enabled));
    } catch {
      // ignore localStorage write errors
    }
  };

  const handleQuickRead = async () => {
    // Cancel in-progress
    if (quickReading) {
      activeQuickReadIdRef.current = null;
      if (quickReadUnlistenRef.current) {
        quickReadUnlistenRef.current();
        quickReadUnlistenRef.current = null;
      }
      setQuickReading(false);
      if (!quickReadContent) {
        setQuickReadVisible(false);
      }
      return;
    }

    if (quickReadVisible) {
      setQuickReadVisible(false);
      return;
    }

    if (quickReadContent) {
      setQuickReadVisible(true);
      return;
    }

    if (!skill || !onReadContent || !aiConfigured) return;

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `detail-summary-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeQuickReadIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setQuickReading(true);
    setQuickReadError(null);
    setQuickReadHasDelta(false);
    setQuickReadWasNonStreaming(false);
    setQuickReadVisible(true);
    setQuickReadContent(null);

    let rafId: number | null = null;
    const flushDelta = () => {
      rafId = null;
      if (activeQuickReadIdRef.current !== requestId) return;
      setQuickReadContent(streamedRaw);
      if (deltaCount >= 2) setQuickReadHasDelta(true);
    };

    try {
      const unlisten = await listen<AiStreamPayload>("ai://summarize-stream", (event) => {
        if (activeQuickReadIdRef.current !== requestId) return;
        const payload = event.payload;
        if (payload.requestId !== requestId) return;

        if (payload.event === "delta" && payload.delta) {
          deltaCount += 1;
          streamedRaw += payload.delta;
          if (rafId == null) {
            rafId = requestAnimationFrame(flushDelta);
          }
          return;
        }

        if (payload.event === "error" && payload.message) {
          setQuickReadError(payload.message);
        }
      });
      quickReadUnlistenRef.current = unlisten;

      const skillContent = await onReadContent(skill.name);
      const result = await invoke<string>("ai_summarize_skill_stream", {
        requestId,
        content: skillContent.content,
      });

      if (activeQuickReadIdRef.current !== requestId) return;
      setQuickReadContent(result);
      setQuickReadVisible(true);
      setQuickReadWasNonStreaming(deltaCount < 2);
      // Cache completed summary (language-aware)
      if (skill) {
        const cacheKey = `${targetLanguage}::${skill.name}`;
        quickReadCacheRef.current.set(cacheKey, result);
      }
    } catch (e) {
      if (activeQuickReadIdRef.current !== requestId) return;
      setQuickReadHasDelta(deltaCount >= 2);
      setQuickReadWasNonStreaming(false);
      if (!streamedRaw.trim()) {
        setQuickReadContent(null);
        setQuickReadVisible(false);
      } else {
        setQuickReadContent(streamedRaw);
        setQuickReadVisible(true);
      }
      setQuickReadError(String(e));
    } finally {
      if (rafId != null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      if (quickReadUnlistenRef.current) {
        quickReadUnlistenRef.current();
        quickReadUnlistenRef.current = null;
      }
      if (activeQuickReadIdRef.current === requestId) {
        setQuickReading(false);
        activeQuickReadIdRef.current = null;
      }
    }
  };

  const canEdit = skill?.installed && onReadContent && onSaveContent;

  // skills.sh URL
  const skillsShUrl = skill?.source ? `https://skills.sh/${skill.source}/${skill.name}` : null;
  const rawDescription = skill?.description?.trim() || "";
  // Use enriched summary from detail fetch when available
  const enrichedDescription = skillDetails?.summary?.trim() || rawDescription;
  const hasDescription = enrichedDescription.length > 0;
  const hasTranslationForCurrentDescription =
    translatedDescription != null &&
    (descriptionTranslationSource === enrichedDescription || descriptionTranslationSource === rawDescription);
  const descriptionTranslationMatches =
    descriptionTranslationSource != null &&
    (descriptionTranslationSource === enrichedDescription || descriptionTranslationSource === rawDescription);
  const descriptionTranslationActive =
    descriptionTranslationVisible && translatedDescription != null && descriptionTranslationMatches;
  const displayDescriptionProvider = descriptionTranslationProvider ?? (descriptionTranslationActive ? "ai" : null);
  const localizedDescriptionError = formatAiErrorMessage(descriptionTranslateError, t);
  const localizedQuickReadError = formatAiErrorMessage(quickReadError, t);
  const shouldDeferDescriptionRender =
    autoTranslateDescription &&
    shortDescriptionTranslationEnabled &&
    hasDescription &&
    !skill?.localized_description &&
    !hasTranslationForCurrentDescription &&
    !descriptionTranslateError &&
    !descriptionDeferTimedOut;
  const displayDescription = descriptionTranslationActive ? translatedDescription : enrichedDescription;

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

      {skill && !editing && !reading && (
        <motion.aside
          key="skill-detail"
          initial={{ x: "100%", opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
          className="absolute right-0 top-0 bottom-0 w-full max-w-md h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl will-change-transform flex flex-col"
        >
          {/* Header — pinned */}
          <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
            <h2 className="text-heading-sm truncate">{skill.name}</h2>
            <button
              onClick={onClose}
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

              {/* Security Audits */}
              {skillDetails && skillDetails.security_audits.length > 0 && (
                <div className="flex items-center gap-2 flex-wrap">
                  <ShieldCheck className="w-3.5 h-3.5 text-green-500/70" />
                  {skillDetails.security_audits.map((audit) => (
                    <Badge
                      key={audit.name}
                      variant="outline"
                      className={`text-micro font-mono ${
                        audit.result === "Pass"
                          ? "border-green-500/30 text-green-400"
                          : audit.result === "Fail"
                            ? "border-red-500/30 text-red-400"
                            : "border-yellow-500/30 text-yellow-400"
                      }`}
                    >
                      {audit.name}: {audit.result}
                    </Badge>
                  ))}
                </div>
              )}

              {/* Detail loading skeleton */}
              {detailsLoading && (
                <div className="space-y-2">
                  <Skeleton className="h-3 w-full" />
                  <Skeleton className="h-3 w-5/6" />
                  <Skeleton className="h-3 w-4/6" />
                  <Skeleton className="h-20 w-full mt-3" />
                </div>
              )}

              {/* Description */}
              <div className="space-y-2">
                <div className="rounded-xl border border-border/80 bg-muted/25 px-4 py-3">
                  {shouldDeferDescriptionRender ? (
                    <p className="text-body leading-relaxed text-muted-foreground">
                      {t("detailPanel.translatingDescription")}
                    </p>
                  ) : hasDescription ? (
                    <Markdown
                      streaming={translatingDescription && descriptionTranslationActive}
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
                  {hasDescription && (
                    <button
                      onClick={
                        shortDescriptionTranslationEnabled
                          ? () => void handleTranslateDescription("manual")
                          : navigateToAiSettings
                      }
                      className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition duration-300 cursor-pointer shadow-sm relative overflow-hidden group focus-ring ${
                        shortDescriptionTranslationEnabled && translatingDescription
                          ? "bg-destructive/10 text-destructive border border-destructive/20"
                          : shortDescriptionTranslationEnabled && descriptionTranslationActive
                            ? "bg-primary/10 text-primary border border-primary/20"
                            : "bg-gradient-to-br from-background to-muted/50 border border-border hover:border-primary/40 text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      <div className="absolute inset-0 bg-gradient-to-r from-primary/0 via-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
                      <span
                        role="checkbox"
                        aria-checked={autoTranslateDescription}
                        aria-label={t("detailPanel.autoTranslateDescription")}
                        title={t("detailPanel.autoTranslateDescription")}
                        onClick={(event) => {
                          event.stopPropagation();
                          toggleAutoTranslateDescription(!autoTranslateDescription);
                        }}
                        className={`relative z-10 h-4 w-4 rounded-[4px] border flex items-center justify-center transition-colors ${
                          autoTranslateDescription
                            ? "border-primary/60 bg-primary/15 text-primary"
                            : "border-border/90 bg-background text-transparent group-hover:border-primary/35"
                        }`}
                      >
                        <Check className="h-3 w-3" />
                      </span>
                      {shortDescriptionTranslationEnabled && translatingDescription ? (
                        <Square className="w-3.5 h-3.5 fill-current animate-pulse relative z-10" />
                      ) : (
                        <Languages
                          className={`w-3.5 h-3.5 relative z-10 ${shortDescriptionTranslationEnabled && !descriptionTranslationActive ? "text-primary/70" : ""}`}
                        />
                      )}
                      <span className="relative z-10">
                        {!shortDescriptionTranslationEnabled
                          ? t("detailPanel.goToAiConfig")
                          : translatingDescription
                            ? t("common.cancel")
                            : descriptionTranslationActive
                              ? t("detailPanel.showOriginalDescription")
                              : t("detailPanel.translateDescription")}
                      </span>
                    </button>
                  )}

                  {skill.installed && onReadContent && aiConfigured && (
                    <button
                      onClick={handleQuickRead}
                      className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition duration-300 cursor-pointer shadow-sm relative overflow-hidden group focus-ring ${
                        quickReading
                          ? "bg-destructive/10 text-destructive border border-destructive/20"
                          : quickReadVisible
                            ? "bg-primary/10 text-primary border border-primary/20"
                            : "bg-gradient-to-br from-background to-muted/50 border border-border hover:border-primary/40 text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      <div className="absolute inset-0 bg-gradient-to-r from-primary/0 via-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
                      {quickReading ? (
                        <Square className="w-3.5 h-3.5 fill-current animate-pulse relative z-10" />
                      ) : (
                        <Sparkles
                          className={`w-3.5 h-3.5 relative z-10 ${!quickReadVisible ? "text-primary/70" : ""}`}
                        />
                      )}
                      <span className="relative z-10">
                        {quickReading
                          ? t("common.cancel")
                          : quickReadVisible
                            ? t("detailPanel.hideQuickRead")
                            : t("detailPanel.aiQuickRead")}
                      </span>
                    </button>
                  )}
                </div>
                {localizedDescriptionError && (
                  <div className="text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2">
                    {localizedDescriptionError}
                  </div>
                )}
                {translatingDescription && descriptionTranslationActive && descriptionHasDelta && (
                  <div className="text-xs text-primary bg-primary/10 rounded-md px-3 py-2 border border-primary/20">
                    {t("detailPanel.streamingDescriptionPreview")}
                  </div>
                )}
                {descriptionTranslationActive && (displayDescriptionProvider || aiConfigured) && (
                  <div className="text-xs text-muted-foreground bg-muted/40 rounded-md px-3 py-2 border border-border flex items-center justify-between gap-3">
                    <span>
                      {displayDescriptionProvider
                        ? t("detailPanel.translationSourceNotice", {
                            source:
                              displayDescriptionProvider === "mymemory"
                                ? t("detailPanel.translationSourceMyMemory")
                                : t("detailPanel.translationSourceAi"),
                          })
                        : null}
                    </span>
                    {aiConfigured && (
                      <button
                        onClick={() => void handleAiRetranslateDescription()}
                        disabled={translatingDescription}
                        className="text-primary hover:text-primary/80 disabled:opacity-60 disabled:cursor-not-allowed transition-colors cursor-pointer whitespace-nowrap"
                      >
                        {translatingDescription
                          ? t("detailPanel.retranslatingWithAi")
                          : t("detailPanel.retranslateWithAi")}
                      </button>
                    )}
                  </div>
                )}
              </div>

              {/* AI Quick Read Content */}
              {skill.installed &&
                onReadContent &&
                aiConfigured &&
                (quickReadError || quickReading || quickReadVisible) && (
                  <div className="space-y-2">
                    {localizedQuickReadError && (
                      <div className="text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2">
                        {localizedQuickReadError}
                      </div>
                    )}

                    {quickReading && quickReadHasDelta && (
                      <div className="text-xs text-primary bg-primary/10 rounded-md px-3 py-2 border border-primary/20">
                        {t("detailPanel.streamingQuickReadPreview")}
                      </div>
                    )}

                    {!quickReading && quickReadVisible && quickReadContent && quickReadWasNonStreaming && (
                      <div className="text-xs text-muted-foreground bg-muted/40 rounded-md px-3 py-2 border border-border">
                        {t("detailPanel.nonStreamingQuickReadNotice")}
                      </div>
                    )}

                    {quickReadVisible && quickReadContent && (
                      <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
                        <Markdown streaming={quickReading} className="text-xs [&_p]:my-1 [&_strong]:text-primary/90">
                          {quickReadContent}
                        </Markdown>
                      </div>
                    )}
                  </div>
                )}

              {skill.installed && onReadContent && !aiConfigured && (
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
          <div className="shrink-0 border-t border-border bg-card/80 backdrop-blur-sm p-4 space-y-2">
            {skill.installed ? (
              <>
                {skill.update_available && skill.skill_type !== "local" && (
                  <Button className="w-full" onClick={() => onUpdate(skill.name)}>
                    <RefreshCw className="w-4 h-4 mr-2" />
                    {t("detailPanel.updateAvailable")}
                  </Button>
                )}

                <div className="flex gap-2">
                  {onReinstall && skill.skill_type !== "local" && (
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
