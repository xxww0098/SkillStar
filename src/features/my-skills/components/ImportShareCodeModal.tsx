import { useState } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../../../components/ui/button";
import { parseShareCode, extractShareCode } from "../../../lib/shareCode";
import { Download, KeyRound, Loader2, X, Lock, Info, Star } from "lucide-react";
import type { SkillCardDeck } from "../../../types";


interface ImportShareCodeModalProps {
  open: boolean;
  onClose: () => void;
  onImport: (
    name: string,
    desc: string,
    icon: string,
    skills: string[],
    sources: Record<string, string>,
    download: boolean
  ) => Promise<void>;
  /** Existing groups for duplicate detection */
  existingGroups?: SkillCardDeck[];
}

export function ImportShareCodeModal({
  open,
  onClose,
  onImport,
  existingGroups = [],
}: ImportShareCodeModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [intentInstall, setIntentInstall] = useState(false);
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

  const handleParse = async (install: boolean) => {
    if (!code.trim()) return;
    setIntentInstall(install);
    setLoading(true);
    setError(null);
    try {
      // Extract raw share code from formatted message or use as-is
      const rawCode = extractShareCode(code);
      const { data: sharePayload } = await parseShareCode(rawCode);

      const skillNames = sharePayload.s.map((skillEntry) => skillEntry.n);
      const sources: Record<string, string> = {};
      const embeddedSkills: string[] = [];
      const privateSkills: string[] = [];

      for (const skillEntry of sharePayload.s) {
        if (skillEntry.u) {
          sources[skillEntry.n] = skillEntry.u;
        }

        // Handle inline embedded content
        if (skillEntry.c && !skillEntry.u) {
          try {
            // Decode base64 content
            const binaryStr = atob(skillEntry.c);
            const bytes = new Uint8Array(binaryStr.length);
            for (let i = 0; i < binaryStr.length; i++) {
              bytes[i] = binaryStr.charCodeAt(i);
            }
            const content = new TextDecoder().decode(bytes);

            // Write the skill to local hub via Tauri command
            await invoke("create_local_skill_from_content", {
              name: skillEntry.n,
              content,
            });
            embeddedSkills.push(skillEntry.n);
          } catch (e) {
            console.warn(`Failed to create embedded skill ${skillEntry.n}:`, e);
            // Still add it to the group, just won't have content
          }
        }

        // Track private repo warnings
        if (skillEntry.p) {
          privateSkills.push(skillEntry.n);
        }
      }

      // Check for duplicates before importing
      const getSkillsHash = (skills: { n: string; u?: string }[]) => {
        const sortedSkills = [...skills].sort((a, b) => a.n.localeCompare(b.n));
        return JSON.stringify(sortedSkills.map(s => ({ n: s.n, u: s.u || "" })));
      };

      const importedSkillsHash = getSkillsHash(sharePayload.s);

      // 1. Check if an identical deck (same skills content) already exists
      const duplicateContentGroup = existingGroups.find(g => {
        const existingSkillsHash = getSkillsHash(g.skills.map(name => ({
          n: name,
          u: g.skill_sources?.[name] || ""
        })));
        return existingSkillsHash === importedSkillsHash;
      });

      if (duplicateContentGroup) {
        setError(t("importShareCodeModal.alreadyHasGroup", { name: duplicateContentGroup.name, defaultValue: "已导入过本卡组" }));
        setLoading(false);
        return;
      }

      // 2. Check if a deck with the same name already exists
      const duplicateNameGroup = existingGroups.find((g) => g.name === sharePayload.n);
      if (duplicateNameGroup) {
        setError(t("importShareCodeModal.duplicateGroup", { name: sharePayload.n }));
        setLoading(false);
        return;
      }

      // If there are warnings, show preview; otherwise import directly
      if (privateSkills.length > 0 || embeddedSkills.length > 0) {
        setPreview({
          name: sharePayload.n,
          desc: sharePayload.d,
          icon: sharePayload.i,
          skills: skillNames,
          sources,
          embeddedSkills,
          privateSkills,
        });
        setLoading(false);
      } else {
        await onImport(sharePayload.n, sharePayload.d, sharePayload.i, skillNames, sources, install);
        resetAndClose();
      }
    } catch (e: unknown) {
      const msg = typeof e === 'string' ? e : (typeof e === 'object' && e !== null && 'message' in e ? String((e as {message?: string}).message) : t("importShareCodeModal.parseError"));
      setError(msg);
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
        preview.sources,
        intentInstall
      );
      resetAndClose();
    } catch (e: unknown) {
      setError(typeof e === 'object' && e !== null && 'message' in e ? String((e as {message?: string}).message) : t("importShareCodeModal.importError"));
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
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={resetAndClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-sm z-50"
          >
            <div role="dialog" aria-modal="true" aria-label={preview ? t("importShareCodeModal.confirmImport") : t("importShareCodeModal.importGroup")} className="modal-surface">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm">
                  {preview ? t("importShareCodeModal.confirmImport") : t("importShareCodeModal.importGroup")}
                </h2>
                <button
                  onClick={resetAndClose}
                  aria-label={t("common.close")}
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
                        className="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[100px] resize-y font-mono"
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
                      variant="outline"
                      size="sm"
                      className="border-primary/20 hover:bg-primary/5 hover:text-primary"
                      onClick={() => handleParse(false)}
                      disabled={loading || !code.trim()}
                    >
                      {loading && !intentInstall ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Star className="w-4 h-4 mr-2" />
                      )}
                      {t("importShareCodeModal.favorite", { defaultValue: "收藏" })}
                    </Button>
                    <Button
                      size="sm"
                      onClick={() => handleParse(true)}
                      disabled={loading || !code.trim()}
                    >
                      {loading && intentInstall ? (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      ) : (
                        <Download className="w-4 h-4 mr-2" />
                      )}
                      {t("importShareCodeModal.importAndDownload", { defaultValue: "导入并下载" })}
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
                        <div className="flex flex-wrap gap-1 max-h-[120px] overflow-y-auto pr-1">
                          {preview.embeddedSkills.map((name) => (
                            <span
                              key={name}
                              className="text-micro px-1.5 py-0.5 rounded bg-primary/10 text-primary font-medium"
                            >
                              {name}
                            </span>
                          ))}
                        </div>
                        <p className="text-micro text-muted-foreground">
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
                        <div className="flex flex-wrap gap-1 max-h-[120px] overflow-y-auto pr-1">
                          {preview.privateSkills.map((name) => (
                            <span
                              key={name}
                              className="text-micro px-1.5 py-0.5 rounded bg-warning/10 text-warning font-medium"
                            >
                              {name}
                            </span>
                          ))}
                        </div>
                        <p className="text-micro text-muted-foreground">
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
                      ) : intentInstall ? (
                        <Download className="w-4 h-4 mr-2" />
                      ) : (
                        <Star className="w-4 h-4 mr-2" />
                      )}
                      {loading 
                        ? t("importShareCodeModal.creating") 
                        : (intentInstall ? t("importShareCodeModal.confirmImportAndDownload", { defaultValue: "确认导入并下载" }) : t("importShareCodeModal.confirmFavorite", { defaultValue: "确认收藏" }))}
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
