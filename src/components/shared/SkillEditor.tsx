import { Eye, FileText, PanelLeftClose, PanelLeftOpen, RotateCcw, Save, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { normalizeSkillMarkdownForPreview, parseFrontmatterEntries, splitFrontmatter } from "../../lib/frontmatter";
import type { SkillContent } from "../../types";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";

interface SkillEditorProps {
  skillName: string;
  /** Header X: dismiss the whole detail drawer. */
  onClose: () => void;
  /** Footer cancel: leave the editor, back to the detail view. */
  onCancel: () => void;
  onRead: (name: string) => Promise<SkillContent>;
  onSave: (name: string, content: string) => Promise<void>;
}
export function SkillEditor({ skillName, onClose, onCancel, onRead, onSave }: SkillEditorProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState<SkillContent | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [editedContent, setEditedContent] = useState("");
  const [hasChanges, setHasChanges] = useState(false);
  const [isLeftPaneOpen, setIsLeftPaneOpen] = useState(false);

  const previewSource = normalizeSkillMarkdownForPreview(editedContent);
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

  useEffect(() => {
    const loadContent = async () => {
      setLoading(true);
      setLoadError(null);
      try {
        const latestContent = await onRead(skillName);
        setContent(latestContent);
        setEditedContent(latestContent.content);
      } catch (e) {
        if (import.meta.env.DEV) console.error("Failed to load skill content:", e);
        setContent(null);
        setEditedContent("");
        setHasChanges(false);
        setLoadError(String(e));
      } finally {
        setLoading(false);
      }
    };
    loadContent();
  }, [onRead, skillName]);

  const handleSave = async () => {
    if (!content) return;
    setSaving(true);
    try {
      await onSave(skillName, editedContent);
      setHasChanges(false);
      const latestContent = await onRead(skillName);
      setContent(latestContent);
    } catch (e) {
      if (import.meta.env.DEV) console.error("Failed to save:", e);
    } finally {
      setSaving(false);
    }
  };

  const handleContentChange = (value: string) => {
    setEditedContent(value);
    setHasChanges(value !== content?.content);
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
              type="button"
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
          </div>

          {loadError && (
            <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20">
              <div className="text-xs font-medium text-destructive">{t("skillEditor.loadFailed")}</div>
              <div className="text-xs text-destructive/90 break-words mt-0.5">{loadError}</div>
            </div>
          )}

          <div className="markdown-content flex-1 p-4 overflow-y-auto overscroll-y-contain prose prose-sm dark:prose-invert max-w-none">
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

            {/* Main SKILL.md content */}
            {loadError ? (
              <div className="text-sm text-muted-foreground">{t("skillEditor.noContent")}</div>
            ) : previewContent.trim().length === 0 && previewFrontmatterEntries.length === 0 ? (
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
                }
              }}
              title="Discard unsaved changes"
            >
              <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
              {t("skillEditor.reset")}
            </Button>
          )}
        </div>
        <Button variant="outline" onClick={onCancel} className="cursor-pointer">
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
