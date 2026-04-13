import { Eye, FileText, Globe, Loader2, RotateCcw, Sparkles, Square, X } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAiStream } from "../../hooks/useAiStream";
import {
  normalizeSkillMarkdownForPreview,
  parseFrontmatterEntries,
  splitFrontmatter,
  unwrapOuterMarkdownFence,
} from "../../lib/frontmatter";
import { formatAiErrorMessage, navigateToAiSettings } from "../../lib/utils";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";
import { AiErrorBanner, AiNotConfiguredBanner } from "./AiBanners";

interface SkillReaderProps {
  skillName: string;
  content: string;
  onClose: () => void;
}

function buildCacheKey(targetLanguage: string, sourceContent: string): string {
  return `${targetLanguage}::${sourceContent}`;
}

/** Module-level summary cache keyed by target language + content. */
const summaryCache = new Map<string, string>();

const MAX_CACHE_SIZE = 100;

function trimCache<K, V>(cache: Map<K, V>) {
  if (cache.size <= MAX_CACHE_SIZE) return;
  let count = 0;
  const max = Math.floor(MAX_CACHE_SIZE / 2);
  for (const key of cache.keys()) {
    if (count++ >= max) break;
    cache.delete(key);
  }
}

export function SkillReader({ skillName, content, onClose }: SkillReaderProps) {
  const { t } = useTranslation();
  const [retranslating, setRetranslating] = useState(false);
  const prefersReducedMotion = useReducedMotion();

  const translationStream = useAiStream({
    command: "ai_translate_skill_stream",
    eventChannel: "ai://translate-stream",
    normalizeResult: (_source, result) => normalizeSkillMarkdownForPreview(unwrapOuterMarkdownFence(result).trim()),
  });
  const summaryStream = useAiStream({
    command: "ai_summarize_skill_stream",
    eventChannel: "ai://summarize-stream",
  });

  const translatedContent = translationStream.content;
  const translationVisible = translationStream.visible;
  const translating = translationStream.loading;
  const translationHasDelta = translationStream.hasDelta;
  const translationWasNonStreaming = translationStream.wasNonStreaming;
  const summaryContent = summaryStream.content;
  const summaryVisible = summaryStream.visible;
  const summarizing = summaryStream.loading;
  const summaryHasDelta = summaryStream.hasDelta;
  const aiConfigured = translationStream.aiConfigured;
  const targetLanguage = translationStream.targetLanguage;
  const aiError = translationStream.error ?? summaryStream.error;
  const localizedAiError = formatAiErrorMessage(aiError, t);

  const previewSource = normalizeSkillMarkdownForPreview(
    translationVisible && translatedContent != null ? translatedContent : content,
  );
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

  useEffect(() => {
    translationStream.hydrate(null, null);
    translationStream.setVisible(false);
    translationStream.setError(null);

    const summaryKey = buildCacheKey(targetLanguage, content);
    const cachedSummary = summaryCache.get(summaryKey) ?? null;
    summaryStream.hydrate(cachedSummary, cachedSummary ? content : null);
    summaryStream.setVisible(false);
    summaryStream.setError(null);

    setRetranslating(false);
  }, [content, targetLanguage]);

  const clearAiError = () => {
    translationStream.setError(null);
    summaryStream.setError(null);
  };

  const handleTranslate = async () => {
    if (!aiConfigured) return;

    if (translating) {
      translationStream.cancel();
      setRetranslating(false);
      return;
    }

    setRetranslating(false);
    clearAiError();
    await translationStream.execute(content);
  };

  const handleAiRetranslate = async () => {
    if (!aiConfigured || translating) return;

    setRetranslating(true);
    clearAiError();
    try {
      await translationStream.execute(content, {
        forceRefresh: true,
        keepVisibleWhileLoading: true,
      });
    } finally {
      setRetranslating(false);
    }
  };

  const handleSummarize = async () => {
    if (!aiConfigured) return;

    if (summarizing) {
      summaryStream.cancel();
      return;
    }

    clearAiError();
    const result = await summaryStream.execute(content);
    if (result != null) {
      const key = buildCacheKey(targetLanguage, content);
      summaryCache.set(key, result);
      trimCache(summaryCache);
    }
  };

  return (
    <ResizablePanel defaultWidth={600} storageKey="skill-reader-width">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-primary" />
          <h2 className="text-heading-sm truncate">{skillName}</h2>
          <span className="text-micro text-muted-foreground bg-muted/60 px-1.5 py-0.5 rounded font-mono">SKILL.md</span>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={onClose}>
            <X className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {/* Toolbar */}
      <div className="flex items-center gap-2 px-4 py-2 border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <Eye className="w-3.5 h-3.5 text-muted-foreground" />
          <span className="text-xs font-medium text-muted-foreground">{t("skillEditor.preview")}</span>
        </div>

        {/* AI Action Buttons */}
        <div className="ml-auto flex items-center gap-1 shrink-0">
          <motion.button
            onClick={() => {
              if (!aiConfigured) {
                navigateToAiSettings();
                return;
              }
              void handleTranslate();
            }}
            whileTap={{ scale: 0.94 }}
            animate={translating && !prefersReducedMotion ? {
              boxShadow: [
                "0 0 0px 0px rgba(239,68,68,0)",
                "0 0 12px 3px rgba(239,68,68,0.35)",
                "0 0 0px 0px rgba(239,68,68,0)",
              ],
            } : {}}
            transition={{ duration: 1.2, repeat: translating && !prefersReducedMotion ? Infinity : 0, ease: "easeInOut" }}
            className={`relative flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer overflow-hidden ${
              translating
                ? "bg-destructive/10 text-destructive"
                : translationVisible
                  ? "bg-primary/15 text-primary"
                  : aiConfigured
                    ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                    : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
            }`}
            title={
              aiConfigured
                ? translationVisible
                  ? "Show original"
                  : translatedContent
                    ? "Show cached translation"
                    : "Translate to target language"
                : "AI not configured"
            }
          >
            {/* Spinning icon when translating */}
            <AnimatePresence mode="wait">
              {translating ? (
                <motion.span
                  key="square"
                  initial={{ opacity: 0, rotate: -90, scale: 0.7 }}
                  animate={{ opacity: 1, rotate: 0, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.7 }}
                  transition={{ duration: 0.18 }}
                  className="flex items-center"
                >
                  <motion.div
                    animate={{ rotate: 360 }}
                    transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
                  >
                    <Square className="w-3 h-3 fill-current" />
                  </motion.div>
                </motion.span>
              ) : translationVisible ? (
                <motion.span
                  key="rotate"
                  initial={{ opacity: 0, rotate: -90, scale: 0.7 }}
                  animate={{ opacity: 1, rotate: 0, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.7 }}
                  transition={{ duration: 0.18 }}
                >
                  <RotateCcw className="w-3 h-3" />
                </motion.span>
              ) : (
                <motion.span
                  key="globe"
                  initial={{ opacity: 0, scale: 0.6 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.6 }}
                  transition={{ duration: 0.18 }}
                >
                  <Globe className="w-3 h-3" />
                </motion.span>
              )}
            </AnimatePresence>

            {/* Button label with crossfade */}
            <AnimatePresence mode="wait">
              <motion.span
                key={
                  translating
                    ? "cancel"
                    : translationVisible
                      ? "original"
                      : translatedContent
                        ? "show"
                        : "translate"
                }
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.16 }}
              >
                {translating
                  ? t("common.cancel")
                  : translationVisible
                    ? t("skillEditor.original")
                    : translatedContent
                      ? t("skillEditor.showTranslation")
                      : t("skillEditor.translate")}
              </motion.span>
            </AnimatePresence>
          </motion.button>
          {translatedContent && (
            <motion.button
              onClick={() => {
                if (!aiConfigured) {
                  navigateToAiSettings();
                  return;
                }
                void handleAiRetranslate();
              }}
              disabled={translating}
              whileTap={{ scale: 0.94 }}
              animate={translating && retranslating ? {
                boxShadow: [
                  "0 0 0px 0px rgba(239,68,68,0)",
                  "0 0 10px 2px rgba(239,68,68,0.3)",
                  "0 0 0px 0px rgba(239,68,68,0)",
                ],
              } : {}}
              transition={{ duration: 1, repeat: translating && retranslating ? Infinity : 0, ease: "easeInOut" }}
              className={`relative flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
                translating && retranslating
                  ? "bg-destructive/10 text-destructive"
                  : aiConfigured
                    ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                    : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
              } disabled:cursor-not-allowed disabled:opacity-60`}
              title={t("skillEditor.retranslateWithAi")}
            >
              {translating && retranslating ? (
                <motion.div
                  animate={{ rotate: 360 }}
                  transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
                >
                  <Square className="w-3 h-3 fill-current" />
                </motion.div>
              ) : (
                <Sparkles className="w-3 h-3" />
              )}
              {translating && retranslating
                ? t("skillEditor.retranslatingWithAi")
                : t("skillEditor.retranslateWithAi")}
            </motion.button>
          )}
          <button
            onClick={() => {
              if (!aiConfigured) {
                navigateToAiSettings();
                return;
              }
              void handleSummarize();
            }}
            className={`flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
              summarizing
                ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
                : summaryContent
                  ? "bg-primary/15 text-primary"
                  : aiConfigured
                    ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                    : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
            }`}
            title={
              aiConfigured
                ? summarizing
                  ? "Click to cancel"
                  : summaryContent
                    ? "Hide summary"
                    : "AI quick summary"
                : "AI not configured"
            }
          >
            {summarizing ? <Square className="w-3 h-3 fill-current" /> : <Sparkles className="w-3 h-3" />}
            {summarizing
              ? t("common.cancel")
              : summaryContent
                ? summaryVisible
                  ? t("skillEditor.hideSummary")
                  : t("skillEditor.summary")
                : t("skillEditor.summary")}
          </button>
        </div>
      </div>

      <AiNotConfiguredBanner show={!aiConfigured} />

      {/* AI Error Banner */}
      <AiErrorBanner error={localizedAiError} onDismiss={clearAiError} />

      {translating && translationVisible && translationHasDelta && (
        <div className="px-4 py-2 bg-primary/5 border-b border-primary/15">
          <span className="text-xs text-primary/90">{t("skillEditor.streamingPreview")}</span>
        </div>
      )}
      {!translating && translationVisible && translatedContent && translationWasNonStreaming && (
        <div className="px-4 py-2 bg-muted/40 border-b border-border">
          <span className="text-xs text-muted-foreground">{t("skillEditor.nonStreamingNotice")}</span>
        </div>
      )}

      {/* Content */}
      <div className="markdown-content flex-1 p-4 overflow-y-auto overscroll-y-contain prose prose-sm dark:prose-invert max-w-none">
        {/* AI Summary Card */}
        {summaryVisible && summaryContent !== null && (
          <div className="not-prose mb-4 rounded-lg border border-primary/20 bg-primary/5 p-4">
            <div className="flex items-center gap-2 mb-2">
              <Sparkles className="w-4 h-4 text-primary" />
              <span className="text-sm font-medium text-primary">{t("skillEditor.aiSummary")}</span>
              {summarizing && summaryHasDelta && (
                <span className="text-micro text-primary/70 bg-primary/10 px-1.5 py-0.5 rounded">
                  {t("skillEditor.streamingPreview")}
                </span>
              )}
            </div>
            {summaryContent ? (
              <Markdown streaming={summarizing} className="text-sm">
                {summaryContent}
              </Markdown>
            ) : summarizing ? (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                <span>{t("skillEditor.summarizing")}</span>
              </div>
            ) : null}
          </div>
        )}

        {previewFrontmatterEntries.length > 0 && (
          <div className="not-prose mb-4 overflow-hidden rounded-lg border border-border bg-card/60">
            <table className="w-full border-collapse text-sm">
              <tbody>
                {previewFrontmatterEntries.map((entry) => (
                  <tr key={entry.key} className="border-b border-border last:border-b-0">
                    <th className="w-44 bg-muted/40 px-3 py-2 text-left align-top font-medium text-foreground/90">
                      {entry.key}
                    </th>
                    <td className="px-3 py-2 text-foreground break-words">
                      <Markdown className="[&_p]:my-1 [&_pre]:my-2 [&_ul]:my-1 [&_ol]:my-1">{entry.value}</Markdown>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* Main content */}
        {previewContent.trim().length === 0 && previewFrontmatterEntries.length === 0 ? (
          <div className="text-sm text-muted-foreground">{t("skillEditor.noContent")}</div>
        ) : (
          <Markdown
            streaming={translating && translationVisible}
            fallback={<div className="text-sm text-muted-foreground">Loading preview...</div>}
          >
            {previewContent}
          </Markdown>
        )}
      </div>
    </ResizablePanel>
  );
}
