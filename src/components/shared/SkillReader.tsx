import { Edit3, Eye, FileText, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { normalizeSkillMarkdownForPreview, parseFrontmatterEntries, splitFrontmatter } from "../../lib/frontmatter";
import { Button } from "../ui/button";
import { Markdown } from "../ui/Markdown";
import { ResizablePanel } from "../ui/ResizablePanel";

interface SkillReaderProps {
  skillName: string;
  content: string;
  onClose: () => void;
  /** Optional edit affordance: switch to the editor for installed skills. */
  onEdit?: () => void;
}

export function SkillReader({ skillName, content, onClose, onEdit }: SkillReaderProps) {
  const { t } = useTranslation();

  const previewSource = normalizeSkillMarkdownForPreview(content);
  const previewFrontmatterEntries = parseFrontmatterEntries(splitFrontmatter(previewSource).frontmatter);
  const previewContent = splitFrontmatter(previewSource).body;

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
          {onEdit && (
            <Button
              size="sm"
              variant="outline"
              onClick={onEdit}
              title={t("detailPanel.editSkillMd")}
              aria-label={t("detailPanel.editSkillMd")}
            >
              <Edit3 className="w-4 h-4" />
            </Button>
          )}
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
      </div>

      {/* Content */}
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
