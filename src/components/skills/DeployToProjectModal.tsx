import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, FolderOpen, Rocket, Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../ui/button";
import { cn } from "../../lib/utils";
import type { AgentProfile } from "../../types";

interface DeployToProjectModalProps {
  open: boolean;
  onClose: () => void;
  selectedSkills: string[];
  profiles: AgentProfile[];
  onDeploy: (projectPath: string, skills: string[], agentTypes: string[]) => Promise<number>;
}

export function DeployToProjectModal({
  open: isOpen,
  onClose,
  selectedSkills,
  profiles,
  onDeploy,
}: DeployToProjectModalProps) {
  const { t } = useTranslation();
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [selectedAgents, setSelectedAgents] = useState<string[]>(["claude"]);
  const [deploying, setDeploying] = useState(false);
  const [result, setResult] = useState<number | null>(null);

  const handleChooseFolder = async () => {
    const path = await open({
      directory: true,
        title: t("deployModal.chooseDir"),
    });
    if (path) setProjectPath(path as string);
  };

  const handleDeploy = async () => {
    if (!projectPath) return;
    setDeploying(true);
    setResult(null);
    try {
      const count = await onDeploy(projectPath, selectedSkills, selectedAgents);
      setResult(count);
    } catch (e) {
      console.error("Deploy failed:", e);
    } finally {
      setDeploying(false);
    }
  };

  const handleClose = () => {
    setProjectPath(null);
    setResult(null);
    setDeploying(false);
    onClose();
  };

  const toggleAgent = (id: string) => {
    setSelectedAgents((prev) =>
      prev.includes(id) ? prev.filter((a) => a !== id) : [...prev, id]
    );
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-md z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              {/* Header */}
              <div className="flex items-center justify-between px-6 pt-4 pb-0">
                <h2 className="text-heading-sm">{t("deployModal.title")}</h2>
                <button
                  onClick={handleClose}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="px-6 py-4 space-y-4">
                {/* Agent selector */}
                <div className="grid grid-cols-5 gap-1.5">
                  {profiles.map((profile) => {
                    const isSelected = selectedAgents.includes(profile.id);
                    return (
                      <button
                        key={profile.id}
                        onClick={() => toggleAgent(profile.id)}
                        className={cn(
                          "relative flex flex-col items-center gap-1 p-2.5 rounded-xl border transition-colors cursor-pointer",
                          isSelected
                            ? "border-primary/40 bg-primary/5"
                            : "border-border hover:bg-muted"
                        )}
                      >
                        {isSelected && (
                          <div className="absolute top-1 right-1 w-1.5 h-1.5 rounded-full bg-primary" />
                        )}
                        <img
                          src={`/${profile.icon}`}
                          alt={profile.display_name}
                          className={cn(
                            "w-5 h-5 transition-[filter,opacity]",
                            !isSelected && "grayscale opacity-40"
                          )}
                        />
                        <span className="text-[9px] font-medium truncate w-full text-center leading-tight">
                          {profile.display_name.split(" ")[0]}
                        </span>
                      </button>
                    );
                  })}
                </div>

                {/* Project folder */}
                <div className="flex gap-2">
                  <button
                    onClick={handleChooseFolder}
                    className={cn(
                      "flex-1 flex items-center gap-2 px-3 py-2 rounded-lg border text-sm text-left transition-colors cursor-pointer",
                      projectPath
                        ? "border-border bg-card font-mono text-foreground"
                        : "border-dashed border-border hover:border-border hover:bg-muted text-muted-foreground"
                    )}
                  >
                    <FolderOpen className="w-3.5 h-3.5 shrink-0 opacity-60" />
                    <span className="truncate">
                      {projectPath || t("deployModal.chooseFolder")}
                    </span>
                  </button>
                </div>

                {/* Skills pills */}
                <div className="flex flex-wrap gap-1">
                  {selectedSkills.map((name) => (
                    <span
                      key={name}
                      className="px-1.5 py-0.5 rounded text-[11px] bg-muted text-muted-foreground"
                    >
                      {name}
                    </span>
                  ))}
                </div>

                {/* Result */}
                {result !== null && (
                  <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="flex items-center gap-2 text-sm text-success"
                  >
                    <Check className="w-3.5 h-3.5" />
                    {t("deployModal.deployed", { count: result })}
                  </motion.div>
                )}
              </div>

              {/* Footer */}
              <div className="flex items-center justify-end gap-2 px-6 py-3.5 border-t border-border/60">
                <Button variant="ghost" size="sm" onClick={handleClose}>
                  {result !== null ? t("common.done") : t("deployModal.cancel")}
                </Button>
                {result === null && (
                  <Button
                    size="sm"
                    onClick={handleDeploy}
                    disabled={!projectPath || deploying}
                  >
                    {deploying ? (
                      <span className="flex items-center gap-1.5">
                        <span className="w-3.5 h-3.5 border-2 border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin" />
                        {t("deployModal.deploying")}
                      </span>
                    ) : (
                      <span className="flex items-center gap-1.5">
                        <Rocket className="w-3.5 h-3.5" />
                        {t("deployModal.deploy")}
                      </span>
                    )}
                  </Button>
                )}
              </div>
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
