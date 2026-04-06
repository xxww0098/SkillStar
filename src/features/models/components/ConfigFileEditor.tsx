import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, Braces, FileCode2, Loader2, RotateCcw, Save, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "../../../components/ui/button";
import { ResizablePanel } from "../../../components/ui/ResizablePanel";

export type ConfigFileKey = "claude" | "codex_config" | "opencode";

interface ConfigFileEditorProps {
  /** Which config file to edit */
  fileKey: ConfigFileKey;
  /** Display title */
  title: string;
  /** File path hint */
  filePath: string;
  /** Called when the editor is closed */
  onClose: () => void;
}

/**
 * Side-drawer config file editor, reusing the same ResizablePanel pattern
 * as the SkillEditor. Loads raw file text, edits in a monospace textarea,
 * and saves back via Tauri commands.
 *
 * Features colored bracket matching to help users spot missing brackets.
 */
function renderHighlightedText(text: string) {
  const colors = [
    "text-blue-400 font-bold",
    "text-yellow-400 font-bold",
    "text-purple-400 font-bold",
    "text-green-400 font-bold",
    "text-pink-400 font-bold",
    "text-cyan-400 font-bold",
  ];

  // First pass: assign a unique pair index to each matched open/close bracket
  const bracketColorMap = new Map<number, number>(); // char position → color index
  const stack: number[] = []; // stack of char positions for open brackets
  let pairCounter = 0;

  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (char === "{" || char === "[" || char === "(") {
      stack.push(i);
    } else if (char === "}" || char === "]" || char === ")") {
      if (stack.length > 0) {
        const openPos = stack.pop()!;
        const colorIndex = pairCounter % colors.length;
        bracketColorMap.set(openPos, colorIndex);
        bracketColorMap.set(i, colorIndex);
        pairCounter++;
      }
      // Unmatched close brackets: not in map → will render red
    }
  }
  // Remaining items in stack are unmatched open brackets → not in map → render red

  // Second pass: render with colors from the map
  const result: React.ReactNode[] = [];
  let currentChunk = "";

  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (char === "{" || char === "[" || char === "(" || char === "}" || char === "]" || char === ")") {
      if (currentChunk) {
        result.push(currentChunk);
        currentChunk = "";
      }
      const colorIndex = bracketColorMap.get(i);
      if (colorIndex !== undefined) {
        result.push(
          <span key={i} className={colors[colorIndex]}>
            {char}
          </span>,
        );
      } else {
        // Unmatched bracket
        result.push(
          <span key={i} className="text-red-500 font-bold underline bg-red-500/20" title="Unmatched bracket">
            {char}
          </span>,
        );
      }
    } else {
      currentChunk += char;
    }
  }
  if (currentChunk) {
    result.push(currentChunk);
  }

  // Ensure trailing newline renders correctly in the container
  if (text.endsWith("\n")) {
    result.push(<br key="trailing" />);
  }

  return result;
}
export function ConfigFileEditor({ fileKey, title, filePath, onClose }: ConfigFileEditorProps) {
  const [content, setContent] = useState("");
  const [originalContent, setOriginalContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const bgRef = useRef<HTMLDivElement>(null);
  const textAreaRef = useRef<HTMLTextAreaElement>(null);

  const hasChanges = content !== originalContent;
  const hasChangesRef = useRef(false);
  hasChangesRef.current = hasChanges;

  const handleScroll = (e: React.UIEvent<HTMLTextAreaElement>) => {
    if (bgRef.current) {
      bgRef.current.scrollTop = e.currentTarget.scrollTop;
      bgRef.current.scrollLeft = e.currentTarget.scrollLeft;
    }
  };

  const load = useCallback(
    async (isSilent = false) => {
      if (!isSilent) setLoading(true);
      setError(null);
      try {
        const text = await invoke<string>("read_model_config_text", { fileKey });
        setContent(text);
        setOriginalContent(text);
      } catch (e) {
        setError(String(e));
      } finally {
        if (!isSilent) setLoading(false);
      }
    },
    [fileKey],
  );

  useEffect(() => {
    load(false);
    const onExternalChange = () => {
      // Only reload if the user hasn't made local uncommitted edits
      if (!hasChangesRef.current) {
        load(true);
      }
    };
    window.addEventListener("skillstar_config_changed", onExternalChange);
    return () => window.removeEventListener("skillstar_config_changed", onExternalChange);
  }, [load]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await invoke("write_model_config_text", { fileKey, content });
      setOriginalContent(content);
      toast.success(`${title} 已保存`);
      window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
    } catch (e) {
      toast.error(`保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    setContent(originalContent);
  };

  const isToml = fileKey.includes("toml") || fileKey === "codex_config";

  const handleFormat = async () => {
    try {
      const formatted = await invoke<string>("format_model_config_text", {
        content,
        isToml,
      });
      setContent(formatted);
      toast.success("已格式化");
    } catch (e) {
      toast.error(typeof e === "string" ? e : isToml ? "TOML 格式无效" : "JSON 格式无效");
    }
  };

  if (loading) {
    return (
      <ResizablePanel defaultWidth={600} storageKey="config-editor-width">
        <div className="flex-1 flex items-center justify-center">
          <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
        </div>
      </ResizablePanel>
    );
  }

  return (
    <ResizablePanel defaultWidth={640} storageKey="config-editor-width">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-border shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          <FileCode2 className="w-4 h-4 text-primary shrink-0" />
          <h2 className="text-sm font-semibold truncate">{title}</h2>
          {hasChanges && (
            <span className="text-[10px] text-amber-500 px-1.5 py-0.5 bg-amber-500/10 rounded shrink-0">未保存</span>
          )}
        </div>
        <Button size="sm" variant="outline" onClick={onClose} className="cursor-pointer shrink-0">
          <X className="w-4 h-4" />
        </Button>
      </div>

      {/* File path hint */}
      <div className="px-4 py-2 border-b border-border bg-muted/20">
        <span className="text-[10px] text-muted-foreground/60 font-mono">{filePath}</span>
      </div>

      {/* Error state */}
      {error && (
        <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20">
          <div className="flex items-start gap-1.5 text-xs text-destructive">
            <AlertCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            <span>{error}</span>
          </div>
        </div>
      )}

      {/* Editor */}
      <div className="flex-1 flex flex-col relative overflow-hidden bg-input/50 backdrop-blur-sm">
        {/* Syntax highlight layer */}
        <div
          ref={bgRef}
          className="absolute inset-0 p-4 text-[13px] font-mono leading-relaxed whitespace-pre-wrap break-words pointer-events-none overflow-hidden text-foreground/90 font-medium"
          aria-hidden="true"
        >
          {renderHighlightedText(content)}
        </div>

        {/* Editor layer */}
        <textarea
          ref={textAreaRef}
          className="absolute inset-0 w-full h-full p-4 text-[13px] border-0 resize-none focus:outline-none font-mono leading-relaxed bg-transparent text-transparent caret-foreground selection:bg-primary/30"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onScroll={handleScroll}
          spellCheck={false}
          data-gramm="false"
        />
      </div>

      {/* Footer */}
      <div className="flex items-center justify-end gap-2 p-4 border-t border-border shrink-0 bg-card/50 backdrop-blur-sm">
        <div className="mr-auto flex items-center gap-2">
          <Button variant="outline" size="sm" className="cursor-pointer" onClick={handleFormat} title="格式化">
            <Braces className="w-3.5 h-3.5 mr-1.5" />
            格式化
          </Button>
          {hasChanges && (
            <Button variant="destructive" size="sm" className="cursor-pointer" onClick={handleReset} title="放弃修改">
              <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
              重置
            </Button>
          )}
        </div>
        <Button variant="outline" onClick={onClose} className="cursor-pointer">
          关闭
        </Button>
        <Button onClick={handleSave} disabled={!hasChanges || saving} className="cursor-pointer">
          {saving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Save className="w-4 h-4 mr-2" />}
          {saving ? "保存中..." : "保存修改"}
        </Button>
      </div>
    </ResizablePanel>
  );
}
