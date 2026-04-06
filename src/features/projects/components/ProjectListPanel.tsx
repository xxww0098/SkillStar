import { FolderOpen, Plus, Trash2 } from "lucide-react";
import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { ProjectEntry } from "../../../types";

interface ProjectListPanelProps {
  filteredProjects: ProjectEntry[];
  selectedProject: ProjectEntry | null;
  projectFilter: string;
  onSelectProject: (project: ProjectEntry) => void;
  onRemoveProject: (event: MouseEvent, name: string) => void;
  onOpenFolder: () => void;
}

export function ProjectListPanel({
  filteredProjects,
  selectedProject,
  projectFilter,
  onSelectProject,
  onRemoveProject,
  onOpenFolder,
}: ProjectListPanelProps) {
  const { t } = useTranslation();

  return (
    <div className="w-72 min-w-[288px] border-r border-border flex flex-col bg-sidebar/50 pt-3">
      {" "}
      <div className="flex-1 overflow-y-auto px-3 space-y-1">
        {filteredProjects.map((project) => (
          <CardTemplate
            key={project.name}
            onClick={() => onSelectProject(project)}
            className={cn(
              "group !h-auto cursor-pointer rounded-xl hover:-translate-y-0",
              selectedProject?.name === project.name
                ? "bg-primary/5 border-primary/20 shadow-sm"
                : "hover:bg-muted border-transparent",
            )}
            role="button"
            tabIndex={0}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelectProject(project);
              }
            }}
            bodyClassName="p-0"
            body={
              <div className="flex items-center gap-3 px-3 py-2.5">
                <div
                  className={cn(
                    "w-8 h-8 rounded-lg flex items-center justify-center shrink-0 transition-colors",
                    selectedProject?.name === project.name ? "bg-primary/10" : "bg-muted",
                  )}
                >
                  <FolderOpen
                    className={cn(
                      "w-4 h-4",
                      selectedProject?.name === project.name ? "text-primary" : "text-muted-foreground",
                    )}
                  />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium truncate">{project.name}</div>
                  <div className="text-micro text-muted-foreground truncate font-mono">{project.path}</div>
                </div>
                <div className="shrink-0">
                  <button
                    onClick={(event) => onRemoveProject(event, project.name)}
                    className="p-1.5 rounded-md opacity-0 group-hover:opacity-100 hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition cursor-pointer"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            }
          />
        ))}

        {filteredProjects.length === 0 && !projectFilter && (
          <div className="text-center py-10 px-4">
            <div className="w-10 h-10 rounded-xl bg-primary/10 flex items-center justify-center mx-auto mb-3">
              <FolderOpen className="w-5 h-5 text-primary" />
            </div>
            <p className="text-sm font-medium mb-1">{t("projects.emptyTitle")}</p>
            <p className="text-xs text-muted-foreground mb-4">{t("projects.emptyDesc")}</p>
            <Button variant="outline" size="sm" onClick={onOpenFolder}>
              <Plus className="w-3.5 h-3.5" />
              {t("projects.registerProject")}
            </Button>
          </div>
        )}
        {filteredProjects.length === 0 && projectFilter && (
          <div className="text-center py-8">
            <p className="text-xs text-muted-foreground">{t("projects.noMatching")}</p>
          </div>
        )}
      </div>
    </div>
  );
}
