import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../../components/ui/button";
import { createShareCode, ShareCodeData } from "../../lib/shareCode";
import { Copy, KeyRound, Loader2, Check, X, AlertTriangle, Github } from "lucide-react";
import type { SkillCardDeck, Skill } from "../../types";

interface ExportShareCodeModalProps {
  open: boolean;
  onClose: () => void;
  group: SkillCardDeck | null;
  hubSkills: Skill[];
  onPublishSkill?: (skillName: string) => void;
}

interface SkillExportStatus {
  name: string;
  gitUrl: string;
  /** "ok" = has git_url, "embedded" = will inline embed, "too-large" = needs publish, "no-skill-md" = no file */
  status: "ok" | "embedded" | "too-large" | "no-skill-md";
  content?: string; // raw SKILL.md content for embedding
}

const INLINE_SIZE_LIMIT = 4096; // 4KB limit for inline embedding

export function ExportShareCodeModal({
  open,
  onClose,
  group,
  hubSkills,
  onPublishSkill,
}: ExportShareCodeModalProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [code, setCode] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [skillStatuses, setSkillStatuses] = useState<SkillExportStatus[]>([]);

  // Analyze skills when modal opens
  useEffect(() => {
    if (!open || !group) {
      setSkillStatuses([]);
      return;
    }

    const analyze = async () => {
      setAnalyzing(true);
      const statuses: SkillExportStatus[] = [];

      for (const skillName of group.skills) {
        const localSkill = hubSkills.find((s) => s.name === skillName);
        const gitUrl =
          group.skill_sources?.[skillName] || localSkill?.git_url || "";

        if (gitUrl) {
          statuses.push({ name: skillName, gitUrl, status: "ok" });
          continue;
        }

        // No git_url — try to read SKILL.md for inline embedding
        try {
          const content = await invoke<string>("read_skill_file_raw", {
            name: skillName,
          });
          if (content.length <= INLINE_SIZE_LIMIT) {
            statuses.push({
              name: skillName,
              gitUrl: "",
              status: "embedded",
              content,
            });
          } else {
            statuses.push({
              name: skillName,
              gitUrl: "",
              status: "too-large",
            });
          }
        } catch {
          statuses.push({ name: skillName, gitUrl: "", status: "no-skill-md" });
        }
      }

      setSkillStatuses(statuses);
      setAnalyzing(false);
    };

    analyze();
  }, [open, group, hubSkills]);

  const hasIssues = skillStatuses.some(
    (s) => s.status === "too-large" || s.status === "no-skill-md"
  );
  const hasEmbedded = skillStatuses.some((s) => s.status === "embedded");

  const handleExport = async () => {
    if (!group) return;
    setLoading(true);

    const skillsList = skillStatuses.map((ss) => {
      const entry: ShareCodeData["s"][number] = { n: ss.name, u: ss.gitUrl };

      // Inline embed for small local skills
      if (ss.status === "embedded" && ss.content) {
        entry.c = btoa(
          new TextEncoder()
            .encode(ss.content)
            .reduce((acc, b) => acc + String.fromCharCode(b), "")
        );
      }

      return entry;
    });

    const data: ShareCodeData = {
      n: group.name,
      d: group.description,
      i: group.icon,
      s: skillsList,
    };

    try {
      const generated = await createShareCode(data, password);
      setCode(generated);
    } catch (e) {
      console.error("Export error", e);
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = async () => {
    if (!code) return;
    await navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const reset = () => {
    setCode(null);
    setPassword("");
    setSkillStatuses([]);
  };

  const handleClose = () => {
    onClose();
    setTimeout(reset, 200);
  };

  const issueSkills = skillStatuses.filter(
    (s) => s.status === "too-large" || s.status === "no-skill-md"
  );

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
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-sm max-h-[calc(100vh-2rem)] z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5 max-h-[calc(100vh-2rem)] flex flex-col">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10 flex flex-col min-h-0">
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm">{t("exportShareCodeModal.title")}</h2>
                <button
                  onClick={handleClose}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="px-6 pb-2 pt-1 text-sm text-muted-foreground leading-relaxed">
                {t("exportShareCodeModal.description")}
              </div>

              <div className="px-6 py-4 space-y-4 overflow-y-auto min-h-0">
                {/* Analyzing state */}
                {analyzing && (
                  <div className="flex items-center justify-center py-4 text-muted-foreground gap-2">
                    <Loader2 className="w-4 h-4 animate-spin" />
                    <span className="text-sm">{t("exportShareCodeModal.analyzing")}</span>
                  </div>
                )}

                {/* Skill status warnings */}
                {!analyzing && !code && (
                  <>
                    {hasEmbedded && (
                      <div className="rounded-lg border border-primary/20 bg-primary/5 p-3 space-y-1.5">
                        <p className="text-xs font-medium text-primary flex items-center gap-1.5">
                          <Check className="w-3.5 h-3.5" />
                          {t("exportShareCodeModal.embeddedNotice")}
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {skillStatuses
                            .filter((s) => s.status === "embedded")
                            .map((s) => (
                              <span
                                key={s.name}
                                className="text-[10px] px-1.5 py-0.5 rounded bg-primary/10 text-primary font-medium"
                              >
                                {s.name}
                              </span>
                            ))}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t("exportShareCodeModal.embeddedDesc")}
                        </p>
                      </div>
                    )}

                    {issueSkills.length > 0 && (
                      <div className="rounded-lg border border-warning/30 bg-warning/5 p-3 space-y-2">
                        <p className="text-xs font-medium text-warning flex items-center gap-1.5">
                          <AlertTriangle className="w-3.5 h-3.5" />
                          {t("exportShareCodeModal.warning")}
                        </p>
                        <div className="space-y-1.5 max-h-56 overflow-y-auto pr-1">
                          {issueSkills.map((s) => (
                            <div
                              key={s.name}
                              className="flex items-center justify-between gap-2"
                            >
                              <div className="min-w-0">
                                <span className="text-xs font-medium text-foreground">
                                  {s.name}
                                </span>
                                <span className="text-[10px] text-muted-foreground ml-1.5">
                                  {s.status === "too-large"
                                    ? t("exportShareCodeModal.tooLarge")
                                    : t("exportShareCodeModal.notFound")}
                                </span>
                              </div>
                              {onPublishSkill && (
                                <button
                                  onClick={() => {
                                    handleClose();
                                    setTimeout(
                                      () => onPublishSkill(s.name),
                                      250
                                    );
                                  }}
                                  className="flex items-center gap-1 text-[10px] font-medium text-primary hover:underline cursor-pointer shrink-0"
                                >
                                  <Github className="w-3 h-3" />
                                  {t("exportShareCodeModal.publish")}
                                </button>
                              )}
                            </div>
                          ))}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {t("exportShareCodeModal.warningDesc")}
                        </p>
                      </div>
                    )}
                  </>
                )}

                {!analyzing && !code && (
                  <>
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

                    <Button
                      onClick={handleExport}
                      disabled={loading}
                      className="w-full mt-2"
                    >
                      {loading && (
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      )}
                      {t("exportShareCodeModal.generateShareCode")}
                      {hasIssues && (
                        <span className="ml-1 text-xs opacity-70">
                          {t("exportShareCodeModal.partialNotice")}
                        </span>
                      )}
                    </Button>
                  </>
                )}

                {code && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="relative group">
                      <textarea
                        readOnly
                        value={code}
                        className="flex w-full rounded-md border border-input bg-muted/50 px-3 py-2 text-[11px] font-mono shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring min-h-[120px] resize-none pr-10"
                      />
                      <button
                        onClick={handleCopy}
                        className="absolute right-2 top-2 p-1.5 rounded-md hover:bg-card-hover hover:scale-105 bg-card border shadow-sm text-foreground transition-all cursor-pointer"
                      >
                        {copied ? (
                          <Check className="w-4 h-4 text-success" />
                        ) : (
                          <Copy className="w-4 h-4" />
                        )}
                      </button>
                    </div>

                    {hasEmbedded && (
                      <p className="text-xs text-primary/70">
                        {t("exportShareCodeModal.embeddedInfo")}
                      </p>
                    )}

                    {code.length > 500 && (
                      <p className="text-xs text-muted-foreground">
                        {t("exportShareCodeModal.longCodeNotice")}
                      </p>
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
