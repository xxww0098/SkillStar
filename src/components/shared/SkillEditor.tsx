import {
  Eye,
  FileText,
  Globe,
  Loader2,
  PanelLeftClose,
  PanelLeftOpen,
  RotateCcw,
  Save,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { useAiStream } from "../../hooks/useAiStream";
import {
  normalizeSkillMarkdownForPreview,
  normalizeTranslatedDocument,
  parseFrontmatterEntries,
  splitFrontmatter,
} from "../../lib/frontmatter";
import { formatAiErrorMessage, navigateToAiSettings } from "../../lib/utils";
import type { SkillContent } from "../../types";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";
import { AiErrorBanner, AiNotConfiguredBanner } from "./AiBanners";

interface SkillEditorProps {
  skillName: string;
  onClose: () => void;
  onRead: (name: string) => Promise<SkillContent>;
  onSave: (name: string, content: string) => Promise<void>;
}
export function SkillEditor({ skillName, onClose, onRead, onSave }: SkillEditorProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState<SkillContent | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [editedContent, setEditedContent] = useState("");
  const [hasChanges, setHasChanges] = useState(false);
  const [isLeftPaneOpen, setIsLeftPaneOpen] = useState(false);
  const prefersReducedMotion = useReducedMotion();

  // AI features via shared hook
  const [retranslating, setRetranslating] = useState(false);

  const translationStream = useAiStream({
    command: "ai_translate_skill_stream",
    eventChannel: "ai://translate-stream",
    normalizeResult: (source, result) => normalizeTranslatedDocument(source, result),
  });
  const summaryStream = useAiStream({
    command: "ai_summarize_skill_stream",
    eventChannel: "ai://summarize-stream",
  });

  const translatedContent = translationStream.content;
  const translatedSource = translationStream.source;
  const translationVisible = translationStream.visible;
  const translating = translationStream.loading;
  const translationHasDelta = translationStream.hasDelta;
  const translationWasNonStreaming = translationStream.wasNonStreaming;
  const summaryContent = summaryStream.content;
  const summaryVisible = summaryStream.visible;
  const summarizing = summaryStream.loading;
  const summaryHasDelta = summaryStream.hasDelta;
  const aiConfigured = translationStream.aiConfigured;
  const aiError = translationStream.error ?? summaryStream.error;
  const localizedAiError = formatAiErrorMessage(aiError, t);

  const previewSource = normalizeSkillMarkdownForPreview(
    translationVisible && translatedContent != null ? translatedContent : editedContent,
  );
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

  const handleTranslate = async () => {
    if (!aiConfigured || loadError) return;

    if (translating) {
      translationStream.cancel();
      setRetranslating(false);
      return;
    }

    setRetranslating(false);
    clearAiError();
    await translationStream.execute(editedContent);
  };

  const handleAiRetranslate = async () => {
    if (!aiConfigured || translating || loadError) return;

    setRetranslating(true);
    clearAiError();
    try {
      await translationStream.execute(editedContent, {
        forceRefresh: true,
        keepVisibleWhileLoading: true,
      });
    } finally {
      setRetranslating(false);
    }
  };

  const handleSummarize = async () => {
    if (!aiConfigured || loadError) return;

    if (summarizing) {
      summaryStream.cancel();
      return;
    }

    clearAiError();
    await summaryStream.execute(editedContent);
  };

  const clearAiError = () => {
    translationStream.setError(null);
    summaryStream.setError(null);
  };

  useEffect(() => {
    const loadContent = async () => {
      setLoading(true);
      setLoadError(null);
      try {
        const latestContent = await onRead(skillName);
        setContent(latestContent);
        setEditedContent(latestContent.content);
        translationStream.hydrate(null, null);
        translationStream.setVisible(false);
      } catch (e) {
        console.error("Failed to load skill content:", e);
        setContent(null);
        setEditedContent("");
        setHasChanges(false);
        setLoadError(String(e));
      } finally {
        setLoading(false);
      }
    };
    loadContent();
  }, [skillName, onRead]);

  const handleSave = async () => {
    if (!content) return;
    setSaving(true);
    try {
      await onSave(skillName, editedContent);
      setHasChanges(false);
      const latestContent = await onRead(skillName);
      setContent(latestContent);
    } catch (e) {
      console.error("Failed to save:", e);
    } finally {
      setSaving(false);
    }
  };

  const handleContentChange = (value: string) => {
    setEditedContent(value);
    setHasChanges(value !== content?.content);
    if (translationVisible) translationStream.setVisible(false);
    if (summaryVisible) summaryStream.setVisible(false);
  };

  if (loading) {
    return (
      <ResizablePanel defaultWidth={600} storageKey="skill-editor-width">
        <div className="flex-1 flex items-center justify-center">
          <span className="text-muted-foreground text-sm">{t("skillEditor.loadingContent")}</span>
        </div>
      </ResizablePanel>
    );
  }

  return (
    <ResizablePanel defaultWidth={800} storageKey="skill-editor-width">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-primary" />
          <h2 className="text-heading-sm truncate">{skillName}</h2>
          {hasChanges && (
            <span className="text-xs text-warning px-1.5 py-0.5 bg-warning/10 rounded">{t("skillEditor.unsaved")}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={onClose}>
            <X className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left Pane - Edit */}
        {isLeftPaneOpen && (
          <div className="w-1/2 flex flex-col border-r border-border">
            <div className="flex-1 flex flex-col">
              <textarea
                className="flex-1 w-full p-4 text-sm bg-input border-0 resize-none focus:outline-none font-mono backdrop-blur-sm"
                value={editedContent}
                onChange={(e) => handleContentChange(e.target.value)}
                spellCheck={false}
              />
            </div>
          </div>
        )}

        {/* Right Pane - Preview */}
        <div className={`flex flex-col ${isLeftPaneOpen ? "w-1/2" : "flex-1"}`}>
          <div className="flex items-center gap-2 px-4 py-2 border-b border-border shrink-0">
            <button
              onClick={() => setIsLeftPaneOpen(!isLeftPaneOpen)}
              className="p-1 -ml-1 rounded-md hover:bg-card-hover text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
              title={isLeftPaneOpen ? "Collapse editor" : "Expand editor"}
            >
              {isLeftPaneOpen ? <PanelLeftClose className="w-4 h-4" /> : <PanelLeftOpen className="w-4 h-4" />}
            </button>
            <div className="flex items-center gap-2 border-l border-border pl-2">
              <Eye className="w-3.5 h-3.5 text-muted-foreground" />
              <span className="text-xs font-medium text-muted-foreground">{t("skillEditor.preview")}</span>
            </div>

            {/* AI Action Buttons */}
            <div className="ml-auto flex items-center gap-1 shrink-0">
              <motion.button
                onClick={() => {
                  if (loadError) return;
                  if (!aiConfigured) {
                    navigateToAiSettings();
                    return;
                  }
                  void handleTranslate();
                }}
                disabled={!!loadError}
                whileTap={!loadError ? { scale: 0.94 } : {}}
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
                      : aiConfigured && !loadError
                        ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                        : loadError
                          ? "text-muted-foreground/50 cursor-not-allowed"
                          : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
                }`}
                title={
                  loadError
                    ? t("skillEditor.loadFailed")
                    : aiConfigured
                      ? translationVisible
                        ? "Show original"
                        : translatedContent && translatedSource === editedContent
                          ? "Show cached translation"
                          : "Translate to target language"
                      : "AI not configured (optional). Editing is still available."
                }
              >
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

                <AnimatePresence mode="wait">
                  <motion.span
                    key={
                      translating
                        ? "cancel"
                        : translationVisible
                          ? "original"
                          : translatedContent && translatedSource === editedContent
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
                        : translatedContent && translatedSource === editedContent
                          ? t("skillEditor.showTranslation")
                          : t("skillEditor.translate")}
                  </motion.span>
                </AnimatePresence>
              </motion.button>
              {translatedContent && translatedSource === editedContent && (
                <motion.button
                  onClick={() => {
                    if (loadError) return;
                    if (!aiConfigured) {
                      navigateToAiSettings();
                      return;
                    }
                    void handleAiRetranslate();
                  }}
                  disabled={!!loadError || translating}
                  whileTap={{ scale: 0.94 }}
                  animate={translating && retranslating && !prefersReducedMotion ? {
                    boxShadow: [
                      "0 0 0px 0px rgba(239,68,68,0)",
                      "0 0 10px 2px rgba(239,68,68,0.3)",
                      "0 0 0px 0px rgba(239,68,68,0)",
                    ],
                  } : {}}
                  transition={{ duration: 1, repeat: translating && retranslating && !prefersReducedMotion ? Infinity : 0, ease: "easeInOut" }}
                  className={`relative flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
                    translating && retranslating
                      ? "bg-destructive/10 text-destructive"
                      : aiConfigured && !loadError
                        ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                        : loadError
                          ? "text-muted-foreground/50 cursor-not-allowed"
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
                  if (loadError) return;
                  if (!aiConfigured) {
                    navigateToAiSettings();
                    return;
                  }
                  void handleSummarize();
                }}
                disabled={!!loadError}
                className={`flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
                  summarizing
                    ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
                    : summaryContent
                      ? "bg-primary/15 text-primary"
                      : aiConfigured && !loadError
                        ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                        : loadError
                          ? "text-muted-foreground/50 cursor-not-allowed"
                          : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
                }`}
                title={
                  loadError
                    ? t("skillEditor.loadFailed")
                    : aiConfigured
                      ? summarizing
                        ? "Click to cancel"
                        : summaryContent && summaryVisible
                          ? "Hide summary"
                          : "AI quick summary"
                      : "AI not configured (optional). Editing is still available."
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

          {loadError && (
            <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20">
              <div className="text-xs font-medium text-destructive">{t("skillEditor.loadFailed")}</div>
              <div className="text-xs text-destructive/90 break-words mt-0.5">{loadError}</div>
            </div>
          )}

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

            {/* Main content: translated or original */}
            {loadError ? (
              <div className="text-sm text-muted-foreground">{t("skillEditor.noContent")}</div>
            ) : previewContent.trim().length === 0 && previewFrontmatterEntries.length === 0 ? (
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
        </div>
      </div>

      {/* Footer */}
      <div className="flex items-center justify-end gap-2 p-4 border-t border-border shrink-0 bg-card/50 backdrop-blur-sm">
        <div className="mr-auto">
          {hasChanges && (
            <Button
              variant="destructive"
              size="sm"
              className="cursor-pointer"
              onClick={() => {
                if (content) {
                  setEditedContent(content.content);
                  setHasChanges(false);
                  if (translationVisible) translationStream.setVisible(false);
                }
              }}
              title="Discard unsaved changes"
            >
              <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
              {t("skillEditor.reset")}
            </Button>
          )}
        </div>
        <Button variant="outline" onClick={onClose} className="cursor-pointer">
          {t("common.cancel")}
        </Button>
        <Button onClick={handleSave} disabled={!hasChanges || saving} className="cursor-pointer">
          <Save className="w-4 h-4 mr-2" />
          {saving ? t("common.saving") : t("skillEditor.saveChanges")}
        </Button>
      </div>
    </ResizablePanel>
  );
}
