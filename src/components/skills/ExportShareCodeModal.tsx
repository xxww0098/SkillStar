import { useState, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/ui/button";
import { createShareCode, formatShareMessage, ShareCodeData, ShareCodeType } from "../../lib/shareCode";
import { toast } from "../../lib/toast";
import {
  Copy,
  KeyRound,
  Loader2,
  Check,
  X,
  Github,
  Download,
  Link2,
  FileText,
  Package,
  Sparkles,
} from "lucide-react";
import type { SkillCardDeck, Skill } from "../../types";

interface ExportShareCodeModalProps {
  open: boolean;
  onClose: () => void;
  group?: SkillCardDeck | null;
  skillNames?: string[] | null;
  hubSkills: Skill[];
  onPublishSkill?: (skillName: string) => void;
}

interface SkillExportStatus {
  name: string;
  gitUrl: string;
  /** "ok" = has git_url, "embedded" = will inline embed, "bundle" = needs bundle archive */
  status: "ok" | "embedded" | "bundle";
  content?: string;
  fileCount?: number;
}

const INLINE_SIZE_LIMIT = 4096; // 4KB limit for inline embedding

export function ExportShareCodeModal({
  open,
  onClose,
  group,
  skillNames,
  hubSkills,
  onPublishSkill,
}: ExportShareCodeModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [code, setCode] = useState<string | null>(null);
  const [shareData, setShareData] = useState<{ data: ShareCodeData; type: ShareCodeType } | null>(null);
  const [copied, setCopied] = useState(false);
  const [skillStatuses, setSkillStatuses] = useState<SkillExportStatus[]>([]);
  const [bundleSaved, setBundleSaved] = useState(false);
  const [bundleReady, setBundleReady] = useState(false);
  const [bundleExporting, setBundleExporting] = useState(false);

  // Analyze skills when modal opens
  useEffect(() => {
    const activeSkills = group ? group.skills : (skillNames || []);
    if (!open || activeSkills.length === 0) {
      setSkillStatuses([]);
      return;
    }

    const analyze = async () => {
      setAnalyzing(true);
      const statuses: SkillExportStatus[] = [];

      for (const skillName of activeSkills) {
        const localSkill = hubSkills.find((s) => s.name === skillName);
        const gitUrl =
          group?.skill_sources?.[skillName] || localSkill?.git_url || "";

        if (gitUrl) {
          statuses.push({ name: skillName, gitUrl, status: "ok" });
          continue;
        }

        // No git_url — try to read SKILL.md for inline embedding
        try {
          const content = await invoke<string>("read_skill_file_raw", {
            name: skillName,
          });

          // Check if this is a multi-file skill
          let fileCount = 1;
          try {
            const files = await invoke<string[]>("list_skill_files", {
              name: skillName,
            });
            fileCount = files.length;
          } catch {
            // ignore
          }

          if (content.length <= INLINE_SIZE_LIMIT && fileCount <= 1) {
            statuses.push({
              name: skillName,
              gitUrl: "",
              status: "embedded",
              content,
              fileCount,
            });
          } else {
            statuses.push({
              name: skillName,
              gitUrl: "",
              status: "bundle",
              fileCount,
            });
          }
        } catch {
          statuses.push({ name: skillName, gitUrl: "", status: "bundle", fileCount: 0 });
        }
      }

      setSkillStatuses(statuses);
      setAnalyzing(false);
    };

    analyze();
  }, [open, group, skillNames, hubSkills]);

  // Categorize
  const shareCodeSkills = useMemo(
    () => skillStatuses.filter((s) => s.status === "ok" || s.status === "embedded"),
    [skillStatuses]
  );
  const bundleSkills = useMemo(
    () => skillStatuses.filter((s) => s.status === "bundle"),
    [skillStatuses]
  );
  const hasEmbedded = skillStatuses.some((s) => s.status === "embedded");

  // Determine export mode: if ANY skill requires a bundle, the whole payload is exported as a bundle
  const mode: "share-code" | "bundle" = bundleSkills.length > 0 ? "bundle" : "share-code";

  const handleGenerateShareCode = async () => {
    if (shareCodeSkills.length === 0 && mode === "share-code") return;
    setLoading(true);

    const targetSkills = mode === "share-code" ? shareCodeSkills : shareCodeSkills;

    if (targetSkills.length === 0) {
      // All skills need bundle, no share code generated
      setCode("");
      setLoading(false);
      return;
    }

    const skillsList = targetSkills.map((ss) => {
      const entry: ShareCodeData["s"][number] = { n: ss.name, u: ss.gitUrl };
      if (ss.status === "embedded" && ss.content) {
        entry.c = btoa(unescape(encodeURIComponent(ss.content)));
      }
      return entry;
    });

    let sharePayload: ShareCodeData;
    let codeType: ShareCodeType = "deck";

    if (group) {
      sharePayload = {
        n: group.name,
        d: group.description,
        i: group.icon,
        s: skillsList,
      };
      codeType = "deck";
    } else {
      sharePayload = {
        n: "SkillStar Skills",
        d: `${targetSkills.length} skills shared from SkillStar`,
        i: "\u2B50",
        s: skillsList,
      };
      codeType = "skills";
    }

    try {
      const generated = await createShareCode(sharePayload, codeType);
      setCode(generated);
      setShareData({ data: sharePayload, type: codeType });
    } catch (e) {
      console.error("Export error", e);
    } finally {
      setLoading(false);
    }
  };

  const handleExport = async () => {
    if (mode === "share-code") {
      await handleGenerateShareCode();
    } else {
      setBundleExporting(true);
      await new Promise((r) => setTimeout(r, 300));
      setBundleExporting(false);
      setBundleReady(true);
    }
  };

  const handleCopy = async () => {
    if (!code) return;
    // Copy formatted message with deck name, description and share code
    const textToCopy = shareData
      ? formatShareMessage(shareData.data, code, shareData.type)
      : code;
    await navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    toast.success(t("shareResultCard.copied"));
    setTimeout(() => setCopied(false), 2000);
  };



  const handleSaveBundleFile = async () => {
    try {
      const ext = group ? "agd" : "ags";
      const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const defaultName = group
        ? `${group.name}-bundle-${ts}.${ext}`
        : `skills-bundle-${ts}.${ext}`;
      const path = await save({
        defaultPath: defaultName,
        filters: [
          { name: "SkillStar Bundle", extensions: ["ags", "agd", "agentskills"] },
        ],
      });
      if (!path) return;
      setLoading(true);
      setBundleExporting(true);
      await invoke<string>("export_multi_skill_bundle", {
        names: skillStatuses.map((s) => s.name),
        outputPath: path,
      });
      setBundleSaved(true);
      toast.success(t("exportShareCodeModal.bundleSaved"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoading(false);
      setBundleExporting(false);
    }
  };

  const reset = () => {
    setCode(null);
    setShareData(null);
    setPassword("");
    setSkillStatuses([]);
    setBundleSaved(false);
    setBundleReady(false);
  };

  const handleClose = () => {
    onClose();
    setTimeout(reset, 200);
  };

  // Check if result phase
  const isResultPhase = code !== null || bundleReady;

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
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-[420px] max-h-[calc(100vh-2rem)] z-50"
          >
            <div role="dialog" aria-modal="true" aria-label={t("exportShareCodeModal.title")} className="modal-surface max-h-[calc(100vh-2rem)] flex flex-col">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />
              <div className="relative z-10 flex flex-col min-h-0 flex-1 h-full">
                {/* Header */}
                <div className="flex items-center justify-between px-6 pt-5 pb-1">
                  <h2 className="text-heading-sm">{t("exportShareCodeModal.title")}</h2>
                  <button
                    onClick={handleClose}
                    aria-label={t("common.close")}
                    className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                  >
                    <X className="w-4 h-4" />
                  </button>
                </div>

                <div className="px-6 pb-3 text-sm text-muted-foreground leading-relaxed">
                  {t("exportShareCodeModal.description")}
                </div>

                <div className="px-6 pb-6 space-y-4 overflow-y-auto min-h-0 flex-1">
                  {/* Analyzing state */}
                  {analyzing && (
                    <div className="flex items-center justify-center py-8 text-muted-foreground gap-2">
                      <Loader2 className="w-4 h-4 animate-spin" />
                      <span className="text-sm">{t("exportShareCodeModal.analyzing")}</span>
                    </div>
                  )}

                  {/* Analysis result — strategy preview */}
                  {!analyzing && !isResultPhase && (
                    <>
                      {/* Strategy cards */}
                      <div className="space-y-2.5">
                        {/* Share Code group */}
                        {mode === "share-code" && shareCodeSkills.length > 0 && (
                          <motion.div
                            initial={{ opacity: 0, y: 6 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.05 }}
                            className="rounded-xl border border-primary/15 bg-primary/[0.03] p-3.5"
                          >
                            <div className="flex items-center gap-2 mb-2">
                              <div className="w-6 h-6 rounded-lg bg-primary/10 flex items-center justify-center">
                                <Link2 className="w-3.5 h-3.5 text-primary" />
                              </div>
                              <span className="text-xs font-semibold text-primary">
                                {t("exportShareCodeModal.shareCodeLabel")}
                              </span>
                              <span className="text-micro text-muted-foreground ml-auto">
                                {shareCodeSkills.length} {shareCodeSkills.length === 1 ? "skill" : "skills"}
                              </span>
                            </div>
                            <div className="flex flex-wrap gap-1.5 max-h-[160px] overflow-y-auto pr-1">
                              {shareCodeSkills.map((s) => (
                                <span
                                  key={s.name}
                                  className="inline-flex items-center gap-1 text-micro px-2 py-0.5 rounded-md bg-primary/8 text-primary/80 font-medium border border-primary/10"
                                >
                                  {s.status === "ok" ? (
                                    <Link2 className="w-2.5 h-2.5 opacity-60" />
                                  ) : (
                                    <FileText className="w-2.5 h-2.5 opacity-60" />
                                  )}
                                  {s.name}
                                </span>
                              ))}
                            </div>
                            {hasEmbedded && (
                              <p className="text-micro text-muted-foreground mt-2 leading-relaxed">
                                {t("exportShareCodeModal.embeddedDesc")}
                              </p>
                            )}
                          </motion.div>
                        )}

                        {/* Bundle group */}
                        {mode === "bundle" && (
                          <motion.div
                            initial={{ opacity: 0, y: 6 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.1 }}
                            className="rounded-xl border border-blue-500/15 bg-blue-500/[0.03] p-3.5"
                          >
                            <div className="flex items-center gap-2 mb-2">
                              <div className="w-6 h-6 rounded-lg bg-blue-500/10 flex items-center justify-center">
                                <Package className="w-3.5 h-3.5 text-blue-400" />
                              </div>
                              <span className="text-xs font-semibold text-blue-400">
                                {t("exportShareCodeModal.bundleLabel")}
                              </span>
                              <span className="text-micro text-muted-foreground ml-auto">
                                {skillStatuses.length} {skillStatuses.length === 1 ? "skill" : "skills"}
                              </span>
                            </div>
                            <div className="flex flex-wrap gap-1.5 max-h-[160px] overflow-y-auto pr-1">
                              {skillStatuses.map((s) => (
                                <span
                                  key={s.name}
                                  className="inline-flex items-center gap-1 text-micro px-2 py-0.5 rounded-md bg-blue-500/8 text-blue-400/80 font-medium border border-blue-500/10"
                                >
                                  <Package className="w-2.5 h-2.5 opacity-60" />
                                  {s.name}
                                  {s.fileCount && s.fileCount > 1 && (
                                    <span className="opacity-50">({s.fileCount})</span>
                                  )}
                                </span>
                              ))}
                            </div>
                            <p className="text-micro text-muted-foreground mt-2 leading-relaxed">
                              {t("exportShareCodeModal.bundleDesc")}
                            </p>
                          </motion.div>
                        )}
                      </div>

                      {/* Publish shortcut for bundle skills */}
                      {mode === "bundle" && onPublishSkill && (
                        <div className="flex flex-wrap gap-1.5 max-h-[120px] overflow-y-auto pr-1">
                          {bundleSkills.map((s) => (
                            <button
                              key={s.name}
                              onClick={() => {
                                handleClose();
                                setTimeout(() => onPublishSkill(s.name), 250);
                              }}
                              className="inline-flex items-center gap-1 text-micro font-medium text-primary hover:text-primary/80 transition-colors cursor-pointer"
                            >
                              <Github className="w-3 h-3" />
                              {t("exportShareCodeModal.publishToSimplify", { name: s.name })}
                            </button>
                          ))}
                        </div>
                      )}

                      {/* Password */}
                      {mode === "share-code" && (
                        <div className="space-y-2">
                          <label className="text-sm font-medium flex items-center gap-1.5">
                            <KeyRound className="w-3.5 h-3.5" />
                            {t("exportShareCodeModal.encryptionPassword")}
                          </label>
                          <input
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            placeholder={t("exportShareCodeModal.passwordPlaceholder")}
                            className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                          />
                        </div>
                      )}

                      {/* Export button */}
                      <Button
                        onClick={handleExport}
                        disabled={loading || skillStatuses.length === 0}
                        className="w-full mt-1"
                      >
                        {loading && (
                          <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                        )}
                        <Sparkles className="w-3.5 h-3.5 mr-1.5" />
                        {t("exportShareCodeModal.generateExport")}
                      </Button>
                    </>
                  )}

                  {/* Result phase */}
                  {isResultPhase && (
                    <div className="space-y-4 animate-in fade-in zoom-in-95 pb-4">
                      {/* Share code result */}
                      {code && code.length > 0 && (
                        <div className="space-y-2.5">
                          <div className="flex items-center gap-2">
                            <div className="w-5 h-5 rounded-md bg-primary/10 flex items-center justify-center">
                              <Link2 className="w-3 h-3 text-primary" />
                            </div>
                            <span className="text-xs font-semibold text-foreground">
                              {t("exportShareCodeModal.shareCodeReady")}
                            </span>
                          </div>
                          <div className="relative group">
                            <textarea
                              readOnly
                              value={shareData && code ? formatShareMessage(shareData.data, code, shareData.type) : (code || "")}
                              className="flex w-full rounded-xl border border-input bg-muted/50 px-3 py-2.5 text-micro font-mono shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[140px] resize-none pr-20"
                            />
                            <div className="absolute right-2 top-2 flex items-center gap-1.5">
                              <button
                                onClick={handleCopy}
                                className="p-1.5 rounded-md hover:bg-card-hover hover:scale-105 bg-card border shadow-sm text-foreground transition cursor-pointer"
                              >
                                {copied ? (
                                  <Check className="w-3.5 h-3.5 text-success" />
                                ) : (
                                  <Copy className="w-3.5 h-3.5" />
                                )}
                              </button>
                            </div>
                          </div>
                          {hasEmbedded && (
                            <p className="text-micro text-primary/70">
                              {t("exportShareCodeModal.embeddedInfo")}
                            </p>
                          )}
                        </div>
                      )}

                      {/* Bundle save section */}
                      {mode === "bundle" && (
                        <div className="space-y-2.5">
                          <div className="flex items-center gap-2">
                            <div className="w-5 h-5 rounded-md bg-blue-500/10 flex items-center justify-center">
                              <Package className="w-3 h-3 text-blue-400" />
                            </div>
                            <span className="text-xs font-semibold text-foreground">
                              {t("exportShareCodeModal.bundleReady")}
                            </span>
                            <span className="text-micro text-muted-foreground ml-auto">
                              {skillStatuses.length} {skillStatuses.length === 1 ? "skill" : "skills"}
                            </span>
                          </div>
                          <div className="rounded-xl border border-blue-500/15 bg-blue-500/[0.03] p-3">
                            <div className="flex flex-wrap gap-1.5 mb-3 max-h-[160px] overflow-y-auto pr-1">
                              {skillStatuses.map((s) => (
                                <span
                                  key={s.name}
                                  className="inline-flex items-center gap-1 text-micro px-2 py-0.5 rounded-md bg-blue-500/8 text-blue-400/80 font-medium border border-blue-500/10"
                                >
                                  <Package className="w-2.5 h-2.5 opacity-60" />
                                  {s.name}
                                </span>
                              ))}
                            </div>
                            <Button
                              size="sm"
                              variant={bundleSaved ? "ghost" : "secondary"}
                              onClick={handleSaveBundleFile}
                              disabled={loading || bundleSaved}
                              className="w-full"
                            >
                              {bundleExporting ? (
                                <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                              ) : loading ? (
                                <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                              ) : bundleSaved ? (
                                <Check className="w-3.5 h-3.5 mr-1.5 text-success" />
                              ) : (
                                <Download className="w-3.5 h-3.5 mr-1.5" />
                              )}
                              {bundleExporting
                                ? t("exportShareCodeModal.exporting")
                                : bundleSaved
                                ? t("exportShareCodeModal.bundleSavedBtn")
                                : t("exportShareCodeModal.saveBundleFile")}
                            </Button>
                          </div>
                        </div>
                      )}
                    </div>
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
