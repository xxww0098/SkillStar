import { useState, useRef, useEffect } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { X, Image as ImageIcon, Sparkles, FolderKanban, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { MOTION_TRANSITION, motionDuration } from "../../../comm/motion";
import type { CustomProfileDef } from "../../../types";
import { toast } from "../../../lib/toast";

interface AddCustomAgentDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: (def: CustomProfileDef) => void;
  initialData?: CustomProfileDef;
  onRemove?: () => void;
}

export function AddCustomAgentDialog({ open, onClose, onConfirm, initialData, onRemove }: AddCustomAgentDialogProps) {
  const { t, i18n } = useTranslation();
  const prefersReducedMotion = useReducedMotion();

  const [displayName, setDisplayName] = useState("");
  const [globalPath, setGlobalPath] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [iconData, setIconData] = useState<string | null>(null);

  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      if (initialData) {
        setDisplayName(initialData.display_name);
        setGlobalPath(initialData.global_skills_dir);
        setProjectPath(initialData.project_skills_rel || "");
        setIconData(initialData.icon_data_uri || null);
      } else {
        setDisplayName("");
        setGlobalPath("");
        setProjectPath("");
        setIconData(null);
      }
    }
  }, [open, initialData]);

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (file.type !== "image/svg+xml" && !file.name.toLowerCase().endsWith(".svg")) {
      toast.error(t("settings.invalidSvgFormat", { defaultValue: "Please upload a valid SVG file." }));
      return;
    }

    const reader = new FileReader();
    reader.onload = (ev) => {
      const data = ev.target?.result as string;
      setIconData(data);

      if (!displayName.trim()) {
        const rawName = file.name.replace(/\.svg$/i, "");
        const formatted = rawName
          .split(/[-_]+/)
          .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
          .join(" ");
        setDisplayName(formatted);
      }
    };
    reader.onerror = () => {
      toast.error(t("settings.failedToReadSvg", { defaultValue: "Failed to read SVG file." }));
    };
    reader.readAsDataURL(file);
  };

  const handleConfirm = () => {
    if (!displayName.trim() || !globalPath.trim()) return;

    onConfirm({
      id: initialData?.id || "",
      display_name: displayName.trim(),
      global_skills_dir: globalPath.trim(),
      project_skills_rel: projectPath.trim(),
      icon_data_uri: iconData,
    });
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{
              ...MOTION_TRANSITION.modalBackdrop,
              duration: motionDuration(prefersReducedMotion, MOTION_TRANSITION.modalBackdrop.duration),
            }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          <motion.div
            initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, y: 20 }}
            animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, y: 15 }}
            transition={{
              ...MOTION_TRANSITION.modal,
              duration: motionDuration(prefersReducedMotion, MOTION_TRANSITION.modal.duration),
            }}
            className="fixed left-1/2 top-1/2 z-50 w-full max-w-[500px] -translate-x-1/2 -translate-y-1/2 p-4"
          >
            <div role="dialog" aria-modal="true" className="modal-surface-subtle overflow-hidden">
              <div className="flex items-center justify-between border-b border-border/40 bg-card/70 px-6 py-4">
                <div className="flex items-center gap-3">
                  <div className="flex h-9 w-9 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary">
                    <Sparkles className="h-4 w-4" />
                  </div>
                  <h2 className="text-base font-semibold tracking-tight">
                    {initialData 
                      ? t("common.edit", { defaultValue: "Edit" }) 
                      : t("settings.addCustomAgent", { defaultValue: "Add Custom Agent" })}
                  </h2>
                </div>
                <button
                  onClick={onClose}
                  className="flex h-8 w-8 items-center justify-center rounded-full hover:bg-muted text-muted-foreground transition cursor-pointer"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="p-6 space-y-4">
                <div className="space-y-1.5 flex flex-col items-center">
                  <div 
                    className="h-16 w-16 mb-2 rounded-[14px] border border-border bg-card shadow-sm flex items-center justify-center overflow-hidden cursor-pointer hover:border-primary/50 transition relative group"
                    onClick={() => fileInputRef.current?.click()}
                  >
                    {iconData ? (
                      <img src={iconData} className="w-8 h-8 object-contain" alt="Agent Icon" />
                    ) : (
                      <ImageIcon className="w-6 h-6 text-muted-foreground/60" />
                    )}
                    <div className="absolute inset-0 bg-black/40 flex items-center justify-center opacity-0 group-hover:opacity-100 transition">
                      <span className="text-[10px] font-medium text-white shadow-sm">+ SVG</span>
                    </div>
                  </div>
                  <input ref={fileInputRef} type="file" accept=".svg" className="hidden" onChange={handleFileChange} />
                  <div className="flex flex-col items-center gap-0.5 mt-1">
                    <p className="text-xs text-muted-foreground text-center">
                      {t("settings.uploadSvgIcon", { defaultValue: "Upload SVG Icon" })}
                    </p>
                    <a
                      href={`https://lobehub.com/${i18n.language === "zh-CN" ? "zh/" : ""}icons`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-[10px] text-primary/70 hover:text-primary hover:underline transition-colors"
                    >
                      {t("settings.findIconsOnLobeHub", { defaultValue: "Find icons on LobeHub" })}
                    </a>
                  </div>
                </div>

                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-foreground ml-1">
                    {t("settings.agentDisplayName", { defaultValue: "Display Name" })} <span className="text-destructive">*</span>
                  </label>
                  <Input
                    placeholder="e.g. My AI Assistant"
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    className="bg-background"
                  />
                </div>

                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-foreground ml-1 flex items-center gap-1.5">
                    <FolderKanban className="w-3.5 h-3.5 text-muted-foreground" />
                    {t("settings.globalSkillsDir", { defaultValue: "Global Skills Path" })} <span className="text-destructive">*</span>
                  </label>
                  <Input
                    placeholder="e.g. ~/.myagent/skills"
                    value={globalPath}
                    onChange={(e) => setGlobalPath(e.target.value)}
                    className="bg-background font-mono text-xs"
                  />
                </div>

                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-foreground ml-1 flex items-center gap-1.5">
                    {t("settings.projectSkillsRel", { defaultValue: "Project Skills Relative Path" })} 
                  </label>
                  <Input
                    placeholder="e.g. .myagent/skills"
                    value={projectPath}
                    onChange={(e) => setProjectPath(e.target.value)}
                    className="bg-background font-mono text-xs"
                  />
                </div>
              </div>

              <div className="flex items-center justify-between border-t border-border/40 bg-muted/20 px-6 py-4">
                <div className="flex items-center">
                  {initialData && onRemove && (
                    <Button 
                      variant="ghost" 
                      size="sm" 
                      onClick={() => {
                        onRemove();
                        onClose();
                      }} 
                      className="rounded-lg text-destructive hover:bg-destructive/10 hover:text-destructive px-2"
                      title={t("settings.removeCustomAgent", { defaultValue: "Remove custom agent" })}
                    >
                      <Trash2 className="w-4 h-4 mr-1.5" />
                      {t("common.delete", { defaultValue: "Delete" })}
                    </Button>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <Button variant="ghost" size="sm" onClick={onClose} className="rounded-lg">
                    {t("common.cancel")}
                  </Button>
                  <Button 
                    size="sm" 
                    onClick={handleConfirm} 
                    disabled={!displayName.trim() || !globalPath.trim()}
                    className="rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground"
                  >
                    {initialData ? t("common.save") : t("common.add")}
                  </Button>
                </div>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
