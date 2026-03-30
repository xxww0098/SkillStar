import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { X, Download, GitBranch, RefreshCw, Trash2, Edit3, ExternalLink, Copy, Check, Sparkles, Github, Languages, Square, Star, Calendar, ShieldCheck, BookOpen } from "lucide-react";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Markdown } from "../ui/Markdown";
import { Skeleton } from "../ui/Skeleton";
import { formatInstalls, unwrapOuterMarkdownFence, navigateToAiSettings } from "../../lib/utils";
import type { Skill, SkillContent, MarketplaceSkillDetails } from "../../types";

const SkillEditor = lazy(() =>
  import("../skills/SkillEditor").then((mod) => ({ default: mod.SkillEditor }))
);

const SkillReader = lazy(() =>
  import("../skills/SkillReader").then((mod) => ({ default: mod.SkillReader }))
);

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

interface AiConfigLike {
  enabled: boolean;
  api_key: string;
}

interface AiStreamPayload {
  requestId: string;
  event: "start" | "delta" | "complete" | "error";
  delta?: string | null;
  message?: string | null;
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
  const [copied, setCopied] = useState(false);

  // AI Quick Read
  const [quickReadContent, setQuickReadContent] = useState<string | null>(null);
  const [quickReadVisible, setQuickReadVisible] = useState(false);
  const [quickReading, setQuickReading] = useState(false);
  const [quickReadHasDelta, setQuickReadHasDelta] = useState(false);
  const [quickReadWasNonStreaming, setQuickReadWasNonStreaming] = useState(false);
  const [quickReadError, setQuickReadError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState(false);
  const [translatedDescription, setTranslatedDescription] = useState<string | null>(null);
  const [descriptionTranslationSource, setDescriptionTranslationSource] = useState<string | null>(null);
  const [descriptionTranslationVisible, setDescriptionTranslationVisible] = useState(false);
  const [translatingDescription, setTranslatingDescription] = useState(false);
  const [descriptionHasDelta, setDescriptionHasDelta] = useState(false);
  const [descriptionWasNonStreaming, setDescriptionWasNonStreaming] = useState(false);
  const [descriptionTranslateError, setDescriptionTranslateError] = useState<string | null>(null);

  // Marketplace detail fetching
  const [skillDetails, setSkillDetails] = useState<MarketplaceSkillDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const detailsCacheRef = useRef<Map<string, MarketplaceSkillDetails>>(new Map());
  const quickReadCacheRef = useRef<Map<string, string>>(new Map());
  const descriptionTranslateCacheRef = useRef<Map<string, string>>(new Map());

  // Cancel refs
  const activeTranslateIdRef = useRef<string | null>(null);
  const activeQuickReadIdRef = useRef<string | null>(null);
  const translateUnlistenRef = useRef<(() => void) | null>(null);
  const quickReadUnlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const loadAiConfig = async () => {
      try {
        const config = await invoke<AiConfigLike>("get_ai_config");
        setAiConfigured(config.enabled && config.api_key.trim().length > 0);
      } catch {
        setAiConfigured(false);
      }
    };
    loadAiConfig();
  }, []);

  // Cleanup event listeners on unmount
  useEffect(() => {
    return () => {
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      if (quickReadUnlistenRef.current) {
        quickReadUnlistenRef.current();
        quickReadUnlistenRef.current = null;
      }
    };
  }, []);

  // Fetch marketplace details for remote skills
  const fetchDetails = useCallback(async (source: string, name: string) => {
    const cacheKey = `${source}/${name}`;
    const cached = detailsCacheRef.current.get(cacheKey);
    if (cached) {
      setSkillDetails(cached);
      return;
    }
    setDetailsLoading(true);
    try {
      const details = await invoke<MarketplaceSkillDetails>("get_marketplace_skill_details", { source, name });
      detailsCacheRef.current.set(cacheKey, details);
      setSkillDetails(details);
    } catch (e) {
      console.warn("[DetailPanel] Failed to fetch skill details:", e);
      setSkillDetails(null);
    } finally {
      setDetailsLoading(false);
    }
  }, []);

  useEffect(() => {
    // Restore cached quick-read & description translation, or reset
    const skillKey = skill?.name ?? "";
    const cachedQuickRead = quickReadCacheRef.current.get(skillKey) ?? null;
    setQuickReadContent(cachedQuickRead);
    setQuickReadVisible(false);
    setQuickReadHasDelta(false);
    setQuickReadWasNonStreaming(false);
    setQuickReadError(null);
    const cachedDescTranslation = descriptionTranslateCacheRef.current.get(skillKey) ?? null;
    setTranslatedDescription(cachedDescTranslation);
    setDescriptionTranslationSource(null);
    setDescriptionTranslationVisible(false);
    setDescriptionHasDelta(false);
    setDescriptionWasNonStreaming(false);
    setDescriptionTranslateError(null);
    setSkillDetails(null);
    setReading(false);

    // Fetch details for remote marketplace skills
    if (skill && !skill.installed && skill.source) {
      fetchDetails(skill.source, skill.name);
    }
  }, [skill?.name, skill?.description, skill?.installed, skill?.source, fetchDetails]);

  const handleTranslateDescription = async () => {
    const renderedDescription = skillDetails?.summary?.trim() || skill?.description?.trim() || "";
    if (!renderedDescription || !aiConfigured) return;
    const hasReusableTranslation =
      translatedDescription != null && descriptionTranslationSource === renderedDescription;

    // Cancel in-progress
    if (translatingDescription) {
      activeTranslateIdRef.current = null;
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      setTranslatingDescription(false);
      if (!translatedDescription) {
        setDescriptionTranslationVisible(false);
      }
      return;
    }

    if (descriptionTranslationVisible && hasReusableTranslation) {
      setDescriptionTranslationVisible(false);
      return;
    }

    if (hasReusableTranslation) {
      setDescriptionTranslationVisible(true);
      return;
    }

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `detail-translate-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeTranslateIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setTranslatingDescription(true);
    setDescriptionTranslateError(null);
    setDescriptionHasDelta(false);
    setDescriptionWasNonStreaming(false);
    setTranslatedDescription(null);
    setDescriptionTranslationVisible(false);
    setDescriptionTranslationSource(renderedDescription);
    // Don't set descriptionTranslationVisible or translatedDescription yet —
    // keep showing the original description until the first delta or final result arrives.
    try {
      const unlisten = await listen<AiStreamPayload>("ai://translate-stream", (event) => {
        if (activeTranslateIdRef.current !== requestId) return;
        const payload = event.payload;
        if (payload.requestId !== requestId) return;

        if (payload.event === "delta" && payload.delta) {
          deltaCount += 1;
          if (deltaCount >= 2) setDescriptionHasDelta(true);
          streamedRaw += payload.delta;
          setTranslatedDescription(unwrapOuterMarkdownFence(streamedRaw).trim());
          // Show translated content now that we have actual data
          setDescriptionTranslationVisible(true);
          return;
        }

        if (payload.event === "error" && payload.message) {
          setDescriptionTranslateError(payload.message);
        }
      });
      translateUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_translate_short_text_stream", {
        requestId,
        content: renderedDescription,
      });

      if (activeTranslateIdRef.current !== requestId) return;
      const finalTranslation = unwrapOuterMarkdownFence(result).trim();
      setTranslatedDescription(finalTranslation);
      setDescriptionTranslationSource(renderedDescription);
      setDescriptionTranslationVisible(true);
      setDescriptionWasNonStreaming(deltaCount < 2);
      // Cache completed description translation
      if (skill) descriptionTranslateCacheRef.current.set(skill.name, finalTranslation);
    } catch (e) {
      if (activeTranslateIdRef.current !== requestId) return;
      setDescriptionHasDelta(deltaCount >= 2);
      setDescriptionWasNonStreaming(false);
      if (!streamedRaw.trim()) {
        setTranslatedDescription(null);
        setDescriptionTranslationSource(null);
        setDescriptionTranslationVisible(false);
      } else {
        setTranslatedDescription(unwrapOuterMarkdownFence(streamedRaw).trim());
        setDescriptionTranslationSource(renderedDescription);
        setDescriptionTranslationVisible(true);
      }
      setDescriptionTranslateError(String(e));
    } finally {
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      if (activeTranslateIdRef.current === requestId) {
        setTranslatingDescription(false);
        activeTranslateIdRef.current = null;
      }
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
    try {
      const unlisten = await listen<AiStreamPayload>("ai://summarize-stream", (event) => {
        if (activeQuickReadIdRef.current !== requestId) return;
        const payload = event.payload;
        if (payload.requestId !== requestId) return;

        if (payload.event === "delta" && payload.delta) {
          deltaCount += 1;
          if (deltaCount >= 2) setQuickReadHasDelta(true);
          streamedRaw += payload.delta;
          setQuickReadContent(streamedRaw);
          return;
        }

        if (payload.event === "error" && payload.message) {
          setQuickReadError(payload.message);
        }
      });
      quickReadUnlistenRef.current = unlisten;

      const data = await onReadContent(skill.name);
      const result = await invoke<string>("ai_summarize_skill_stream", {
        requestId,
        content: data.content,
      });

      if (activeQuickReadIdRef.current !== requestId) return;
      setQuickReadContent(result);
      setQuickReadVisible(true);
      setQuickReadWasNonStreaming(deltaCount < 2);
      // Cache completed summary
      if (skill) quickReadCacheRef.current.set(skill.name, result);
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

  // Build install command
  const installCmd = skill?.source
    ? `npx skills add ${skill.source} --skill ${skill.name}`
    : skill?.git_url
    ? `npx skills add ${skill.git_url}`
    : null;

  // skills.sh URL
  const skillsShUrl = skill?.source
    ? `https://skills.sh/${skill.source}/${skill.name}`
    : null;
  const rawDescription = skill?.description?.trim() || "";
  // Use enriched summary from detail fetch when available
  const enrichedDescription = skillDetails?.summary?.trim() || rawDescription;
  const hasDescription = enrichedDescription.length > 0;
  const descriptionTranslationMatches =
    descriptionTranslationSource != null && descriptionTranslationSource === enrichedDescription;
  const descriptionTranslationActive =
    descriptionTranslationVisible && translatedDescription != null && descriptionTranslationMatches;
  const displayDescription =
    descriptionTranslationActive ? translatedDescription : enrichedDescription;

  const handleCopy = async () => {
    if (!installCmd) return;
    await navigator.clipboard.writeText(installCmd);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (editing && skill && onReadContent && onSaveContent) {
    return (
      <Suspense
        fallback={
          <div className="absolute right-0 top-0 bottom-0 w-[600px] h-full border-l border-border bg-background shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
             <span className="text-muted-foreground text-sm">{t("detailPanel.reading")}</span>
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
    );
  }

  if (reading && skill && skillDetails?.readme) {
    return (
      <Suspense
        fallback={
          <div className="absolute right-0 top-0 bottom-0 w-[600px] h-full border-l border-border bg-background shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
             <span className="text-muted-foreground text-sm">{t("detailPanel.reading")}</span>
          </div>
        }
      >
        <SkillReader
          skillName={skill.name}
          content={skillDetails.readme}
          onClose={() => setReading(false)}
        />
      </Suspense>
    );
  }

  return (
    <AnimatePresence>
      {skill && (
        <motion.aside
          initial={{ x: "100%", opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0 }}
          transition={{ type: "spring", bounce: 0, duration: 0.3 }}
           className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl will-change-transform flex flex-col"
        >
          {/* Header — pinned */}
           <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
            <h2 className="text-heading-sm truncate">{skill.name}</h2>
            <button
              onClick={onClose}
               className="p-1 rounded-md hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Scrollable content */}
          <div className="flex-1 overflow-y-auto">
          <div className="p-5 space-y-5">
            {/* Meta */}
            <div className="flex items-center gap-3 flex-wrap">

              {skill.rank && (
                <Badge variant="outline" className="tabular-nums font-semibold">
                  #{skill.rank}
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
              {skill.skill_type === "local" && (
                <span className="text-caption">local</span>
              )}
              {skill.source && (
                <span className="text-caption">by {skill.source}</span>
              )}
              {!skill.source && skill.author && (
                <span className="text-caption">by {skill.author}</span>
              )}
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
                    className={`text-[10px] font-mono ${
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
              <div className="rounded-xl border border-border/80 bg-muted/25 backdrop-blur-sm px-4 py-3">
                {hasDescription ? (
                  <Markdown className="text-body leading-relaxed [&_p]:my-0 [&_p]:whitespace-pre-wrap [&_p+ul]:mt-3 [&_p+ol]:mt-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1.5 [&_strong]:text-foreground">
                    {displayDescription}
                  </Markdown>
                ) : (
                  <p className="text-body leading-relaxed">
                    {t("detailPanel.noDescription")}
                  </p>
                )}
              </div>
              {/* AI Actions Row */}
              <div className="flex items-center gap-2">
                {hasDescription && (
                  <button
                    onClick={aiConfigured ? handleTranslateDescription : navigateToAiSettings}
                    className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition-all duration-300 cursor-pointer shadow-sm relative overflow-hidden group ${
                      aiConfigured && translatingDescription
                        ? "bg-destructive/10 text-destructive border border-destructive/20"
                        : aiConfigured && descriptionTranslationActive
                        ? "bg-indigo-500/10 text-indigo-700 dark:text-indigo-300 border border-indigo-500/20"
                        : "bg-gradient-to-br from-background to-muted/50 border border-border hover:border-indigo-500/40 text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <div className="absolute inset-0 bg-gradient-to-r from-indigo-500/0 via-indigo-500/5 to-purple-500/0 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
                    {aiConfigured && translatingDescription ? (
                      <Square className="w-3.5 h-3.5 fill-current animate-pulse relative z-10" />
                    ) : (
                      <Languages className={`w-3.5 h-3.5 relative z-10 ${aiConfigured && !descriptionTranslationActive ? "text-indigo-500/70" : ""}`} />
                    )}
                    <span className="relative z-10">
                      {!aiConfigured
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
                    className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition-all duration-300 cursor-pointer shadow-sm relative overflow-hidden group ${
                      quickReading
                         ? "bg-destructive/10 text-destructive border border-destructive/20"
                         : quickReadVisible
                         ? "bg-purple-500/10 text-purple-700 dark:text-purple-300 border border-purple-500/20"
                         : "bg-gradient-to-br from-background to-muted/50 border border-border hover:border-purple-500/40 text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <div className="absolute inset-0 bg-gradient-to-r from-purple-500/0 via-purple-500/5 to-pink-500/0 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
                    {quickReading ? (
                      <Square className="w-3.5 h-3.5 fill-current animate-pulse relative z-10" />
                    ) : (
                      <Sparkles className={`w-3.5 h-3.5 relative z-10 ${!quickReadVisible ? "text-purple-500/70" : ""}`} />
                    )}
                    <span className="relative z-10">
                      {quickReading ? t("common.cancel") : quickReadVisible ? t("detailPanel.hideQuickRead") : t("detailPanel.aiQuickRead")}
                    </span>
                  </button>
                )}
              </div>
              {descriptionTranslateError && (
                <div className="text-xs text-destructive bg-destructive/10 backdrop-blur-sm rounded-md px-3 py-2">
                  {descriptionTranslateError}
                </div>
              )}
              {translatingDescription && descriptionTranslationActive && descriptionHasDelta && (
                <div className="text-xs text-primary bg-primary/10 backdrop-blur-sm rounded-md px-3 py-2 border border-primary/20">
                  {t("detailPanel.streamingDescriptionPreview")}
                </div>
              )}
              {!translatingDescription && descriptionTranslationActive && descriptionWasNonStreaming && (
                <div className="text-xs text-muted-foreground bg-muted/40 backdrop-blur-sm rounded-md px-3 py-2 border border-border">
                  {t("detailPanel.nonStreamingDescriptionNotice")}
                </div>
              )}
            </div>

            {/* AI Quick Read Content */}
            {skill.installed && onReadContent && aiConfigured && (quickReadError || quickReading || quickReadVisible) && (
              <div className="space-y-2">

                {quickReadError && (
                   <div className="text-xs text-destructive bg-destructive/10 backdrop-blur-sm rounded-md px-3 py-2">
                    {quickReadError}
                  </div>
                )}

                {quickReading && quickReadHasDelta && (
                  <div className="text-xs text-primary bg-primary/10 backdrop-blur-sm rounded-md px-3 py-2 border border-primary/20">
                    {t("detailPanel.streamingQuickReadPreview")}
                  </div>
                )}

                {!quickReading && quickReadVisible && quickReadContent && quickReadWasNonStreaming && (
                  <div className="text-xs text-muted-foreground bg-muted/40 backdrop-blur-sm rounded-md px-3 py-2 border border-border">
                    {t("detailPanel.nonStreamingQuickReadNotice")}
                  </div>
                )}

                {quickReadVisible && quickReadContent && (
                   <div className="rounded-lg border border-primary/20 bg-primary/5 backdrop-blur-sm p-3">
                    <Markdown className="text-xs [&_p]:my-1 [&_strong]:text-primary/90">
                      {quickReadContent}
                    </Markdown>
                  </div>
                )}
              </div>
            )}

            {skill.installed && onReadContent && !aiConfigured && (
               <div className="rounded-lg border border-border bg-card backdrop-blur-sm px-3 py-2 flex items-center gap-2">
                 <p className="text-xs text-muted-foreground flex-1">
                  {t("detailPanel.aiPromptHint")}
                </p>
                <button
                  onClick={navigateToAiSettings}
                   className="px-2 py-1 rounded-md text-[11px] font-medium border border-border hover:bg-muted transition-colors cursor-pointer"
                >
                  {t("detailPanel.goToAiConfig")}
                </button>
              </div>
            )}

            {/* Install Command */}
            {installCmd && (
              <div className="space-y-1.5">
                <label className="text-caption font-medium uppercase tracking-wider text-xs">
                  {t("detailPanel.installLabel")}
                </label>
                 <div className="flex items-center gap-2 bg-card backdrop-blur-sm rounded-lg px-3 py-2.5 border border-border">
                   <code className="text-xs font-mono text-foreground flex-1 select-all overflow-x-auto whitespace-nowrap">
                    {installCmd}
                  </code>
                  <button
                    onClick={handleCopy}
                     className="p-1 rounded-md hover:bg-muted text-muted-foreground transition-colors shrink-0 cursor-pointer"
                  >
                    {copied ? (
                      <Check className="w-3.5 h-3.5 text-success" />
                    ) : (
                      <Copy className="w-3.5 h-3.5" />
                    )}
                  </button>
                </div>
              </div>
            )}

            {/* skills.sh link */}
            {skillsShUrl && (
              <a
                href={skillsShUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-2 text-xs text-primary/70 hover:text-primary transition-colors"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                {t("detailPanel.viewOnSkillsSh")}
              </a>
            )}

            {/* Git info — only for hub (git-backed) skills */}
            {skill.skill_type !== "local" && skill.git_url && (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-caption">
                <GitBranch className="w-3.5 h-3.5" />
                <span className="truncate font-mono text-xs">{skill.git_url}</span>
              </div>
              {skill.tree_hash && (
                <div className="text-caption font-mono text-xs">
                   {t("detailPanel.tree")} {skill.tree_hash.slice(0, 12)}…
                </div>
              )}
              <div className="text-caption">
                {t("detailPanel.updated")} {new Date(skill.last_updated).toLocaleDateString()}
              </div>
            </div>
            )}

            {/* Topics */}
            {skill.topics.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {skill.topics.map((t) => (
                  <Badge key={t} variant="outline">
                    {t}
                  </Badge>
                ))}
              </div>
            )}

            {/* SKILL.md — open in reader */}
            {skillDetails?.readme && (
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setReading(true)}
              >
                <BookOpen className="w-4 h-4 mr-2" />
                {t("detailPanel.readSkillMd")}
              </Button>
            )}

            {/* Edit Button (only for installed skills) */}
            {canEdit && (
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setEditing(true)}
              >
                <Edit3 className="w-4 h-4 mr-2" />
                {t("detailPanel.editSkillMd")}
              </Button>
            )}

            {/* Publish Button — for local skills */}
            {skill.installed && skill.skill_type === "local" && onPublish && (
              <Button
                variant="outline"
                 className="w-full border-primary/30 text-primary hover:bg-primary/15 hover:text-primary backdrop-blur-sm"
                onClick={() => onPublish(skill.name)}
              >
                <Github className="w-4 h-4 mr-2" />
                {t("detailPanel.publishToGithub")}
              </Button>
            )}

            {/* Actions */}
            <div className="space-y-2 pt-2">
              {skill.installed ? (
                <>
                  {skill.update_available && skill.skill_type !== "local" && (
                    <Button
                      className="w-full"
                      onClick={() => onUpdate(skill.name)}
                    >
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
                <Button
                  className="w-full"
                  onClick={() => onInstall(skill.git_url, skill.name)}
                >
                  <Download className="w-4 h-4" />
                  {t("common.install")}
                </Button>
              )}
            </div>
          </div>
          </div>
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
