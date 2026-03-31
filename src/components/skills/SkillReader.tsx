import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { X, FileText, Eye, Globe, Sparkles, Loader2, RotateCcw, Square } from "lucide-react";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";
import { unwrapOuterMarkdownFence, navigateToAiSettings } from "../../lib/utils";
import type { AiConfigStatus, AiStreamPayload, FrontmatterEntry } from "../../types";

interface SkillReaderProps {
  skillName: string;
  content: string;
  onClose: () => void;
}

const FRONTMATTER_RE = /^\uFEFF?---\s*\r?\n([\s\S]*?)\r?\n---\s*(?:\r?\n|$)/;

function splitFrontmatter(content: string): { frontmatter: string | null; body: string } {
  const match = content.match(FRONTMATTER_RE);
  if (!match) {
    return { frontmatter: null, body: content };
  }
  return {
    frontmatter: match[1],
    body: content.slice(match[0].length),
  };
}

function parseFrontmatterEntries(frontmatter: string | null): FrontmatterEntry[] {
  if (!frontmatter) {
    return [];
  }

  const entries: FrontmatterEntry[] = [];
  const lines = frontmatter.split(/\r?\n/);
  let current: FrontmatterEntry | null = null;

  for (let i = 0; i < lines.length; i += 1) {
    const rawLine = lines[i];
    const line = rawLine.trimEnd();
    if (!line.trim()) continue;

    const keyValueMatch = line.match(/^([a-zA-Z0-9_-]+)\s*:\s*(.*)$/);
    if (keyValueMatch) {
      const rawValue = keyValueMatch[2] ?? "";
      const isBlockScalar = /^[|>][-+]?$/.test(rawValue.trim());
      let value = rawValue;

      if (isBlockScalar) {
        const blockLines: string[] = [];
        let j = i + 1;
        while (j < lines.length) {
          const next = lines[j];
          if (!next.trim()) {
            blockLines.push("");
            j += 1;
            continue;
          }
          if (/^\s+/.test(next)) {
            blockLines.push(next);
            j += 1;
            continue;
          }
          break;
        }

        const nonEmpty = blockLines.filter((l) => l.trim().length > 0);
        const minIndent = nonEmpty.length > 0
          ? Math.min(...nonEmpty.map((l) => (l.match(/^\s*/) || [""])[0].length))
          : 0;
        value = blockLines
          .map((l) => (l.trim().length > 0 ? l.slice(minIndent) : ""))
          .join("\n")
          .trimEnd();
        i = j - 1;
      }

      current = {
        key: keyValueMatch[1],
        value,
      };
      entries.push(current);
      continue;
    }

    if (current && /^\s+/.test(rawLine)) {
      current.value = `${current.value}\n${line.trim()}`;
    }
  }

  return entries;
}

/** Module-level translation cache: content → translatedContent. Survives component unmount. */
const translationCache = new Map<string, string>();
/** Module-level summary cache: content → summaryContent. */
const summaryCache = new Map<string, string>();

export function SkillReader({ skillName, content, onClose }: SkillReaderProps) {
  const { t } = useTranslation();

  // AI features
  const [translatedContent, setTranslatedContent] = useState<string | null>(() =>
    translationCache.get(content) ?? null
  );
  const [translationVisible, setTranslationVisible] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [translationHasDelta, setTranslationHasDelta] = useState(false);
  const [translationWasNonStreaming, setTranslationWasNonStreaming] = useState(false);
  const [summaryContent, setSummaryContent] = useState<string | null>(() =>
    summaryCache.get(content) ?? null
  );
  const [summaryVisible, setSummaryVisible] = useState(false);
  const [summarizing, setSummarizing] = useState(false);
  const [summaryHasDelta, setSummaryHasDelta] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState(false);

  // Cancel refs
  const activeTranslateIdRef = useRef<string | null>(null);
  const activeSummarizeIdRef = useRef<string | null>(null);
  const translateUnlistenRef = useRef<(() => void) | null>(null);
  const summarizeUnlistenRef = useRef<(() => void) | null>(null);

  const previewSource = translationVisible && translatedContent != null ? translatedContent : content;
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

  useEffect(() => {
    const loadAiConfig = async () => {
      try {
        const config = await invoke<AiConfigStatus>("get_ai_config");
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
      if (summarizeUnlistenRef.current) {
        summarizeUnlistenRef.current();
        summarizeUnlistenRef.current = null;
      }
    };
  }, []);

  const handleTranslate = async () => {
    if (!aiConfigured) return;

    // Cancel in-progress translation
    if (translating) {
      activeTranslateIdRef.current = null;
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      setTranslating(false);
      if (!translatedContent) {
        setTranslationVisible(false);
      }
      return;
    }

    if (translationVisible) {
      setTranslationVisible(false);
      return;
    }

    if (translatedContent) {
      setTranslationVisible(true);
      return;
    }

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `reader-translate-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeTranslateIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setTranslating(true);
    setAiError(null);
    setTranslationHasDelta(false);
    setTranslationWasNonStreaming(false);
    // Don't set translationVisible or translatedContent yet —
    // keep showing original content until first delta or final result arrives.
    try {
      const unlisten = await listen<AiStreamPayload>("ai://translate-stream", (event) => {
        if (activeTranslateIdRef.current !== requestId) return;
        const payload = event.payload;
        if (payload.requestId !== requestId) return;

        if (payload.event === "delta" && payload.delta) {
          deltaCount += 1;
          if (deltaCount >= 2) setTranslationHasDelta(true);
          streamedRaw += payload.delta;
          setTranslatedContent(streamedRaw);
          // Show translated content now that we have actual data
          setTranslationVisible(true);
          return;
        }

        if (payload.event === "error" && payload.message) {
          setAiError(payload.message);
        }
      });
      translateUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_translate_skill_stream", {
        requestId,
        content,
      });

      if (activeTranslateIdRef.current !== requestId) return;
      const final = unwrapOuterMarkdownFence(result).trim();
      setTranslatedContent(final);
      setTranslationVisible(true);
      setTranslationWasNonStreaming(deltaCount < 2);
      // Cache completed translation
      translationCache.set(content, final);
    } catch (e) {
      if (activeTranslateIdRef.current !== requestId) return;
      setTranslationHasDelta(deltaCount >= 2);
      setTranslationWasNonStreaming(false);
      if (!streamedRaw.trim()) {
        setTranslatedContent(null);
        setTranslationVisible(false);
      } else {
        setTranslatedContent(streamedRaw);
        setTranslationVisible(true);
      }
      setAiError(String(e));
    } finally {
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      if (activeTranslateIdRef.current === requestId) {
        setTranslating(false);
        activeTranslateIdRef.current = null;
      }
    }
  };

  const handleSummarize = async () => {
    if (!aiConfigured) return;

    // Cancel in-progress summarize
    if (summarizing) {
      activeSummarizeIdRef.current = null;
      if (summarizeUnlistenRef.current) {
        summarizeUnlistenRef.current();
        summarizeUnlistenRef.current = null;
      }
      setSummarizing(false);
      if (!summaryContent) {
        setSummaryContent(null);
      }
      return;
    }

    if (summaryContent) {
      setSummaryVisible((v) => !v);
      return;
    }

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `reader-summary-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeSummarizeIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setSummarizing(true);
    setAiError(null);
    setSummaryHasDelta(false);
    setSummaryContent(null);
    setSummaryVisible(true);
    try {
      const unlisten = await listen<AiStreamPayload>("ai://summarize-stream", (event) => {
        if (activeSummarizeIdRef.current !== requestId) return;
        const payload = event.payload;
        if (payload.requestId !== requestId) return;

        if (payload.event === "delta" && payload.delta) {
          deltaCount += 1;
          if (deltaCount >= 2) setSummaryHasDelta(true);
          streamedRaw += payload.delta;
          setSummaryContent(streamedRaw);
          return;
        }

        if (payload.event === "error" && payload.message) {
          setAiError(payload.message);
        }
      });
      summarizeUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_summarize_skill_stream", {
        requestId,
        content,
      });

      if (activeSummarizeIdRef.current !== requestId) return;
      setSummaryContent(result);
      // Cache completed summary
      summaryCache.set(content, result);
    } catch (e) {
      if (activeSummarizeIdRef.current !== requestId) return;
      if (!streamedRaw.trim()) {
        setSummaryContent(null);
      } else {
        setSummaryContent(streamedRaw);
      }
      setAiError(String(e));
    } finally {
      if (summarizeUnlistenRef.current) {
        summarizeUnlistenRef.current();
        summarizeUnlistenRef.current = null;
      }
      if (activeSummarizeIdRef.current === requestId) {
        setSummarizing(false);
        activeSummarizeIdRef.current = null;
      }
    }
  };

  return (
    <ResizablePanel defaultWidth={600} storageKey="skill-reader-width">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-primary" />
          <h2 className="text-heading-sm truncate">{skillName}</h2>
          <span className="text-micro text-muted-foreground bg-muted/60 px-1.5 py-0.5 rounded font-mono">
            SKILL.md
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={onClose}
          >
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
            onClick={() => {
              if (!aiConfigured) {
                navigateToAiSettings();
                return;
              }
              void handleTranslate();
            }}
            className={`flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
              translating
                ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
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
            {translating ? (
              <Square className="w-3 h-3 fill-current" />
            ) : translationVisible ? (
              <RotateCcw className="w-3 h-3" />
            ) : (
              <Globe className="w-3 h-3" />
            )}
            {translating
              ? t("common.cancel")
              : translationVisible
              ? t("skillEditor.original")
              : translatedContent
              ? t("skillEditor.showTranslation")
              : t("skillEditor.translate")}
          </button>
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
            {summarizing ? (
              <Square className="w-3 h-3 fill-current" />
            ) : (
              <Sparkles className="w-3 h-3" />
            )}
            {summarizing ? t("common.cancel") : summaryContent ? (summaryVisible ? t("skillEditor.hideSummary") : t("skillEditor.summary")) : t("skillEditor.summary")}
          </button>
        </div>
      </div>

      {!aiConfigured && (
        <div className="px-4 py-2 border-b border-border bg-muted/30 flex items-center gap-2">
          <span className="text-micro text-muted-foreground flex-1">
            {t("skillEditor.aiNotConfigured")}
          </span>
          <button
            onClick={navigateToAiSettings}
            className="px-2 py-1 rounded-md text-micro font-medium border border-border hover:bg-card-hover transition-colors cursor-pointer"
          >
            {t("skillEditor.configureAI")}
          </button>
        </div>
      )}

      {/* AI Error Banner */}
      {aiError && (
        <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20 flex items-center gap-2">
          <span className="text-xs text-destructive flex-1">{aiError}</span>
          <button
            onClick={() => setAiError(null)}
            className="text-destructive/60 hover:text-destructive cursor-pointer"
          >
            <X className="w-3 h-3" />
          </button>
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

      {/* Content */}
      <div className="markdown-content flex-1 p-4 overflow-y-auto prose prose-sm dark:prose-invert max-w-none">
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
              <Markdown className="text-sm">
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
                      <Markdown className="[&_p]:my-1 [&_pre]:my-2 [&_ul]:my-1 [&_ol]:my-1">
                        {entry.value}
                      </Markdown>
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
          <Markdown fallback={<div className="text-sm text-muted-foreground">Loading preview...</div>}>
            {previewContent}
          </Markdown>
        )}
      </div>
    </ResizablePanel>
  );
}
