import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { X, Download, GitBranch, RefreshCw, Trash2, Edit3, ExternalLink, Copy, Check, Sparkles, Github, Package, Languages, Square } from "lucide-react";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Markdown } from "../ui/Markdown";
import { formatInstalls, unwrapOuterMarkdownFence, navigateToAiSettings } from "../../lib/utils";
import type { Skill, SkillContent } from "../../types";

const SkillEditor = lazy(() =>
  import("../skills/SkillEditor").then((mod) => ({ default: mod.SkillEditor }))
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
  onExportBundle?: (skillName: string) => void;
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
  onExportBundle,
}: DetailPanelProps) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
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
  const [descriptionTranslationVisible, setDescriptionTranslationVisible] = useState(false);
  const [translatingDescription, setTranslatingDescription] = useState(false);
  const [descriptionHasDelta, setDescriptionHasDelta] = useState(false);
  const [descriptionWasNonStreaming, setDescriptionWasNonStreaming] = useState(false);
  const [descriptionTranslateError, setDescriptionTranslateError] = useState<string | null>(null);

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

  useEffect(() => {
    setQuickReadContent(null);
    setQuickReadVisible(false);
    setQuickReadHasDelta(false);
    setQuickReadWasNonStreaming(false);
    setQuickReadError(null);
    setTranslatedDescription(null);
    setDescriptionTranslationVisible(false);
    setDescriptionHasDelta(false);
    setDescriptionWasNonStreaming(false);
    setDescriptionTranslateError(null);
  }, [skill?.name, skill?.description]);

  const handleTranslateDescription = async () => {
    const rawDescription = skill?.description?.trim() || "";
    if (!rawDescription || !aiConfigured) return;

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

    if (descriptionTranslationVisible) {
      setDescriptionTranslationVisible(false);
      return;
    }

    if (translatedDescription) {
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
    setDescriptionTranslationVisible(true);
    setTranslatedDescription("");
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
          return;
        }

        if (payload.event === "error" && payload.message) {
          setDescriptionTranslateError(payload.message);
        }
      });
      translateUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_translate_short_text_stream", {
        requestId,
        content: rawDescription,
      });

      if (activeTranslateIdRef.current !== requestId) return;
      setTranslatedDescription(unwrapOuterMarkdownFence(result).trim());
      setDescriptionTranslationVisible(true);
      setDescriptionWasNonStreaming(deltaCount < 2);
    } catch (e) {
      if (activeTranslateIdRef.current !== requestId) return;
      setDescriptionHasDelta(deltaCount >= 2);
      setDescriptionWasNonStreaming(false);
      if (!streamedRaw.trim()) {
        setTranslatedDescription(null);
        setDescriptionTranslationVisible(false);
      } else {
        setTranslatedDescription(unwrapOuterMarkdownFence(streamedRaw).trim());
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
  const hasDescription = rawDescription.length > 0;
  const displayDescription =
    descriptionTranslationVisible && translatedDescription != null ? translatedDescription : rawDescription;

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
          <div className="absolute right-0 top-0 bottom-0 w-[600px] h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
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

  return (
    <AnimatePresence>
      {skill && (
        <motion.aside
          initial={{ x: "100%", opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0 }}
          transition={{ type: "spring", bounce: 0, duration: 0.3 }}
           className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl will-change-transform"
        >
          {/* Header */}
           <div className="flex items-center justify-between p-4 border-b border-border">
            <h2 className="text-heading-sm truncate">{skill.name}</h2>
            <button
              onClick={onClose}
               className="p-1 rounded-md hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Content */}
          <div className="p-5 space-y-5">
            {/* Meta */}
            <div className="flex items-center gap-3 flex-wrap">
              {!skill.git_url?.trim() && (
                <Badge variant="outline" className="font-medium">
                  Local
                </Badge>
              )}
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
                  {formatInstalls(skill.stars)} installs
                </div>
              )}
              {skill.source && (
                <span className="text-caption">by {skill.source}</span>
              )}
              {!skill.source && skill.author && (
                <span className="text-caption">by {skill.author}</span>
              )}
            </div>

            {/* Description */}
            <div className="space-y-2">
              {hasDescription ? (
                <Markdown className="text-body leading-relaxed [&_p]:my-0 [&_p]:whitespace-pre-wrap">
                  {displayDescription}
                </Markdown>
              ) : (
                <p className="text-body leading-relaxed">
                  {t("detailPanel.noDescription")}
                </p>
              )}
              {hasDescription && (
                <button
                  onClick={aiConfigured ? handleTranslateDescription : navigateToAiSettings}
                  className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors cursor-pointer ${
                    aiConfigured && translatingDescription
                      ? "bg-destructive/10 text-destructive border border-destructive/20 backdrop-blur-sm hover:bg-destructive/15"
                      : aiConfigured && descriptionTranslationVisible
                      ? "bg-primary/15 text-primary border border-primary/20 backdrop-blur-sm"
                      : "bg-muted hover:bg-muted text-muted-foreground hover:text-foreground border border-border backdrop-blur-sm"
                  }`}
                >
                  {aiConfigured && translatingDescription ? (
                    <Square className="w-3.5 h-3.5 fill-current" />
                  ) : (
                    <Languages className="w-3.5 h-3.5" />
                  )}
                  {!aiConfigured
                    ? t("detailPanel.goToAiConfig")
                    : translatingDescription
                    ? t("common.cancel")
                    : descriptionTranslationVisible
                    ? t("detailPanel.showOriginalDescription")
                    : t("detailPanel.translateDescription")}
                </button>
              )}
              {descriptionTranslateError && (
                <div className="text-xs text-destructive bg-destructive/10 backdrop-blur-sm rounded-md px-3 py-2">
                  {descriptionTranslateError}
                </div>
              )}
              {translatingDescription && descriptionTranslationVisible && descriptionHasDelta && (
                <div className="text-xs text-primary bg-primary/10 backdrop-blur-sm rounded-md px-3 py-2 border border-primary/20">
                  {t("detailPanel.streamingDescriptionPreview")}
                </div>
              )}
              {!translatingDescription && descriptionTranslationVisible && translatedDescription && descriptionWasNonStreaming && (
                <div className="text-xs text-muted-foreground bg-muted/40 backdrop-blur-sm rounded-md px-3 py-2 border border-border">
                  {t("detailPanel.nonStreamingDescriptionNotice")}
                </div>
              )}
            </div>

            {/* AI Quick Read */}
            {skill.installed && onReadContent && aiConfigured && (
              <div className="space-y-2">
                <button
                  onClick={handleQuickRead}
                   className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors cursor-pointer w-full justify-center ${
                     quickReading
                       ? "bg-destructive/10 text-destructive border border-destructive/20 backdrop-blur-sm hover:bg-destructive/15"
                       : quickReadVisible
                       ? "bg-primary/15 text-primary border border-primary/20 backdrop-blur-sm"
                       : "bg-muted hover:bg-muted text-muted-foreground hover:text-foreground border border-border backdrop-blur-sm"
                   }`}
                >
                  {quickReading ? (
                    <Square className="w-3.5 h-3.5 fill-current" />
                  ) : (
                    <Sparkles className="w-3.5 h-3.5" />
                  )}
                  {quickReading ? t("common.cancel") : quickReadVisible ? t("detailPanel.hideQuickRead") : t("detailPanel.aiQuickRead")}
                </button>

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

            {/* Git info */}
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

            {/* Export & Publish Buttons — for local skills without git_url */}
            {skill.installed && !skill.git_url && onExportBundle && (
              <Button
                variant="outline"
                className="w-full"
                onClick={() => onExportBundle(skill.name)}
              >
                <Package className="w-4 h-4 mr-2" />
                {t("detailPanel.exportBundle")}
              </Button>
            )}
            {skill.installed && !skill.git_url && onPublish && (
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
                  {skill.update_available && (
                    <Button
                      className="w-full"
                      onClick={() => onUpdate(skill.name)}
                    >
                      <RefreshCw className="w-4 h-4 mr-2" />
                      {t("detailPanel.updateAvailable")}
                    </Button>
                  )}

                  <div className="flex gap-2">
                    {onReinstall && (
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
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
