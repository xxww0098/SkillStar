import { useState } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../../components/ui/button";
import { parseShareCode } from "../../lib/shareCode";
import { Download, KeyRound, Loader2, X, Lock, Info } from "lucide-react";

interface ImportShareCodeModalProps {
  open: boolean;
  onClose: () => void;
  onImport: (
    name: string,
    desc: string,
    icon: string,
    skills: string[],
    sources: Record<string, string>
  ) => Promise<any>;
}

export function ImportShareCodeModal({
  open,
  onClose,
  onImport,
}: ImportShareCodeModalProps) {
  const { t } = useTranslation();
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Preview state after successful parse
  const [preview, setPreview] = useState<{
    name: string;
    desc: string;
    icon: string;
    skills: string[];
    sources: Record<string, string>;
    embeddedSkills: string[];
    privateSkills: string[];
  } | null>(null);

  const handleParse = async () => {
    if (!code.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const data = await parseShareCode(code, password);

      const skillNames = data.s.map((s) => s.n);
      const sources: Record<string, string> = {};
      const embeddedSkills: string[] = [];
      const privateSkills: string[] = [];

      for (const s of data.s) {
        if (s.u) {
          sources[s.n] = s.u;
        }

        // Handle inline embedded content
        if (s.c && !s.u) {
          try {
            // Decode base64 content
            const binaryStr = atob(s.c);
            const bytes = new Uint8Array(binaryStr.length);
            for (let i = 0; i < binaryStr.length; i++) {
              bytes[i] = binaryStr.charCodeAt(i);
            }
            const content = new TextDecoder().decode(bytes);

            // Write the skill to local hub via Tauri command
            await invoke("create_local_skill_from_content", {
              name: s.n,
              content,
            });
            embeddedSkills.push(s.n);
          } catch (e) {
            console.warn(`Failed to create embedded skill ${s.n}:`, e);
            // Still add it to the group, just won't have content
          }
        }

        // Track private repo warnings
        if (s.p) {
          privateSkills.push(s.n);
        }
      }

      // If there are warnings, show preview; otherwise import directly
      if (privateSkills.length > 0 || embeddedSkills.length > 0) {
        setPreview({
          name: data.n,
          desc: data.d,
          icon: data.i,
          skills: skillNames,
          sources,
          embeddedSkills,
          privateSkills,
        });
        setLoading(false);
      } else {
        await onImport(data.n, data.d, data.i, skillNames, sources);
        resetAndClose();
      }
    } catch (e: any) {
      setError(e.message || t("importShareCodeModal.parseError"));
      setLoading(false);
    }
  };

  const handleConfirmImport = async () => {
    if (!preview) return;
    setLoading(true);
    try {
      await onImport(
        preview.name,
        preview.desc,
        preview.icon,
        preview.skills,
        preview.sources
      );
      resetAndClose();
    } catch (e: any) {
      setError(e.message || t("importShareCodeModal.importError"));
      setLoading(false);
    }
  };

  const resetAndClose = () => {
    setCode("");
    setPassword("");
    setPreview(null);
    setError(null);
    onClose();
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={resetAndClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-sm z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm">
                  {preview ? t("importShareCodeModal.confirmImport") : t("importShareCodeModal.importGroup")}
                </h2>
                <button
                  onClick={resetAndClose}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              {!preview ? (
                <>
                  <div className="px-6 pb-2 pt-1 text-sm text-muted-foreground leading-relaxed">
                    {t("importShareCodeModal.description")}
                  </div>

                  <div className="px-6 py-4 space-y-4">
                    <div className="space-y-2">
                      <label className="text-sm font-medium">{t("importShareCodeModal.shareCode")}</label>
                      <textarea
                        value={code}
                        onChange={(e) => setCode(e.target.value)}
                        placeholder={t("importShareCodeModal.shareCodePlaceholder")}
                        className="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[80px] resize-y"
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium flex items-center gap-1.5">
                        <KeyRound className="w-3.5 h-3.5" />
                        {t("importShareCodeModal.password")}
                      </label>
                      <input
                        type="password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder={t("importShareCodeModal.passwordPlaceholder")}
                        className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                      />
                    </div>

                    {error && (
                      <div className="text-xs text-destructive bg-destructive/10 p-2.5 rounded-md border border-destructive/20">
                        {error}
                      </div>
                    )}
                  </div>

                  <div className="flex justify-end gap-2 px-6 py-3.5 border-t border-border/60 bg-muted/20">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={resetAndClose}
                      disabled={loading}
                    >
                      {t("importShareCodeModal.cancel")}
                    </Button>
                    <Button
                      size="sm"
                      onClick={handleParse}
                      disabled={loading || !code.trim()}
                    >
                      {loading ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Download className="w-4 h-4 mr-2" />
                      )}
                      {loading ? t("importShareCodeModal.importing") : t("importShareCodeModal.import")}
                    </Button>
                  </div>
                </>
              ) : (
                /* Preview phase with warnings */
                <>
                  <div className="px-6 py-4 space-y-4">
                    {/* Group info */}
                    <div className="flex items-center gap-3">
                      <div className="w-10 h-10 rounded-xl bg-primary/5 border border-primary/10 flex items-center justify-center text-xl shrink-0">
                        {preview.icon}
                      </div>
                      <div className="min-w-0">
                        <p className="text-sm font-semibold truncate">
                          {preview.name}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {t("importShareCodeModal.skillsCount", { count: preview.skills.length })}
                        </p>
                      </div>
                    </div>

                    {/* Embedded skills notice */}
                    {preview.embeddedSkills.length > 0 && (
                      <div className="rounded-lg border border-primary/20 bg-primary/5 p-3 space-y-1.5">
                          <p className="text-xs font-medium text-primary flex items-center gap-1.5">
                            <Info className="w-3.5 h-3.5" />
                            {t("importShareCodeModal.embeddedCreated")}
                          </p>
                        <div className="flex flex-wrap gap-1">
                          {preview.embeddedSkills.map((name) => (
                            <span
                              key={name}
                              className="text-[10px] px-1.5 py-0.5 rounded bg-primary/10 text-primary font-medium"
                            >
                              {name}
                            </span>
                          ))}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t("importShareCodeModal.embeddedDesc")}
                        </p>
                      </div>
                    )}

                    {/* Private repo warning */}
                    {preview.privateSkills.length > 0 && (
                      <div className="rounded-lg border border-warning/30 bg-warning/5 p-3 space-y-1.5">
                          <p className="text-xs font-medium text-warning flex items-center gap-1.5">
                            <Lock className="w-3.5 h-3.5" />
                            {t("importShareCodeModal.privateRepoWarning")}
                          </p>
                        <div className="flex flex-wrap gap-1">
                          {preview.privateSkills.map((name) => (
                            <span
                              key={name}
                              className="text-[10px] px-1.5 py-0.5 rounded bg-warning/10 text-warning font-medium"
                            >
                              {name}
                            </span>
                          ))}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t("importShareCodeModal.privateRepoDesc")}
                        </p>
                      </div>
                    )}

                    {error && (
                      <div className="text-xs text-destructive bg-destructive/10 p-2.5 rounded-md border border-destructive/20">
                        {error}
                      </div>
                    )}
                  </div>

                  <div className="flex justify-end gap-2 px-6 py-3.5 border-t border-border/60 bg-muted/20">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={resetAndClose}
                      disabled={loading}
                    >
                      {t("importShareCodeModal.cancel")}
                    </Button>
                    <Button
                      size="sm"
                      onClick={handleConfirmImport}
                      disabled={loading}
                    >
                      {loading ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Download className="w-4 h-4 mr-2" />
                      )}
                      {loading ? t("importShareCodeModal.creating") : t("importShareCodeModal.confirmImportBtn")}
                    </Button>
                  </div>
                </>
              )}
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
