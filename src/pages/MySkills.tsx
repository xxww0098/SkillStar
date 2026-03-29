import { useState, useMemo, useCallback, useEffect, lazy, Suspense } from "react";
import { motion } from "framer-motion";
import { Layers } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Toolbar } from "../components/layout/Toolbar";
import { SkillGrid } from "../components/skills/SkillGrid";
import { DeployToProjectModal } from "../components/skills/DeployToProjectModal";
import { CreateGroupModal } from "../components/skills/CreateGroupModal";
import { SkillSelectionBar } from "../components/skills/SkillSelectionBar";
import { UninstallConfirmDialog } from "../components/skills/UninstallConfirmDialog";
import { ImportModal } from "../components/skills/ImportModal";
import { PublishSkillModal } from "../components/skills/PublishSkillModal";
import { ImportBundleModal } from "../components/skills/ImportBundleModal";
import { AiPickSkillsModal } from "../components/skills/AiPickSkillsModal";
import { useSkills } from "../hooks/useSkills";
import { useSkillCards } from "../hooks/useSkillCards";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { toast } from "../lib/toast";
import type { Skill, SortOption, ViewMode } from "../types";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  }))
);

interface MySkillsProps {
  initialFocusSkill?: string | null;
  onClearFocus?: () => void;
  onPackSkills?: (skills: string[]) => void;
}

export function MySkills({
  initialFocusSkill,
  onClearFocus,
  onPackSkills,
}: MySkillsProps = {}) {
  const { t } = useTranslation();
  const {
    skills,
    loading,
    refresh,
    installSkill,
    uninstallSkill,
    updateSkill,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
  } = useSkills();
  const { profiles, deploySkillsToProject } = useAgentProfiles();
  const { createGroup, groups } = useSkillCards();
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("name");
  const [showUpdateOnly, setShowUpdateOnly] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [agentFilter, setAgentFilter] = useState<string | null>(null);
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [selectedSkillNames, setSelectedSkillNames] = useState<Set<string>>(new Set());
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [deployModalOpen, setDeployModalOpen] = useState(false);
  const [groupModalOpen, setGroupModalOpen] = useState(false);
  const [uninstallDialogOpen, setUninstallDialogOpen] = useState(false);
  const [pendingUninstallNames, setPendingUninstallNames] = useState<string[]>([]);
  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallError, setUninstallError] = useState<string | null>(null);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importBundleOpen, setImportBundleOpen] = useState(false);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const [aiPickModalOpen, setAiPickModalOpen] = useState(false);
  const pendingUpdateCount = useMemo(
    () => skills.filter((skill) => skill.update_available).length,
    [skills]
  );

  // Auto-focus a skill when navigating from Projects page
  useEffect(() => {
    if (initialFocusSkill && skills.length > 0) {
      const skill = skills.find((s) => s.name === initialFocusSkill);
      if (skill) setSelectedSkill(skill);
      onClearFocus?.();
    }
  }, [initialFocusSkill, skills, onClearFocus]);

  const filtered = useMemo(() => {
    let list = [...skills];

    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.description.toLowerCase().includes(q)
      );
    }

    // Agent filter: only show skills linked to the selected agent
    if (agentFilter) {
      const agentProfile = profiles.find((p) => p.id === agentFilter);
      if (agentProfile) {
        list = list.filter(
          (s) => s.agent_links?.includes(agentProfile.display_name)
        );
      }
    }

    if (showUpdateOnly) {
      list = list.filter((s) => s.update_available);
    }

    list.sort((a, b) => {
      switch (sortBy) {
        case "stars-desc":
          return b.stars - a.stars || a.name.localeCompare(b.name);
        case "updated":
          return (
            new Date(b.last_updated).getTime() - new Date(a.last_updated).getTime() ||
            a.name.localeCompare(b.name)
          );
        case "name":
        default:
          return a.name.localeCompare(b.name);
      }
    });

    return list;
  }, [skills, searchQuery, sortBy, agentFilter, profiles, showUpdateOnly]);

  const handleInstall = async (url: string) => {
    try {
      await installSkill(url);
    } catch (e) {
      toast.error(t("mySkills.installFailed"));
    }
  };

  const handleUpdate = async (name: string) => {
    try {
      const updated = await updateSkill(name);
      if (selectedSkill?.name === name) {
        setSelectedSkill(updated);
      }
    } catch (e) {
      toast.error(t("mySkills.updateFailed"));
    }
  };

  const handleSelectSkill = useCallback((name: string) => {
    setSelectedSkillNames((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  }, []);

  const clearSelection = () => setSelectedSkillNames(new Set());

  const hasSelection = selectedSkillNames.size > 0;
  const [batchLoading, setBatchLoading] = useState(false);

  const removeSkillFromUi = useCallback((name: string) => {
    setSelectedSkill((current) => (current?.name === name ? null : current));
    setSelectedSkillNames((prev) => {
      const next = new Set(prev);
      next.delete(name);
      return next;
    });
  }, []);

  const openUninstallDialog = useCallback((names: Iterable<string>) => {
    const nextNames = Array.from(new Set(names));
    if (nextNames.length === 0) return;
    setPendingUninstallNames(nextNames);
    setUninstallError(null);
    setUninstallDialogOpen(true);
  }, []);

  const closeUninstallDialog = useCallback(() => {
    if (uninstalling) return;
    setPendingUninstallNames([]);
    setUninstallError(null);
    setUninstallDialogOpen(false);
  }, [uninstalling]);

  const handleUninstall = useCallback((name: string) => {
    openUninstallDialog([name]);
  }, [openUninstallDialog]);

  const handleBatchUninstall = useCallback(() => {
    openUninstallDialog(selectedSkillNames);
  }, [openUninstallDialog, selectedSkillNames]);

  const confirmUninstall = useCallback(async () => {
    if (pendingUninstallNames.length === 0) return;

    setUninstalling(true);
    const failedNames: string[] = [];

    for (const name of pendingUninstallNames) {
      try {
        await uninstallSkill(name);
        removeSkillFromUi(name);
      } catch (e) {
        failedNames.push(name);
        toast.error(t("mySkills.batchUninstallFailed", { name, count: 1 }));
      }
    }

    setUninstalling(false);

    if (failedNames.length === 0) {
      closeUninstallDialog();
      return;
    }

    setPendingUninstallNames(failedNames);
    setUninstallError(
      failedNames.length === 1
        ? t("mySkills.batchUninstallFailed", { name: failedNames[0], count: 1 })
        : t("mySkills.batchUninstallFailed", { name: failedNames[0], count: failedNames.length })
    );
  }, [closeUninstallDialog, pendingUninstallNames, removeSkillFromUi, uninstallSkill]);

  const handleBatchUpdate = async () => {
    setBatchLoading(true);
    for (const name of selectedSkillNames) {
      try {
        await updateSkill(name);
      } catch (e) {
        toast.error(t("mySkills.updateFailed"));
      }
    }
    clearSelection();
    setBatchLoading(false);
  };

  const handleBatchLink = useCallback(async (agentId: string) => {
    setBatchLoading(true);
    try {
      await invoke<number>("batch_link_skills_to_agent", {
        skillNames: Array.from(selectedSkillNames),
        agentId,
      });
      // Refresh skills list to update agent_links
      clearSelection();
    } catch (e) {
      toast.error(t("mySkills.batchLinkFailed"));
    } finally {
      setBatchLoading(false);
    }
  }, [selectedSkillNames, clearSelection]);

  return (
    <div className="flex-1 flex overflow-hidden relative">
      <div className="flex-1 flex flex-col overflow-hidden">
        <Toolbar
          titleNode={<h1 className="text-heading-md text-zinc-100">{t("mySkills.title")}</h1>}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          countText={
            <div className="flex items-center gap-1.5 font-medium">
              <Layers className="w-3 h-3 hover:text-muted-foreground/90 transition-colors" />
              <span>{filtered.length}</span>
            </div>
          }
          showUpdateOnly={showUpdateOnly}
          onToggleUpdateOnly={() => setShowUpdateOnly((prev) => !prev)}
          pendingUpdateCount={pendingUpdateCount}
          hideStarsSort={true}
          agentProfiles={profiles}
          agentFilter={agentFilter}
          onAgentFilterChange={setAgentFilter}
          onImport={() => setImportModalOpen(true)}
          onRefresh={() => refresh(false, true)}
          isRefreshing={loading}
          onAiPick={() => setAiPickModalOpen(true)}
        />

        {/* Selection bar */}
        {hasSelection && (
          <SkillSelectionBar
            selectedCount={selectedSkillNames.size}
            totalCount={filtered.length} // Filtered skills count so Select All maps to the current list
            disabled={batchLoading || uninstalling}
            onDeploy={() => setDeployModalOpen(true)}
            onSaveGroup={() => setGroupModalOpen(true)}
            onPackSkills={
              onPackSkills
                ? () => onPackSkills(Array.from(selectedSkillNames))
                : undefined
            }
            onUpdate={handleBatchUpdate}
            onUninstall={handleBatchUninstall}
            onSelectAll={() => setSelectedSkillNames(new Set(filtered.map(s => s.name)))}
            onClear={clearSelection}
            agentProfiles={profiles}
            onBatchLink={handleBatchLink}
          />
        )}

        <motion.main
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="flex-1 overflow-y-auto p-6 bg-gradient-to-br from-transparent via-card/10 to-transparent"
        >
          {loading ? (
            <div className="flex items-center justify-center py-20 text-zinc-500 text-sm">
              {t("mySkills.loading")}
            </div>
          ) : (
            <SkillGrid
              skills={filtered}
              viewMode={viewMode}
              onSkillClick={(skill) => setSelectedSkill(prev => prev?.name === skill.name ? null : skill)}
              onInstall={handleInstall}
              onUpdate={handleUpdate}
              emptyMessage={t("mySkills.empty")}
              selectable
              selectedSkills={selectedSkillNames}
              onSelectSkill={handleSelectSkill}
              profiles={profiles.filter((p) => p.enabled)}
              onToggleAgent={toggleSkillForAgent}
              pendingAgentToggleKeys={pendingAgentToggleKeys}
            />
          )}
        </motion.main>
      </div>

      {selectedSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-white/10 bg-card/60 backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center text-sm text-zinc-400">
              Loading details...
            </div>
          }
        >
          <DetailPanel
            skill={selectedSkill}
            onClose={() => setSelectedSkill(null)}
            onInstall={handleInstall}
            onUpdate={handleUpdate}
            onUninstall={handleUninstall}
            uninstalling={
              uninstalling && pendingUninstallNames.includes(selectedSkill.name)
            }
            onReadContent={readSkillContent}
            onSaveContent={updateSkillContent}
            onPublish={(name) => setPublishTarget(name)}
            onExportBundle={async (name) => {
              try {
                const outputDir = await save({
                  defaultPath: `${name}.agentskill`,
                  filters: [
                    { name: "AgentSkill Bundle", extensions: ["agentskill"] },
                  ],
                });
                if (!outputDir) return;
                // Extract directory from the full save path
                const dir = outputDir.replace(/\/[^/]+$/, "");
                await invoke<string>("export_skill_bundle", {
                  name,
                  outputDir: dir,
                });
                toast.success(t("detailPanel.exportSuccess"));
              } catch (e) {
                toast.error(String(e));
              }
            }}
          />
        </Suspense>
      )}

      <DeployToProjectModal
        open={deployModalOpen}
        onClose={() => setDeployModalOpen(false)}
        selectedSkills={Array.from(selectedSkillNames)}
        profiles={profiles.filter((p) => p.enabled)}
        onDeploy={deploySkillsToProject}
      />

      <CreateGroupModal
        open={groupModalOpen}
        onClose={() => {
          setGroupModalOpen(false);
          setQuickPackSkills([]);
        }}
        availableSkills={skills}
        existingNames={groups.map((g) => g.name)}
        initialSkills={quickPackSkills.length > 0 ? quickPackSkills : Array.from(selectedSkillNames)}
        onSave={async (name, description, icon, skillList) => {
          await createGroup(name, description, icon, skillList);
          clearSelection();
          setQuickPackSkills([]);
        }}
      />

      <UninstallConfirmDialog
        open={uninstallDialogOpen}
        skillNames={pendingUninstallNames}
        uninstalling={uninstalling}
        error={uninstallError}
        onClose={closeUninstallDialog}
        onConfirm={confirmUninstall}
      />

      <ImportModal
        open={importModalOpen}
        onClose={() => setImportModalOpen(false)}
        onInstalled={() => {
          void refresh(false, true);
        }}
        onPickLocalFile={() => {
          setImportModalOpen(false);
          setImportBundleOpen(true);
        }}
        onPackGroup={(names: string[]) => {
          setImportModalOpen(false);
          setQuickPackSkills(names);
          setGroupModalOpen(true);
        }}
      />

      <ImportBundleModal
        open={importBundleOpen}
        onClose={() => setImportBundleOpen(false)}
        onImported={() => {
          void refresh(false, true);
        }}
      />

      <PublishSkillModal
        open={!!publishTarget}
        onClose={() => setPublishTarget(null)}
        skillName={publishTarget || ""}
        skillDescription={
          skills.find((s) => s.name === publishTarget)?.description || ""
        }
        onPublished={() => {
          setPublishTarget(null);
          refresh(false, true);
        }}
      />

      <AiPickSkillsModal
        open={aiPickModalOpen}
        onClose={() => setAiPickModalOpen(false)}
        skills={skills}
        onResult={(names) => {
          setSelectedSkillNames(new Set(names));
        }}
      />
    </div>
  );
}
