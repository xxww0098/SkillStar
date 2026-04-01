import { useState } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../../../components/ui/button";
import {
  Package,
  FileText,
  Loader2,
  X,
  Check,
  AlertTriangle,
  FolderOpen,
} from "lucide-react";
import type { MultiManifest, ImportMultiBundleResult } from "../../../types";

interface ImportDeckBundleModalProps {
  open: boolean;
  onClose: () => void;
  /** Called after successful import with the list of skill names and bundle metadata */
  onDeckImported: (skillNames: string[], name: string, description: string) => void;
}

type Phase = "pick" | "preview" | "conflict" | "done" | "error";

export function ImportDeckBundleModal({
  open: isOpen,
  onClose,
  onDeckImported,
}: ImportDeckBundleModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const [phase, setPhase] = useState<Phase>("pick");
  const [loading, setLoading] = useState(false);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manifest, setManifest] = useState<MultiManifest | null>(null);
  const [result, setResult] = useState<ImportMultiBundleResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Extract a deck name from file name for display
  const [deckName, setDeckName] = useState("");

  const reset = () => {
    setPhase("pick");
    setLoading(false);
    setFilePath(null);
    setManifest(null);
    setResult(null);
    setError(null);
    setDeckName("");
  };

  const handleClose = () => {
    onClose();
    setTimeout(reset, 200);
  };

  const handlePickFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "SkillStar Deck Bundle", extensions: ["agd"] },
        ],
      });
      if (!selected) return;

      const path = typeof selected === "string" ? selected : selected;
      setFilePath(path);
      setLoading(true);

      // Extract a display name from the file path
      const fileName = path.split(/[/\\]/).pop() || "";
      const baseName = fileName.replace(/\.(agd|ags)$/i, "").replace(/-bundle-[\d-T]+$/i, "");
      setDeckName(baseName);

      const previewManifest = await invoke<MultiManifest>("preview_multi_skill_bundle", {
        filePath: path,
      });
      setManifest(previewManifest);
      setPhase("preview");
    } catch (e: unknown) {
      setError(String(e));
      setPhase("error");
    } finally {
      setLoading(false);
    }
  };

  const handleImport = async (force = false) => {
    if (!filePath) return;
    setLoading(true);
    setError(null);

    try {
      const importResult = await invoke<ImportMultiBundleResult>("import_multi_skill_bundle", {
        filePath,
        force,
      });
      setResult(importResult);
      setPhase("done");

      // Auto-create deck with imported skill names
      const desc = manifest
        ? `${manifest.skills.length} ${t("skillCards.skillsCount", { count: manifest.skills.length })}`
        : "";
      onDeckImported(importResult.skill_names, deckName || "Imported Deck", desc);
    } catch (e: unknown) {
      const msg = String(e);
      if (msg.startsWith("CONFLICT:")) {
        setPhase("conflict");
      } else {
        setError(msg);
        setPhase("error");
      }
    } finally {
      setLoading(false);
    }
  };

  const totalFileCount = manifest?.skills.reduce((sum, s) => sum + s.file_count, 0) ?? 0;

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.15 }}
            className="fixed inset-0 bg-black/30 backdrop-blur-[2px] z-50"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-sm z-50"
          >
            <div role="dialog" aria-modal="true" aria-label={t("importDeckBundleModal.title")} className="bg-card border border-border/80 rounded-2xl shadow-xl flex flex-col overflow-hidden">
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm flex items-center gap-2">
                  <Package className="w-4 h-4 text-primary" />
                  {t("importDeckBundleModal.title")}
                </h2>
                <button
                  onClick={handleClose}
                  aria-label={t("common.close")}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="px-6 py-4 space-y-4">
                {/* Phase: Pick File */}
                {phase === "pick" && (
                  <div className="space-y-4">
                    <p className="text-sm text-muted-foreground leading-relaxed">
                      {t("importDeckBundleModal.description")}
                    </p>

                    <button
                      onClick={handlePickFile}
                      disabled={loading}
                      className="w-full flex flex-col items-center gap-3 py-8 rounded-xl border-2 border-dashed border-border hover:border-primary/40 hover:bg-primary/5 transition duration-200 cursor-pointer group"
                    >
                      {loading ? (
                        <Loader2 className="w-8 h-8 text-primary animate-spin" />
                      ) : (
                        <FolderOpen className="w-8 h-8 text-muted-foreground group-hover:text-primary transition-colors" />
                      )}
                      <span className="text-sm font-medium text-muted-foreground group-hover:text-foreground transition-colors">
                        {loading
                          ? t("importDeckBundleModal.reading")
                          : t("importDeckBundleModal.pickFile")}
                      </span>
                      <span className="text-micro text-muted-foreground/70">
                        .agd
                      </span>
                    </button>
                  </div>
                )}

                {/* Phase: Preview */}
                {phase === "preview" && manifest && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="rounded-xl border border-border bg-muted/30 p-4 space-y-3">
                      <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
                          <Package className="w-5 h-5 text-primary" />
                        </div>
                        <div className="min-w-0">
                          <p className="text-sm font-semibold truncate">
                            {deckName || t("importDeckBundleModal.deckBundle")}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {t("importDeckBundleModal.skillsCount", { count: manifest.skills.length })}
                            {" · "}
                            {t("importBundleModal.fileCount", { count: totalFileCount })}
                          </p>
                        </div>
                      </div>

                      {/* Skills list */}
                      {manifest.skills.length > 0 && (
                        <div className="border-t border-border/50 pt-2">
                          <p className="text-micro text-muted-foreground font-medium mb-1.5">
                            {t("importDeckBundleModal.includedSkills")}
                          </p>
                          <div className="max-h-32 overflow-y-auto space-y-0.5">
                            {manifest.skills.map((s) => (
                              <div
                                key={s.name}
                                className="flex items-center gap-1.5 text-micro font-mono text-muted-foreground"
                              >
                                <FileText className="w-3 h-3 shrink-0 opacity-50" />
                                <span className="truncate">{s.name}</span>
                                <span className="text-muted-foreground/50 ml-auto shrink-0">
                                  {s.file_count} {s.file_count === 1 ? "file" : "files"}
                                </span>
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>

                    <Button
                      onClick={() => handleImport(false)}
                      disabled={loading}
                      className="w-full"
                    >
                      {loading ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Package className="w-4 h-4 mr-2" />
                      )}
                      {loading
                        ? t("importDeckBundleModal.importing")
                        : t("importDeckBundleModal.import")}
                    </Button>
                  </div>
                )}

                {/* Phase: Conflict */}
                {phase === "conflict" && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="rounded-lg border border-warning/30 bg-warning/5 p-3 space-y-2">
                      <p className="text-xs font-medium text-warning flex items-center gap-1.5">
                        <AlertTriangle className="w-3.5 h-3.5" />
                        {t("importDeckBundleModal.conflictTitle")}
                      </p>
                      <p className="text-micro text-muted-foreground">
                        {t("importDeckBundleModal.conflictDesc")}
                      </p>
                    </div>

                    <div className="flex gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="flex-1"
                        onClick={handleClose}
                      >
                        {t("common.cancel")}
                      </Button>
                      <Button
                        size="sm"
                        className="flex-1"
                        onClick={() => handleImport(true)}
                        disabled={loading}
                      >
                        {loading ? (
                          <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                        ) : null}
                        {t("importBundleModal.replace")}
                      </Button>
                    </div>
                  </div>
                )}

                {/* Phase: Done */}
                {phase === "done" && result && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95 text-center py-4">
                    <div className="w-12 h-12 rounded-full bg-success/10 border border-success/20 flex items-center justify-center mx-auto">
                      <Check className="w-6 h-6 text-success" />
                    </div>
                    <div>
                      <p className="text-sm font-semibold">
                        {t("importDeckBundleModal.success")}
                      </p>
                      <p className="text-xs text-muted-foreground mt-1">
                        {t("importDeckBundleModal.successDesc", {
                          count: result.skill_names.length,
                          files: result.total_file_count,
                        })}
                      </p>
                    </div>
                    <Button onClick={handleClose} className="w-full">
                      {t("common.done")}
                    </Button>
                  </div>
                )}

                {/* Phase: Error */}
                {phase === "error" && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="text-xs text-destructive bg-destructive/10 p-3 rounded-lg border border-destructive/20">
                      {error}
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="flex-1"
                        onClick={handleClose}
                      >
                        {t("common.close")}
                      </Button>
                      <Button
                        size="sm"
                        className="flex-1"
                        onClick={() => {
                          reset();
                          handlePickFile();
                        }}
                      >
                        {t("common.retry")}
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
