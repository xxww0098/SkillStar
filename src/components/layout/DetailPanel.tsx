import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  X,
  Download,
  GitBranch,
  RefreshCw,
  Trash2,
  Edit3,
  ExternalLink,
  Copy,
  Sparkles,
  Github,
  Languages,
  Square,
  Star,
  Calendar,
  ShieldCheck,
  BookOpen,
  Check,
} from "lucide-react";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Markdown } from "../ui/Markdown";
import { Skeleton } from "../ui/Skeleton";
import { LoadingLogo } from "../ui/LoadingLogo";
import { SuccessCheckmark } from "../ui/SuccessCheckmark";
import {
  formatAiErrorMessage,
  formatInstalls,
  unwrapOuterMarkdownFence,
  navigateToAiSettings,
} from "../../lib/utils";
import type {
  AiConfig,
  AiStreamPayload,
  LocalFirstResult,
  MarketplaceSkillDetails,
  ShortTextTranslationResult,
  Skill,
  SkillContent,
} from "../../types";

const AUTO_TRANSLATE_DESCRIPTION_KEY = "skillstar:auto-translate-description";
const AUTO_TRANSLATE_DESCRIPTION_TIMEOUT_MS = 6000;

const SkillEditor = lazy(() =>
  import("../skills/SkillEditor").then((mod) => ({ default: mod.SkillEditor })),
);

const SkillReader = lazy(() =>
  import("../skills/SkillReader").then((mod) => ({ default: mod.SkillReader })),
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
  const [quickReadWasNonStreaming, setQuickReadWasNonStreaming] =
    useState(false);
  const [quickReadError, setQuickReadError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState(false);
  const [targetLanguage, setTargetLanguage] = useState("zh-CN");
  const [
    shortDescriptionTranslationEnabled,
    setShortDescriptionTranslationEnabled,
  ] = useState(false);
  const [translatedDescription, setTranslatedDescription] = useState<
    string | null
  >(null);
  const [descriptionTranslationSource, setDescriptionTranslationSource] =
    useState<string | null>(null);
  const [descriptionTranslationVisible, setDescriptionTranslationVisible] =
    useState(false);
  const [translatingDescription, setTranslatingDescription] = useState(false);
  const [descriptionHasDelta, setDescriptionHasDelta] = useState(false);
  const [descriptionTranslationProvider, setDescriptionTranslationProvider] =
    useState<"ai" | "mymemory" | null>(null);
  const [descriptionTranslateError, setDescriptionTranslateError] = useState<
    string | null
  >(null);
  const [descriptionDeferTimedOut, setDescriptionDeferTimedOut] =
    useState(false);
  const [autoTranslateDescription, setAutoTranslateDescription] =
    useState<boolean>(() => {
      try {
        return localStorage.getItem(AUTO_TRANSLATE_DESCRIPTION_KEY) === "true";
      } catch {
        return false;
      }
    });

  // Marketplace detail fetching
  const [skillDetails, setSkillDetails] =
    useState<MarketplaceSkillDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const quickReadCacheRef = useRef<Map<string, string>>(new Map());

  // Cancel refs
  const activeTranslateIdRef = useRef<string | null>(null);
  const activeQuickReadIdRef = useRef<string | null>(null);
  const translateUnlistenRef = useRef<(() => void) | null>(null);
  const quickReadUnlistenRef = useRef<(() => void) | null>(null);
  const autoTranslateAttemptedKeyRef = useRef<string | null>(null);
  // Track in-flight status via ref to avoid stale closures in handleTranslateDescription
  const translatingDescriptionRef = useRef(false);
  // Guard async setState after component unmount
  const mountedRef = useRef(true);

  useEffect(() => {
    const loadAiConfig = async () => {
      try {
        const config = await invoke<AiConfig>("get_ai_config");
        if (!mountedRef.current) return;
        const hasAi = config.enabled && config.api_key.trim().length > 0;
        setAiConfigured(hasAi);
        setTargetLanguage(config.target_language || "zh-CN");
        setShortDescriptionTranslationEnabled(true);
      } catch {
        if (!mountedRef.current) return;
        setAiConfigured(false);
        setTargetLanguage("zh-CN");
        setShortDescriptionTranslationEnabled(true);
      }
    };
    loadAiConfig();
  }, []);

  // Cleanup event listeners on unmount
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      // Detach any in-flight AI jobs from this panel instance.
      // Backend invocation keeps running and can still update cache,
      // while we avoid setState attempts after unmount.
      activeTranslateIdRef.current = null;
      activeQuickReadIdRef.current = null;
      translatingDescriptionRef.current = false;
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
    setDetailsLoading(true);
    try {
      const readLocal = () =>
        invoke<LocalFirstResult<MarketplaceSkillDetails>>(
          "get_skill_detail_local",
          {
            source,
            name,
          },
        );
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
            console.warn(
              "[DetailPanel] Failed to refresh local skill detail:",
              e,
            );
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

  useEffect(() => {
    // Restore cached quick-read, and reset description translation state.
    const cacheKey = `${targetLanguage}::${skill?.name ?? ""}`;
    const cachedQuickRead = quickReadCacheRef.current.get(cacheKey) ?? null;
    setQuickReadContent(cachedQuickRead);
    setQuickReadVisible(false);
    setQuickReadHasDelta(false);
    setQuickReadWasNonStreaming(false);
    setQuickReadError(null);
    setTranslatedDescription(null);
    setDescriptionTranslationSource(null);
    setDescriptionTranslationVisible(false);
    setDescriptionHasDelta(false);
    setDescriptionTranslationProvider(null);
    setDescriptionTranslateError(null);
    setDescriptionDeferTimedOut(false);
    setSkillDetails(null);
    setReading(false);

    // Fetch details for remote marketplace skills
    if (skill && !skill.installed && skill.source) {
      fetchDetails(skill.source, skill.name);
    }
  }, [
    skill?.name,
    skill?.description,
    skill?.installed,
    skill?.source,
    targetLanguage,
    fetchDetails,
  ]);

  const handleTranslateDescription = useCallback(
    async (mode: "manual" | "auto" = "manual") => {
      const renderedDescription =
        skillDetails?.summary?.trim() || skill?.description?.trim() || "";
      if (!renderedDescription || !shortDescriptionTranslationEnabled) return;
      const hasReusableTranslation =
        translatedDescription != null &&
        descriptionTranslationSource === renderedDescription;

      // Use ref to read in-flight status to avoid stale closure
      const isInFlight = translatingDescriptionRef.current;

      if (mode === "manual") {
        // Cancel in-progress
        if (isInFlight) {
          activeTranslateIdRef.current = null;
          if (translateUnlistenRef.current) {
            translateUnlistenRef.current();
            translateUnlistenRef.current = null;
          }
          translatingDescriptionRef.current = false;
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
      } else {
        if (isInFlight) return;
        if (descriptionTranslationVisible && hasReusableTranslation) return;
        if (hasReusableTranslation) {
          setDescriptionTranslationVisible(true);
          return;
        }
      }

      const requestId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `detail-translate-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      activeTranslateIdRef.current = requestId;
      let streamedRaw = "";
      let deltaCount = 0;

      translatingDescriptionRef.current = true;
      setTranslatingDescription(true);
      setDescriptionTranslateError(null);
      setDescriptionDeferTimedOut(false);
      setDescriptionHasDelta(false);
      setDescriptionTranslationProvider(null);
      setTranslatedDescription(null);
      setDescriptionTranslationVisible(false);
      setDescriptionTranslationSource(renderedDescription);

      let rafId: number | null = null;
      const flushDelta = () => {
        rafId = null;
        if (activeTranslateIdRef.current !== requestId) return;
        setTranslatedDescription(streamedRaw);
        setDescriptionTranslationProvider("ai");
        setDescriptionTranslationVisible(true);
        if (deltaCount >= 2) setDescriptionHasDelta(true);
      };

      try {
        const unlisten = await listen<AiStreamPayload>(
          "ai://translate-stream",
          (event) => {
            if (activeTranslateIdRef.current !== requestId) return;
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
              setDescriptionTranslateError(payload.message);
            }
          },
        );
        translateUnlistenRef.current = unlisten;

        const result = await invoke<ShortTextTranslationResult>(
          "ai_translate_short_text_stream_with_source",
          {
            requestId,
            content: renderedDescription,
          },
        );

        if (activeTranslateIdRef.current !== requestId) return;
        const finalTranslation = unwrapOuterMarkdownFence(result.text).trim();
        setTranslatedDescription(finalTranslation);
        setDescriptionTranslationSource(renderedDescription);
        setDescriptionTranslationProvider(result.source);
        setDescriptionTranslationVisible(true);
        setDescriptionDeferTimedOut(false);
        setDescriptionHasDelta(deltaCount >= 2);
      } catch (e) {
        if (activeTranslateIdRef.current !== requestId) return;
        setDescriptionHasDelta(deltaCount >= 2);
        setDescriptionTranslationProvider(null);
        if (!streamedRaw.trim()) {
          setTranslatedDescription(null);
          setDescriptionTranslationSource(null);
          setDescriptionTranslationVisible(false);
        } else {
          setTranslatedDescription(streamedRaw);
          setDescriptionTranslationSource(renderedDescription);
          setDescriptionTranslationVisible(true);
        }
        setDescriptionTranslateError(String(e));
      } finally {
        if (rafId != null) {
          cancelAnimationFrame(rafId);
          rafId = null;
        }
        if (translateUnlistenRef.current) {
          translateUnlistenRef.current();
          translateUnlistenRef.current = null;
        }
        if (activeTranslateIdRef.current === requestId) {
          translatingDescriptionRef.current = false;
          setTranslatingDescription(false);
          activeTranslateIdRef.current = null;
        }
      }
    },
    [
      shortDescriptionTranslationEnabled,
      descriptionTranslationSource,
      descriptionTranslationVisible,
      skill,
      skillDetails?.summary,
      targetLanguage,
      translatedDescription,
    ],
  );

  const handleAiRetranslateDescription = useCallback(async () => {
    if (!aiConfigured || translatingDescriptionRef.current) return;
    const renderedDescription =
      skillDetails?.summary?.trim() || skill?.description?.trim() || "";
    if (!renderedDescription) return;

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `detail-retranslate-ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeTranslateIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    translatingDescriptionRef.current = true;
    setTranslatingDescription(true);
    setDescriptionTranslateError(null);
    setDescriptionDeferTimedOut(false);
    setDescriptionHasDelta(false);
    setDescriptionTranslationProvider("ai");

    let rafId: number | null = null;
    const flushDelta = () => {
      rafId = null;
      if (activeTranslateIdRef.current !== requestId) return;
      setTranslatedDescription(streamedRaw);
      setDescriptionTranslationProvider("ai");
      setDescriptionTranslationVisible(true);
      if (deltaCount >= 2) setDescriptionHasDelta(true);
    };

    try {
      const unlisten = await listen<AiStreamPayload>(
        "ai://translate-stream",
        (event) => {
          if (activeTranslateIdRef.current !== requestId) return;
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
            setDescriptionTranslateError(payload.message);
          }
        },
      );
      translateUnlistenRef.current = unlisten;

      const result = await invoke<ShortTextTranslationResult>(
        "ai_retranslate_short_text_stream_with_source",
        {
          requestId,
          content: renderedDescription,
        },
      );

      if (activeTranslateIdRef.current !== requestId) return;
      const finalTranslation = unwrapOuterMarkdownFence(result.text).trim();
      setTranslatedDescription(finalTranslation);
      setDescriptionTranslationSource(renderedDescription);
      setDescriptionTranslationProvider(result.source);
      setDescriptionTranslationVisible(true);
      setDescriptionDeferTimedOut(false);
      setDescriptionHasDelta(deltaCount >= 2);
    } catch (e) {
      if (activeTranslateIdRef.current !== requestId) return;
      setDescriptionHasDelta(deltaCount >= 2);
      if (streamedRaw.trim()) {
        setTranslatedDescription(streamedRaw);
        setDescriptionTranslationSource(renderedDescription);
        setDescriptionTranslationProvider("ai");
        setDescriptionTranslationVisible(true);
      }
      setDescriptionTranslateError(String(e));
    } finally {
      if (rafId != null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      if (activeTranslateIdRef.current === requestId) {
        translatingDescriptionRef.current = false;
        setTranslatingDescription(false);
        activeTranslateIdRef.current = null;
      }
    }
  }, [
    aiConfigured,
    skill,
    skillDetails?.summary,
    targetLanguage,
    translatingDescription,
  ]);

  useEffect(() => {
    if (!autoTranslateDescription || !translatingDescription) return;
    const renderedDescription =
      skillDetails?.summary?.trim() || skill?.description?.trim() || "";
    if (!renderedDescription) return;
    if (descriptionTranslationSource !== renderedDescription) return;
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

  useEffect(() => {
    if (
      !skill ||
      !autoTranslateDescription ||
      !shortDescriptionTranslationEnabled
    )
      return;
    const renderedDescription =
      skillDetails?.summary?.trim() || skill.description?.trim() || "";
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
      const unlisten = await listen<AiStreamPayload>(
        "ai://summarize-stream",
        (event) => {
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
        },
      );
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
  const hasTranslationForCurrentDescription =
    translatedDescription != null &&
    descriptionTranslationSource === enrichedDescription;
  const descriptionTranslationMatches =
    descriptionTranslationSource != null &&
    descriptionTranslationSource === enrichedDescription;
  const descriptionTranslationActive =
    descriptionTranslationVisible &&
    translatedDescription != null &&
    descriptionTranslationMatches;
  const displayDescriptionProvider =
    descriptionTranslationProvider ??
    (descriptionTranslationActive ? "ai" : null);
  const localizedDescriptionError = formatAiErrorMessage(
    descriptionTranslateError,
    t,
  );
  const localizedQuickReadError = formatAiErrorMessage(quickReadError, t);
  const shouldDeferDescriptionRender =
    autoTranslateDescription &&
    shortDescriptionTranslationEnabled &&
    hasDescription &&
    !hasTranslationForCurrentDescription &&
    !descriptionTranslateError &&
    !descriptionDeferTimedOut;
  const displayDescription = descriptionTranslationActive
    ? translatedDescription
    : enrichedDescription;

  const handleCopy = async () => {
    if (!installCmd) return;
    await navigator.clipboard.writeText(installCmd);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

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
            <SkillReader
              skillName={skill.name}
              content={skillDetails.readme}
              onClose={() => setReading(false)}
            />
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
          className="absolute right-0 top-0 bottom-0 w-full max-w-sm h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl will-change-transform flex flex-col"
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
          <div className="flex-1 overflow-y-auto">
            <div className="p-5 space-y-5">
              {/* Meta */}
              <div className="flex items-center gap-3 flex-wrap">
                {skill.rank && (
                  <Badge
                    variant="outline"
                    className="tabular-nums font-semibold"
                  >
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
                {skill.skill_type === "local" && (
                  <span className="text-caption">local</span>
                )}
                {skill.source && (
                  <span className="text-caption">by {skill.source}</span>
                )}
                {!skill.source && skill.author && (
                  <span className="text-caption">by {skill.author}</span>
                )}
                {skillDetails?.github_stars != null &&
                  skillDetails.github_stars > 0 && (
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
                      streaming={
                        translatingDescription && descriptionTranslationActive
                      }
                      className="text-body leading-relaxed [&_p]:my-0 [&_p]:whitespace-pre-wrap [&_p+ul]:mt-3 [&_p+ol]:mt-3 [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1.5 [&_strong]:text-foreground"
                    >
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
                      onClick={
                        shortDescriptionTranslationEnabled
                          ? () => void handleTranslateDescription("manual")
                          : navigateToAiSettings
                      }
                      className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium transition duration-300 cursor-pointer shadow-sm relative overflow-hidden group focus-ring ${
                        shortDescriptionTranslationEnabled &&
                        translatingDescription
                          ? "bg-destructive/10 text-destructive border border-destructive/20"
                          : shortDescriptionTranslationEnabled &&
                              descriptionTranslationActive
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
                          toggleAutoTranslateDescription(
                            !autoTranslateDescription,
                          );
                        }}
                        className={`relative z-10 h-4 w-4 rounded-[4px] border flex items-center justify-center transition-colors ${
                          autoTranslateDescription
                            ? "border-primary/60 bg-primary/15 text-primary"
                            : "border-border/90 bg-background text-transparent group-hover:border-primary/35"
                        }`}
                      >
                        <Check className="h-3 w-3" />
                      </span>
                      {shortDescriptionTranslationEnabled &&
                      translatingDescription ? (
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
                {translatingDescription &&
                  descriptionTranslationActive &&
                  descriptionHasDelta && (
                    <div className="text-xs text-primary bg-primary/10 rounded-md px-3 py-2 border border-primary/20">
                      {t("detailPanel.streamingDescriptionPreview")}
                    </div>
                  )}
                {descriptionTranslationActive &&
                  (displayDescriptionProvider || aiConfigured) && (
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

                    {!quickReading &&
                      quickReadVisible &&
                      quickReadContent &&
                      quickReadWasNonStreaming && (
                        <div className="text-xs text-muted-foreground bg-muted/40 rounded-md px-3 py-2 border border-border">
                          {t("detailPanel.nonStreamingQuickReadNotice")}
                        </div>
                      )}

                    {quickReadVisible && quickReadContent && (
                      <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
                        <Markdown
                          streaming={quickReading}
                          className="text-xs [&_p]:my-1 [&_strong]:text-primary/90"
                        >
                          {quickReadContent}
                        </Markdown>
                      </div>
                    )}
                  </div>
                )}

              {skill.installed && onReadContent && !aiConfigured && (
                <div className="rounded-lg border border-border bg-card px-3 py-2 flex items-center gap-2">
                  <p className="text-xs text-muted-foreground flex-1">
                    {t("detailPanel.aiPromptHint")}
                  </p>
                  <button
                    onClick={navigateToAiSettings}
                    className="px-2 py-1 rounded-md text-micro font-medium border border-border hover:bg-muted transition-colors cursor-pointer focus-ring"
                  >
                    {t("detailPanel.goToAiConfig")}
                  </button>
                </div>
              )}

              {/* Install Command */}
              {installCmd && !skill.installed && (
                <div className="space-y-1.5">
                  <label className="text-caption font-medium uppercase tracking-wider text-xs">
                    {t("detailPanel.installLabel")}
                  </label>
                  <div className="flex items-center gap-2 bg-card rounded-lg px-3 py-2.5 border border-border">
                    <code className="text-xs font-mono text-foreground flex-1 select-all overflow-x-auto whitespace-nowrap">
                      {installCmd}
                    </code>
                    <button
                      onClick={handleCopy}
                      className="p-1 rounded-md hover:bg-muted text-muted-foreground transition-colors shrink-0 cursor-pointer"
                    >
                      {copied ? (
                        <SuccessCheckmark size={14} className="text-success" />
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
                    <span className="truncate font-mono text-xs">
                      {skill.git_url}
                    </span>
                  </div>
                  {skill.tree_hash && (
                    <div className="text-caption font-mono text-xs">
                      {t("detailPanel.tree")} {skill.tree_hash.slice(0, 12)}…
                    </div>
                  )}
                  <div className="text-caption">
                    {t("detailPanel.updated")}{" "}
                    {new Date(skill.last_updated).toLocaleDateString()}
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
                  className="w-full border-primary/30 text-primary hover:bg-primary/15 hover:text-primary"
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
                        {uninstalling
                          ? t("common.uninstalling")
                          : t("common.uninstall")}
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
