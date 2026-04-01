import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { X, Save, FileText, Eye, PanelLeftClose, PanelLeftOpen, Globe, Sparkles, Loader2, RotateCcw, Square } from "lucide-react";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";
import {
  formatAiErrorMessage,
  normalizeSkillMarkdownForPreview,
  unwrapOuterMarkdownFence,
  navigateToAiSettings,
} from "../../lib/utils";
import type { AiConfigStatus, AiStreamPayload, FrontmatterEntry, SkillContent } from "../../types";

interface SkillEditorProps {
  skillName: string;
  onClose: () => void;
  onRead: (name: string) => Promise<SkillContent>;
  onSave: (name: string, content: string) => Promise<void>;
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

function readFrontmatterValue(frontmatter: string, key: string): string | null {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = frontmatter.match(new RegExp(`^\\s*${escapedKey}:\\s*(.+)\\s*$`, "m"));
  return match ? match[1].trim() : null;
}

function readDescriptionFromAnyText(text: string): string | null {
  const lineMatch = text.match(/^\s*description:\s*(.+)\s*$/m);
  if (lineMatch) {
    return lineMatch[1].trim();
  }

  // Handle collapsed single-line metadata like:
  // "name: ... description: ... user-invocable: false"
  const inlineMatch = text.match(
    /\bdescription:\s*([\s\S]*?)(?=\s+\b[a-zA-Z][a-zA-Z-]*:\s|$)/i
  );
  return inlineMatch ? inlineMatch[1].trim() : null;
}

function writeFrontmatterValue(frontmatter: string, key: string, value: string): string {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const line = `${key}: ${value}`;
  const keyLineRe = new RegExp(`^\\s*${escapedKey}:\\s*.+$`, "m");
  if (keyLineRe.test(frontmatter)) {
    return frontmatter.replace(keyLineRe, line);
  }
  return `${line}\n${frontmatter}`.trim();
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

function stripLeadingDuplicatedMetadata(
  content: string,
  allowedKeys: ReadonlySet<string>
): string {
  if (allowedKeys.size === 0) {
    return content;
  }

  const lines = content.replace(/^\uFEFF/, "").split(/\r?\n/);
  let start = 0;
  while (start < lines.length && !lines[start].trim()) {
    start += 1;
  }
  if (start >= lines.length) {
    return content;
  }

  const keyRe = /^([a-zA-Z0-9_-]+)\s*:/;
  const firstLine = lines[start].trimStart();
  const firstKey = firstLine.match(keyRe)?.[1] ?? null;
  if (!firstKey || !allowedKeys.has(firstKey)) {
    return content;
  }

  // Collapsed one-liner metadata:
  // name: ... description: ... argument-hint: ... user-invocable: ...
  const inlineKeys = Array.from(
    firstLine.matchAll(/([a-zA-Z0-9_-]+)\s*:/g),
    (m) => m[1]
  );
  const inlineKnownCount = inlineKeys.filter((k) => allowedKeys.has(k)).length;
  if (inlineKnownCount >= 2) {
    let index = start + 1;
    while (index < lines.length && !lines[index].trim()) {
      index += 1;
    }
    return lines.slice(index).join("\n");
  }

  // Multi-line key/value metadata block at document top.
  let index = start;
  let consumed = false;
  while (index < lines.length) {
    const raw = lines[index];
    const trimmed = raw.trim();
    if (!trimmed) {
      if (consumed) {
        index += 1;
        break;
      }
      index += 1;
      continue;
    }

    const key = raw.trimStart().match(keyRe)?.[1] ?? null;
    if (key && allowedKeys.has(key)) {
      consumed = true;
      index += 1;
      continue;
    }

    if (consumed && /^\s+/.test(raw)) {
      index += 1;
      continue;
    }
    break;
  }

  if (!consumed) {
    return content;
  }
  return lines.slice(index).join("\n");
}

function normalizeTranslatedDocument(
  originalContent: string,
  translatedContent: string
): string {
  const translatedRaw = unwrapOuterMarkdownFence(translatedContent);
  const original = splitFrontmatter(originalContent);
  const translated = splitFrontmatter(translatedRaw);
  const frontmatterKeys = new Set(
    parseFrontmatterEntries(original.frontmatter).map((entry) => entry.key)
  );

  // No frontmatter: use translated document directly.
  if (!original.frontmatter) {
    return translatedRaw;
  }

  // Preferred path: AI returned frontmatter and body.
  if (translated.frontmatter) {
    let mergedFrontmatter = translated.frontmatter;
    const originalName = readFrontmatterValue(original.frontmatter, "name");
    if (originalName) {
      mergedFrontmatter = writeFrontmatterValue(mergedFrontmatter, "name", originalName);
    }
    const translatedBody = stripLeadingDuplicatedMetadata(translated.body, frontmatterKeys);
    return normalizeSkillMarkdownForPreview(
      `---\n${mergedFrontmatter}\n---${translatedBody ? `\n${translatedBody}` : ""}`
    );
  }

  // Fallback path: keep original frontmatter structure, patch translated description if present.
  const translatedDescription =
    readDescriptionFromAnyText(translatedRaw) ??
    readFrontmatterValue(original.frontmatter, "description");

  const mergedFrontmatter = translatedDescription
    ? writeFrontmatterValue(original.frontmatter, "description", translatedDescription)
    : original.frontmatter;

  const translatedBody = stripLeadingDuplicatedMetadata(translatedRaw, frontmatterKeys);
  return normalizeSkillMarkdownForPreview(
    `---\n${mergedFrontmatter}\n---${translatedBody ? `\n${translatedBody}` : ""}`
  );
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

  // AI features
  const [translatedContent, setTranslatedContent] = useState<string | null>(null);
  const [translatedSource, setTranslatedSource] = useState<string | null>(null);
  const [translationVisible, setTranslationVisible] = useState(false);
  const [translating, setTranslating] = useState(false);
  const [retranslating, setRetranslating] = useState(false);
  const [translationHasDelta, setTranslationHasDelta] = useState(false);
  const [translationWasNonStreaming, setTranslationWasNonStreaming] = useState(false);
  const [summaryContent, setSummaryContent] = useState<string | null>(null);
  const [summarizing, setSummarizing] = useState(false);
  const [summaryHasDelta, setSummaryHasDelta] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState(false);

  // Cancel refs — cleared to signal cancellation, checked by event listeners
  const activeTranslateIdRef = useRef<string | null>(null);
  const activeSummarizeIdRef = useRef<string | null>(null);
  const translateUnlistenRef = useRef<(() => void) | null>(null);
  const summarizeUnlistenRef = useRef<(() => void) | null>(null);

  const previewSource = normalizeSkillMarkdownForPreview(
    translationVisible && translatedContent != null ? translatedContent : editedContent
  );
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;
  const localizedAiError = formatAiErrorMessage(aiError, t);

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
      setRetranslating(false);
      if (!translatedContent) {
        setTranslationVisible(false);
      }
      return;
    }

    if (translationVisible) {
      setTranslationVisible(false);
      return;
    }

    if (translatedContent && translatedSource === editedContent) {
      setTranslationVisible(true);
      return;
    }

    const sourceContent = editedContent;
    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `translate-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeTranslateIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setTranslating(true);
    setRetranslating(false);
    setAiError(null);
    setTranslationHasDelta(false);
    setTranslationWasNonStreaming(false);
    // Don't set translationVisible or translatedContent yet —
    // keep showing original content until first delta or final result arrives.
    setTranslatedSource(null);

    let rafId: number | null = null;
    const flushDelta = () => {
      rafId = null;
      if (activeTranslateIdRef.current !== requestId) return;
      setTranslatedContent(streamedRaw);
      setTranslationVisible(true);
      if (deltaCount >= 2) setTranslationHasDelta(true);
    };

    try {
      const unlisten = await listen<AiStreamPayload>("ai://translate-stream", (event) => {
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
          setAiError(payload.message);
        }
      });
      translateUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_translate_skill_stream", {
        requestId,
        content: sourceContent,
      });

      if (activeTranslateIdRef.current !== requestId) return;
      const normalized = normalizeTranslatedDocument(sourceContent, result);
      setTranslatedContent(normalized);
      setTranslatedSource(sourceContent);
      setTranslationVisible(true);
      setTranslationWasNonStreaming(deltaCount < 2);
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
      if (rafId != null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      if (translateUnlistenRef.current) {
        translateUnlistenRef.current();
        translateUnlistenRef.current = null;
      }
      if (activeTranslateIdRef.current === requestId) {
        setTranslating(false);
        setRetranslating(false);
        activeTranslateIdRef.current = null;
      }
    }
  };

  const handleAiRetranslate = async () => {
    if (!aiConfigured || translating || loadError) return;

    const sourceContent = editedContent;
    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `retranslate-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeTranslateIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setTranslating(true);
    setRetranslating(true);
    setAiError(null);
    setTranslationHasDelta(false);
    setTranslationWasNonStreaming(false);

    let rafId: number | null = null;
    const flushDelta = () => {
      rafId = null;
      if (activeTranslateIdRef.current !== requestId) return;
      setTranslatedContent(streamedRaw);
      setTranslationVisible(true);
      if (deltaCount >= 2) setTranslationHasDelta(true);
    };

    try {
      const unlisten = await listen<AiStreamPayload>("ai://translate-stream", (event) => {
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
          setAiError(payload.message);
        }
      });
      translateUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_translate_skill_stream", {
        requestId,
        content: sourceContent,
        forceRefresh: true,
      });

      if (activeTranslateIdRef.current !== requestId) return;
      const normalized = normalizeTranslatedDocument(sourceContent, result);
      setTranslatedContent(normalized);
      setTranslatedSource(sourceContent);
      setTranslationVisible(true);
      setTranslationWasNonStreaming(deltaCount < 2);
    } catch (e) {
      if (activeTranslateIdRef.current !== requestId) return;
      setTranslationHasDelta(deltaCount >= 2);
      setTranslationWasNonStreaming(false);
      if (streamedRaw.trim()) {
        setTranslatedContent(streamedRaw);
        setTranslationVisible(true);
      }
      setAiError(String(e));
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
        setTranslating(false);
        setRetranslating(false);
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
      setSummaryContent(null);
      setSummaryHasDelta(false);
      return;
    }

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `summary-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    activeSummarizeIdRef.current = requestId;
    let streamedRaw = "";
    let deltaCount = 0;

    setSummarizing(true);
    setAiError(null);
    setSummaryHasDelta(false);
    setSummaryContent(null);

    let rafId: number | null = null;
    const flushDelta = () => {
      rafId = null;
      if (activeSummarizeIdRef.current !== requestId) return;
      setSummaryContent(streamedRaw);
      if (deltaCount >= 2) setSummaryHasDelta(true);
    };

    try {
      const unlisten = await listen<AiStreamPayload>("ai://summarize-stream", (event) => {
        if (activeSummarizeIdRef.current !== requestId) return;
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
          setAiError(payload.message);
        }
      });
      summarizeUnlistenRef.current = unlisten;

      const result = await invoke<string>("ai_summarize_skill_stream", {
        requestId,
        content: editedContent,
      });

      if (activeSummarizeIdRef.current !== requestId) return;
      setSummaryContent(result);
    } catch (e) {
      if (activeSummarizeIdRef.current !== requestId) return;
      if (!streamedRaw.trim()) {
        setSummaryContent(null);
      } else {
        setSummaryContent(streamedRaw);
      }
      setAiError(String(e));
    } finally {
      if (rafId != null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
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

      activeTranslateIdRef.current = null;
      activeSummarizeIdRef.current = null;
    };
  }, []);

  useEffect(() => {
    const loadContent = async () => {
      setLoading(true);
      setLoadError(null);
      try {
        const latestContent = await onRead(skillName);
        setContent(latestContent);
        setEditedContent(latestContent.content);
        setTranslatedContent(null);
        setTranslatedSource(null);
        setTranslationVisible(false);
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
    if (translationVisible) setTranslationVisible(false);
    setTranslationHasDelta(false);
    setTranslationWasNonStreaming(false);
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
          <Button
            size="sm"
            variant="outline"
            onClick={onClose}
          >
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
              <button
                onClick={() => {
                  if (loadError) return;
                  if (!aiConfigured) {
                    navigateToAiSettings();
                    return;
                  }
                  void handleTranslate();
                }}
                disabled={!!loadError}
                className={`flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
                  translating
                    ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
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
                  : translatedContent && translatedSource === editedContent
                  ? t("skillEditor.showTranslation")
                  : t("skillEditor.translate")}
              </button>
              {translatedContent && translatedSource === editedContent && (
                <button
                  onClick={() => {
                    if (loadError) return;
                    if (!aiConfigured) {
                      navigateToAiSettings();
                      return;
                    }
                    void handleAiRetranslate();
                  }}
                  disabled={!!loadError || translating}
                  className={`flex items-center gap-1 px-2 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer ${
                    translating && retranslating
                      ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
                      : aiConfigured && !loadError
                      ? "text-muted-foreground hover:text-foreground hover:bg-card-hover"
                      : loadError
                      ? "text-muted-foreground/50 cursor-not-allowed"
                      : "text-primary/80 bg-primary/5 border border-primary/20 hover:bg-primary/10"
                  }`}
                  title={t("skillEditor.retranslateWithAi")}
                >
                  {translating && retranslating ? (
                    <Square className="w-3 h-3 fill-current" />
                  ) : (
                    <Sparkles className="w-3 h-3" />
                  )}
                  {translating && retranslating
                    ? t("skillEditor.retranslatingWithAi")
                    : t("skillEditor.retranslateWithAi")}
                </button>
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
                      : summaryContent
                      ? "Hide summary"
                      : "AI quick summary"
                    : "AI not configured (optional). Editing is still available."
                }
              >
                {summarizing ? (
                  <Square className="w-3 h-3 fill-current" />
                ) : (
                  <Sparkles className="w-3 h-3" />
                )}
                {summarizing ? t("common.cancel") : summaryContent ? t("skillEditor.hideSummary") : t("skillEditor.summary")}
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
          {localizedAiError && (
            <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20 flex items-center gap-2">
              <span className="text-xs text-destructive flex-1">{localizedAiError}</span>
              <button
                onClick={() => setAiError(null)}
                className="text-destructive/60 hover:text-destructive cursor-pointer p-1.5 rounded focus-ring"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          )}

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

          <div className="markdown-content flex-1 p-4 overflow-y-auto prose prose-sm dark:prose-invert max-w-none">
            {/* AI Summary Card */}
            {summaryContent !== null && (
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

            {/* Main content: translated or original */}
            {loadError ? (
              <div className="text-sm text-muted-foreground">{t("skillEditor.noContent")}</div>
            ) : previewContent.trim().length === 0 && previewFrontmatterEntries.length === 0 ? (
              <div className="text-sm text-muted-foreground">{t("skillEditor.noContent")}</div>
            ) : (
              <Markdown streaming={translating && translationVisible} fallback={<div className="text-sm text-muted-foreground">Loading preview...</div>}>
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
                  if (translationVisible) setTranslationVisible(false);
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
