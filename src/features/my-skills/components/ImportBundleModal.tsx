import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { AlertTriangle, Check, FileText, FolderOpen, Loader2, Package, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { BundleManifest, ImportBundleResult } from "../../../types";

interface ImportBundleModalProps {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}

type Phase = "pick" | "preview" | "conflict" | "done" | "error";

export function ImportBundleModal({ open: isOpen, onClose, onImported }: ImportBundleModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const [phase, setPhase] = useState<Phase>("pick");
  const [loading, setLoading] = useState(false);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manifest, setManifest] = useState<BundleManifest | null>(null);
  const [result, setResult] = useState<ImportBundleResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reset = () => {
    setPhase("pick");
    setLoading(false);
    setFilePath(null);
    setManifest(null);
    setResult(null);
    setError(null);
  };

  const handleClose = () => {
    onClose();
    setTimeout(reset, 200);
  };

  const handlePickFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "SkillStar Bundle", extensions: ["ags", "agd", "agentskill", "agentskills"] }],
      });
      if (!selected) return;

      const path = typeof selected === "string" ? selected : selected;
      setFilePath(path);
      setLoading(true);

      const previewManifest = await invoke<BundleManifest>("preview_skill_bundle", {
        filePath: path,
      });
      setManifest(previewManifest);
      setPhase("preview");
    } catch (e: unknown) {
      setError(String(e));
      setPhase("error");
    }
  };

  const handleImport = async (force = false) => {
    if (!filePath) return;
    setLoading(true);
    setError(null);

    try {
      const importResult = await invoke<ImportBundleResult>("import_skill_bundle", {
        filePath,
        force,
      });
      setResult(importResult);
      setPhase("done");
      onImported();
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
            <div
              role="dialog"
              aria-modal="true"
              aria-label={t("importBundleModal.title")}
              className="bg-card border border-border/80 rounded-2xl shadow-xl flex flex-col overflow-hidden"
            >
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm flex items-center gap-2">
                  <Package className="w-4 h-4 text-primary" />
                  {t("importBundleModal.title")}
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
                      {t("importBundleModal.description")}
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
                        {loading ? t("importBundleModal.reading") : t("importBundleModal.pickFile")}
                      </span>
                      <span className="text-micro text-muted-foreground/70">.ags / .agd</span>
                    </button>
                  </div>
                )}

                {/* Phase: Preview */}
                {phase === "preview" && manifest && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    {/* Manifest info */}
                    <div className="rounded-xl border border-border bg-muted/30 p-4 space-y-3">
                      <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
                          <Package className="w-5 h-5 text-primary" />
                        </div>
                        <div className="min-w-0">
                          <p className="text-sm font-semibold truncate">{manifest.name}</p>
                          <p className="text-xs text-muted-foreground truncate">
                            {manifest.description || t("importBundleModal.noDescription")}
                          </p>
                        </div>
                      </div>

                      <div className="grid grid-cols-2 gap-2 text-xs">
                        <div className="flex items-center gap-1.5 text-muted-foreground">
                          <FileText className="w-3.5 h-3.5" />
                          {t("importBundleModal.fileCount", {
                            count: manifest.files.length,
                          })}
                        </div>
                        <div className="text-muted-foreground">v{manifest.version}</div>
                      </div>

                      {/* File list preview */}
                      {manifest.files.length > 0 && (
                        <div className="border-t border-border/50 pt-2">
                          <p className="text-micro text-muted-foreground font-medium mb-1.5">
                            {t("importBundleModal.contents")}
                          </p>
                          <div className="max-h-32 overflow-y-auto space-y-0.5">
                            {manifest.files.slice(0, 20).map((f) => (
                              <div
                                key={f}
                                className="flex items-center gap-1.5 text-micro font-mono text-muted-foreground"
                              >
                                <FileText className="w-3 h-3 shrink-0 opacity-50" />
                                <span className="truncate">{f}</span>
                              </div>
                            ))}
                            {manifest.files.length > 20 && (
                              <p className="text-micro text-muted-foreground/70 pl-4.5">
                                +{manifest.files.length - 20} {t("common.more")}
                              </p>
                            )}
                          </div>
                        </div>
                      )}
                    </div>

                    <Button onClick={() => handleImport(false)} disabled={loading} className="w-full">
                      {loading ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Package className="w-4 h-4 mr-2" />
                      )}
                      {loading ? t("importBundleModal.importing") : t("importBundleModal.import")}
                    </Button>
                  </div>
                )}

                {/* Phase: Conflict */}
                {phase === "conflict" && manifest && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="rounded-lg border border-warning/30 bg-warning/5 p-3 space-y-2">
                      <p className="text-xs font-medium text-warning flex items-center gap-1.5">
                        <AlertTriangle className="w-3.5 h-3.5" />
                        {t("importBundleModal.conflictTitle")}
                      </p>
                      <p className="text-micro text-muted-foreground">
                        {t("importBundleModal.conflictDesc", {
                          name: manifest.name,
                        })}
                      </p>
                    </div>

                    <div className="flex gap-2">
                      <Button variant="ghost" size="sm" className="flex-1" onClick={handleClose}>
                        {t("importBundleModal.skip")}
                      </Button>
                      <Button size="sm" className="flex-1" onClick={() => handleImport(true)} disabled={loading}>
                        {loading ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : null}
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
                      <p className="text-sm font-semibold">{t("importBundleModal.success")}</p>
                      <p className="text-xs text-muted-foreground mt-1">
                        {t("importBundleModal.successDesc", {
                          name: result.name,
                          count: result.file_count,
                        })}
                      </p>
                      {result.replaced && (
                        <p className="text-micro text-warning mt-1">{t("importBundleModal.replaced")}</p>
                      )}
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
                      <Button variant="ghost" size="sm" className="flex-1" onClick={handleClose}>
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
