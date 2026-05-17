import { motion } from "framer-motion";
import { Check, Download, GitBranch, Package, RotateCcw, ScanSearch } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { SearchInput } from "../../../../components/ui/SearchInput";
import { SelectAllButton } from "../../../../components/ui/SelectAllButton";
import { cn } from "../../../../lib/utils";
import type { DiscoveredSkill } from "../../../../types";

export interface SelectSkillsPhaseProps {
  skills: DiscoveredSkill[];
  source: string;
  selectedSkills: Set<string>;
  onToggle: (id: string) => void;
  onSelectAll: (ids?: string[]) => void;
  onDeselectAll: (ids?: string[]) => void;
  onInstall: (pack?: boolean) => void;
  fullDepthEnabled: boolean;
  onDeepScan: () => void;
  hasPackGroup?: boolean;
}

export function SelectSkillsPhase({
  skills,
  source,
  selectedSkills,
  onToggle,
  onSelectAll,
  onDeselectAll,
  onInstall,
  fullDepthEnabled,
  onDeepScan,
  hasPackGroup,
}: SelectSkillsPhaseProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState("");

  const filteredSkills = skills.filter(
    (s) =>
      s.id.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (s.description?.toLowerCase() || "").includes(searchQuery.toLowerCase()),
  );

  const installableFiltered = filteredSkills.filter((s) => !s.already_installed);
  const installableCount = installableFiltered.length;
  const allSelected =
    filteredSkills.length > 0 &&
    (installableCount > 0
      ? installableFiltered.every((s) => selectedSkills.has(s.id))
      : filteredSkills.every((s) => selectedSkills.has(s.id)));

  const handleSelectAll = () => {
    if (allSelected) {
      onDeselectAll(filteredSkills.map((s) => s.id));
    } else {
      const targets = installableCount > 0 ? installableFiltered : filteredSkills;
      onSelectAll(targets.map((s) => s.id));
    }
  };

  return (
    <div className="flex flex-col">
      {/* Source info */}
      <div className="px-6 pt-4 pb-2 space-y-3 shrink-0">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <GitBranch className="w-3.5 h-3.5 text-muted-foreground" />
            <span className="text-xs text-muted-foreground font-medium">{source}</span>
            <span className="text-micro bg-muted px-1.5 py-0.5 rounded-md text-muted-foreground/80">
              {skills.length} skill{skills.length !== 1 ? "s" : ""}
            </span>
          </div>
          <div className="flex items-center gap-3">
            {hasPackGroup && selectedSkills.size > 0 && (
              <button
                type="button"
                onClick={() => onInstall(true)}
                className="text-xs text-amber-500 bg-amber-500/10 hover:bg-amber-500/20 px-2 py-1 rounded-md transition-colors flex items-center gap-1 cursor-pointer font-medium whitespace-nowrap"
              >
                <Package className="w-3.5 h-3.5" />
                {t("githubImportModal.quickPack")}
              </button>
            )}
            <SelectAllButton
              allSelected={allSelected}
              onToggle={handleSelectAll}
              variant="ghost"
              size="sm"
              className="h-auto p-0 text-primary hover:text-primary/80 hover:bg-transparent"
            />
          </div>
        </div>

        <SearchInput
          containerClassName="mt-3"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder={t("common.search")}
          className="h-8 text-xs rounded-lg border-border/80 bg-background/50 placeholder:text-muted-foreground/80 shadow-inner pl-8"
        />
      </div>

      {/* Skill list */}
      <div className="px-6 pb-2 max-h-[38vh] overflow-y-auto">
        <div className="space-y-0.5">
          {filteredSkills.length === 0 && (
            <div className="py-8 text-center text-xs text-muted-foreground">{t("common.noResults")}</div>
          )}
          {filteredSkills.map((skill) => {
            const isInstalled = skill.already_installed;
            const isSelected = selectedSkills.has(skill.id);
            const uniqueKey = skill.folder_path || skill.id;

            return (
              <motion.div
                key={uniqueKey}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.15 }}
                className={cn(
                  "w-full flex items-center justify-between px-3 py-2 rounded-xl text-left transition group",
                  isSelected ? "bg-primary/5" : "hover:bg-muted",
                )}
              >
                <div
                  onClick={() => onToggle(skill.id)}
                  className="flex items-center gap-3 flex-1 min-w-0 cursor-pointer py-0.5"
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onToggle(skill.id);
                    }
                  }}
                >
                  {/* Checkbox */}
                  <div
                    className={cn(
                      "w-4 h-4 rounded border-[1.5px] flex items-center justify-center shrink-0 transition",
                      isSelected
                        ? "bg-primary border-primary"
                        : isInstalled
                          ? "bg-emerald-500/20 border-emerald-500/40"
                          : "border-muted-foreground/30",
                    )}
                  >
                    {(isSelected || (isInstalled && !isSelected)) && (
                      <Check
                        className={cn("w-2.5 h-2.5", isSelected ? "text-white" : "text-emerald-500")}
                        strokeWidth={3}
                      />
                    )}
                  </div>

                  {/* Info */}
                  <div className="flex-1 min-w-0 pr-4">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          "text-caption font-medium truncate",
                          isSelected ? "text-primary" : "text-foreground",
                        )}
                      >
                        {skill.id}
                      </span>
                      {isInstalled && !isSelected && (
                        <span className="text-micro px-1.5 py-0.5 rounded-full bg-emerald-500/10 text-emerald-600 font-medium shrink-0">
                          {t("githubImportModal.installed")}
                        </span>
                      )}
                      {isSelected && isInstalled && (
                        <span className="text-micro px-1.5 py-0.5 rounded-full bg-amber-500/10 text-amber-600 font-medium shrink-0">
                          {t("detailPanel.reinstall")}
                        </span>
                      )}
                    </div>
                    {skill.description && (
                      <p className="text-xs text-muted-foreground truncate mt-0.5">{skill.description}</p>
                    )}
                  </div>
                </div>

                {/* Right side actions */}
                {isInstalled && (
                  <Button
                    variant={isSelected ? "secondary" : "ghost"}
                    size="sm"
                    className={cn(
                      "h-7 text-micro px-2.5 transition-opacity whitespace-nowrap cursor-pointer",
                      !isSelected && "opacity-0 group-hover:opacity-100",
                    )}
                    onClick={() => onToggle(skill.id)}
                  >
                    <RotateCcw className="w-3 h-3" />
                  </Button>
                )}
              </motion.div>
            );
          })}
        </div>
      </div>

      {/* Install bar */}
      <div className="px-6 py-3.5 border-t border-border/60 flex items-center justify-between">
        <span className="text-xs text-muted-foreground">
          {t("githubImportModal.selected", { count: selectedSkills.size })}
        </span>
        <div className="flex items-center gap-2">
          <Button
            variant={fullDepthEnabled ? "secondary" : "outline"}
            size="sm"
            onClick={onDeepScan}
            className="px-3 whitespace-nowrap"
          >
            <ScanSearch className="w-3.5 h-3.5 mr-1.5" />
            {fullDepthEnabled ? t("githubImportModal.rescanFullDepth") : t("githubImportModal.fullDepthLabel")}
          </Button>

          {/* Reinstall All button when every skill is already installed */}
          {skills.every((s) => s.already_installed) && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                onSelectAll(skills.map((s) => s.id));
              }}
              className="text-xs px-3"
            >
              <RotateCcw className="w-3 h-3 mr-1.5" />
              {t("githubImportModal.reinstallAll")}
            </Button>
          )}
          <Button size="sm" onClick={() => onInstall(false)} disabled={selectedSkills.size === 0} className="px-5">
            <Download className="w-3.5 h-3.5 mr-1.5" />
            {t("githubImportModal.install")}
          </Button>
        </div>
      </div>
    </div>
  );
}
