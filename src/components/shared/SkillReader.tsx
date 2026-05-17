import { Eye, FileText, Loader2, Sparkles, Square, X } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAiStream } from "../../hooks/useAiStream";
import { normalizeSkillMarkdownForPreview, parseFrontmatterEntries, splitFrontmatter } from "../../lib/frontmatter";
import { formatAiErrorMessage, navigateToAiSettings } from "../../lib/utils";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";
import { AiErrorBanner } from "./AiBanners";

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

  const summaryStream = useAiStream({
    command: "ai_summarize_skill_stream",
    eventChannel: "ai://summarize-stream",
  });

  const summaryContent = summaryStream.content;
  const summaryVisible = summaryStream.visible;
  const summarizing = summaryStream.loading;
  const summaryHasDelta = summaryStream.hasDelta;
  const summaryAiConfigured = summaryStream.aiConfigured;
  const targetLanguage = summaryStream.targetLanguage;
  const aiError = summaryStream.error;
  const localizedAiError = formatAiErrorMessage(aiError, t);

  const previewSource = normalizeSkillMarkdownForPreview(content);
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

  useEffect(() => {
    const summaryKey = buildCacheKey(targetLanguage, content);
    const cachedSummary = summaryCache.get(summaryKey) ?? null;
    summaryStream.hydrate(cachedSummary, cachedSummary ? content : null);
    summaryStream.setVisible(false);
    summaryStream.setError(null);
  }, [content, summaryStream.hydrate, summaryStream.setError, summaryStream.setVisible, targetLanguage]);

  const clearAiError = () => {
    summaryStream.setError(null);
  };

  const handleSummarize = async () => {
    if (!summaryAiConfigured) return;

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
          <button
            type="button"
            onClick={() => {
              if (!summaryAiConfigured) {
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
                  : summaryAiConfigured
                    ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                    : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
            }`}
            title={
              summaryAiConfigured
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

      {/* AI Error Banner */}
      <AiErrorBanner error={localizedAiError} onDismiss={clearAiError} />

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
            streaming={false}
            fallback={<div className="text-sm text-muted-foreground">Loading preview...</div>}
          >
            {previewContent}
          </Markdown>
        )}
      </div>
    </ResizablePanel>
  );
}
