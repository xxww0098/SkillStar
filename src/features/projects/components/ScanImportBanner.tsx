import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Check, Download, GitBranch, RefreshCw, ScanSearch } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { Badge } from "../../../components/ui/badge";
import { MOTION_TRANSITION } from "../../../comm/motion";
import type { AgentProfile, ImportDone, ScannedSkill } from "../../../types";

interface ScanImportBannerProps {
  unmanagedSkills: ScannedSkill[];
  scanExpanded: boolean;
  importing: boolean;
  importDone: ImportDone | null;
  enabledProfilesById: Map<string, AgentProfile>;
  onToggleScanExpanded: () => void;
  onImportAll: () => void;
}

export function ScanImportBanner({
  unmanagedSkills,
  scanExpanded,
  importing,
  importDone,
  enabledProfilesById,
  onToggleScanExpanded,
  onImportAll,
}: ScanImportBannerProps) {
  const { t } = useTranslation();

  return (
    <>
      <AnimatePresence>
        {unmanagedSkills.length > 0 && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={MOTION_TRANSITION.collapse}
            className="overflow-hidden"
          >
            <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 p-3.5">
              <div className="flex items-center gap-2.5">
                <div className="w-8 h-8 rounded-lg bg-amber-500/10 flex items-center justify-center shrink-0">
                  <ScanSearch className="w-4 h-4 text-amber-500" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium">
                    {t("projects.unmanagedSkills", { count: unmanagedSkills.length })}
                  </p>
                  <p className="text-micro text-muted-foreground">{t("projects.scanDesc")}</p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={onToggleScanExpanded}
                  className="shrink-0 text-xs h-7 px-2 text-muted-foreground hover:text-foreground hover:bg-muted/60"
                >
                  {scanExpanded ? t("common.hide") : t("common.details")}
                </Button>
                <Button
                  size="sm"
                  onClick={onImportAll}
                  disabled={importing}
                  className="shrink-0 text-xs h-7 gap-1"
                >
                  {importing ? (
                    <RefreshCw className="w-3 h-3 animate-spin" />
                  ) : (
                    <Download className="w-3 h-3" />
                  )}
                  {t("projects.importAll")}
                </Button>
              </div>

              <AnimatePresence>
                {scanExpanded && (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={MOTION_TRANSITION.fadeFast}
                    className="overflow-hidden"
                  >
                    <div className="mt-3 space-y-1">
                      {unmanagedSkills.map((skill) => (
                        <div
                          key={`${skill.agent_id}-${skill.name}`}
                          className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-background/60 border border-border/40"
                        >
                          <div className="w-6 h-6 rounded bg-primary/10 flex items-center justify-center shrink-0">
                            <GitBranch className="w-3.5 h-3.5 text-primary" />
                          </div>
                          <span className="text-xs font-mono flex-1">{skill.name}</span>

                          <div className="flex items-center gap-1.5 shrink-0">
                            {enabledProfilesById.get(skill.agent_id) && (
                              <span
                                className="text-micro font-mono text-muted-foreground/60 px-1.5 py-0.5 bg-muted/40 rounded border border-border/40"
                                title={`Found in ${enabledProfilesById.get(skill.agent_id)?.project_skills_rel}`}
                              >
                                {enabledProfilesById
                                  .get(skill.agent_id)
                                  ?.project_skills_rel.split("/")[0]}
                              </span>
                            )}
                            {skill.has_skill_md && (
                              <Badge variant="outline" className="text-micro h-4 px-1">
                                {t("projects.skillMd")}
                              </Badge>
                            )}
                            {skill.in_hub ? (
                              <Badge
                                variant="outline"
                                className="text-micro h-4 px-1 text-muted-foreground"
                              >
                                {t("projects.inHub")}
                              </Badge>
                            ) : (
                              <Badge variant="outline" className="text-micro h-4 px-1 text-amber-600">
                                {t("projects.new")}
                              </Badge>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {importDone && (
          <motion.div
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={MOTION_TRANSITION.enter}
            className="flex items-center gap-2 px-3.5 py-2.5 rounded-xl bg-emerald-500/5 border border-emerald-500/20"
          >
            <Check className="w-4 h-4 text-emerald-500 shrink-0" />
            <span className="text-xs text-emerald-700 dark:text-emerald-400">
              Imported {importDone.hub} skill{importDone.hub !== 1 ? "s" : ""} to hub, created{" "}
              {importDone.links} symlink{importDone.links !== 1 ? "s" : ""}
            </span>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
