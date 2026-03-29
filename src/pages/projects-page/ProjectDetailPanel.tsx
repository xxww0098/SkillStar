import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { FolderKanban, FolderSync } from "lucide-react";
import { Button } from "../../components/ui/button";
import type { AgentProfile, ProjectEntry, ScannedSkill, Skill } from "../../types";
import { ScanImportBanner } from "./ScanImportBanner";
import { AgentAccordion } from "./AgentAccordion";
import { ApplyFooter } from "./ApplyFooter";

interface ImportDone {
  hub: number;
  links: number;
}

interface ProjectDetailPanelProps {
  selectedProject: ProjectEntry | null;
  onRelinkPath: () => void;
  unmanagedSkills: ScannedSkill[];
  scanExpanded: boolean;
  importing: boolean;
  importDone: ImportDone | null;
  enabledProfilesById: Map<string, AgentProfile>;
  enabledProfiles: AgentProfile[];
  enabledAgents: string[];
  expandedAgent: string | null;
  agentSkills: Record<string, string[]>;
  skillFilter: string;
  totalSkills: number;
  syncResult: number | null;
  saving: boolean;
  dirty: boolean;
  getAvailableSkills: (agentId: string) => Skill[];
  onToggleScanExpanded: () => void;
  onImportAll: () => void;
  onToggleExpand: (agentId: string) => void;
  onToggleAgent: (agentId: string) => void;
  onNavigateToSkill?: (skillName: string) => void;
  onRemoveSkill: (agentId: string, skillName: string) => void;
  onSkillFilterChange: (value: string) => void;
  onAddSkill: (agentId: string, skillName: string) => void;
  onApply: () => void;
}

export function ProjectDetailPanel({
  selectedProject,
  onRelinkPath,
  unmanagedSkills,
  scanExpanded,
  importing,
  importDone,
  enabledProfilesById,
  enabledProfiles,
  enabledAgents,
  expandedAgent,
  agentSkills,
  skillFilter,
  totalSkills,
  syncResult,
  saving,
  dirty,
  getAvailableSkills,
  onToggleScanExpanded,
  onImportAll,
  onToggleExpand,
  onToggleAgent,
  onNavigateToSkill,
  onRemoveSkill,
  onSkillFilterChange,
  onAddSkill,
  onApply,
}: ProjectDetailPanelProps) {
  const { t } = useTranslation();

  if (!selectedProject) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.3 }}
          className="text-center"
        >
          <div className="w-16 h-16 rounded-2xl bg-primary/5 border border-primary/10 flex items-center justify-center mx-auto mb-4">
            <FolderKanban className="w-8 h-8 text-primary/30" />
          </div>
          <h3 className="text-heading-sm mb-1">{t("projects.selectProjectTitle")}</h3>
          <p className="text-caption max-w-xs">{t("projects.selectProjectDesc")}</p>
        </motion.div>
      </div>
    );
  }

  return (
    <>
      <div className="px-6 py-4 border-b border-border-subtle shrink-0">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-primary/5 border border-primary/10 flex items-center justify-center">
            <FolderKanban className="w-5 h-5 text-primary/60" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-heading-sm truncate">{selectedProject.name}</h2>
            <p className="text-caption font-mono truncate">{selectedProject.path}</p>
          </div>
          <Button variant="outline" size="sm" onClick={onRelinkPath}>
            <FolderSync className="w-3.5 h-3.5 mr-1.5" />
            {t("projects.changePath")}
          </Button>
        </div>
      </div>

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15 }}
        className="flex-1 overflow-y-auto px-6 py-5 space-y-5"
      >
        <ScanImportBanner
          unmanagedSkills={unmanagedSkills}
          scanExpanded={scanExpanded}
          importing={importing}
          importDone={importDone}
          enabledProfilesById={enabledProfilesById}
          onToggleScanExpanded={onToggleScanExpanded}
          onImportAll={onImportAll}
        />

        <AgentAccordion
          enabledProfiles={enabledProfiles}
          enabledAgents={enabledAgents}
          expandedAgent={expandedAgent}
          agentSkills={agentSkills}
          skillFilter={skillFilter}
          getAvailableSkills={getAvailableSkills}
          onToggleExpand={onToggleExpand}
          onToggleAgent={onToggleAgent}
          onNavigateToSkill={onNavigateToSkill}
          onRemoveSkill={onRemoveSkill}
          onSkillFilterChange={onSkillFilterChange}
          onAddSkill={onAddSkill}
        />
      </motion.div>

      <ApplyFooter
        totalSkills={totalSkills}
        enabledAgentsCount={enabledAgents.length}
        syncResult={syncResult}
        saving={saving}
        dirty={dirty}
        onApply={onApply}
      />
    </>
  );
}
