import { useState, useEffect, useCallback, useRef } from "react";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { X, Download } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { looksLikeShareCode, parseShareCode, extractShareCode, type ShareCodeData } from "../../lib/shareCode";
import type {
  ScanResult,
  SkillInstallTarget,
  RepoHistoryEntry,
} from "../../types";
import {
  InputURLPhase,
  LoadingPhase,
  SelectSkillsPhase,
  CompletedPhase,
  ErrorPhase,
  ShareCodePreviewPhase,
} from "./import-modal";

type Phase =
  | "inputURL"
  | "scanning"
  | "selectSkills"
  | "installing"
  | "completed"
  | "error"
  | "shareCodePreview"
  | "shareCodeInstalling";

export interface ImportModalProps {
  open: boolean;
  onClose: () => void;
  onInstalled: (installedNames: string[]) => void;
  /** Pre-fill URL and auto-scan (for Marketplace Install flow) */
  initialUrl?: string;
  autoScan?: boolean;
  /** When set, only pre-select this specific skill after scan (instead of all) */
  preSelectedSkill?: string;
  /** Callback to trigger local file (.ags) import flow */
  onPickLocalFile?: () => void;
  /** Callback to pack installed skills into a deck immediately */
  onPackGroup?: (skillNames: string[]) => void;
  /** Pre-filled share code from clipboard auto-detect */
  initialShareCode?: string;
  /** Called when the share code is consumed */
  onClearShareCode?: () => void;
}

export function ImportModal({
  open: isOpen,
  onClose,
  onInstalled,
  initialUrl,
  autoScan,
  preSelectedSkill,
  onPickLocalFile,
  onPackGroup,
  initialShareCode,
  onClearShareCode,
}: ImportModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();

  // Keep a ref so handleScan always reads the latest value
  // (avoids stale closure when called from setTimeout)
  const preSelectedSkillRef = useRef(preSelectedSkill);
  preSelectedSkillRef.current = preSelectedSkill;

  // ── State ──────────────────────────────────────────────────────
  const [phase, setPhase] = useState<Phase>("inputURL");
  const [urlInput, setUrlInput] = useState("");
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [selectedSkills, setSelectedSkills] = useState<Set<string>>(new Set());
  const [history, setHistory] = useState<RepoHistoryEntry[]>([]);
  const [progressMsg, setProgressMsg] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const [installedCount, setInstalledCount] = useState(0);
  // Share code state
  const [shareCodeData, setShareCodeData] = useState<ShareCodeData | null>(null);
  const [shareCodePassword, setShareCodePassword] = useState("");
  const [shareCodeError, setShareCodeError] = useState("");
  const [shareCodeDetected, setShareCodeDetected] = useState(false);

  // ── Reset on open ──────────────────────────────────────────────
  useEffect(() => {
    if (isOpen) {
      setShareCodeData(null);
      setShareCodePassword("");
      setShareCodeError("");
      setScanResult(null);
      setSelectedSkills(new Set());
      setProgressMsg("");
      setErrorMsg("");
      setInstalledCount(0);

      // If opening with a share code (clipboard detect or prop), go directly to share code flow
      if (initialShareCode && looksLikeShareCode(initialShareCode)) {
        setPhase("inputURL");
        setUrlInput(initialShareCode);
        setShareCodeDetected(true);
        // Auto-parse after slight delay for animation
        setTimeout(() => handleParseShareCode(initialShareCode), 150);
        return;
      }

      setPhase("inputURL");
      setUrlInput(initialUrl || "");
      setShareCodeDetected(false);

      // Load history
      invoke<RepoHistoryEntry[]>("list_repo_history")
        .then(setHistory)
        .catch(() => setHistory([]));

      // Auto-scan if initialUrl is provided
      if (initialUrl && autoScan) {
        setTimeout(() => handleScan(initialUrl), 100);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  // ── Share Code Parse ─────────────────────────────────────────
  const handleParseShareCode = useCallback(async (code: string) => {
    setShareCodeError("");
    setProgressMsg(t("shareCodeImport.parsing"));
    try {
      // Extract raw share code from formatted message or use as-is
      const rawCode = extractShareCode(code);
      const { data } = await parseShareCode(rawCode);
      setShareCodeData(data);
      setPhase("shareCodePreview");
    } catch (e) {
      const errMsg = String(e);
      if (errMsg.includes("expired")) {
        setShareCodeError(errMsg.replace(/^Error:\s*/, ""));
        setShareCodeData(null);
      } else {
        setShareCodeError(errMsg);
        setPhase("shareCodePreview");
        setShareCodeData(null);
      }
    }
  }, [t]);

  const handleShareCodeInstall = useCallback(async () => {
    if (!shareCodeData) return;
    setPhase("shareCodeInstalling");
    setProgressMsg(t("shareCodeImport.installing"));
    const installedNames: string[] = [];
    try {
      for (const skill of shareCodeData.s) {
        if (skill.u) {
          // Git-backed skill: use repo scanner flow
          try {
            const result = await invoke<ScanResult>("scan_github_repo", { url: skill.u });
            const target = result.skills.find((s) => s.id === skill.n) || result.skills[0];
            if (target) {
              const installed = await invoke<string[]>("install_from_scan", {
                repoUrl: result.source_url,
                source: result.source,
                skills: [{ id: target.id, folder_path: target.folder_path }],
              });
              installedNames.push(...installed);
            }
          } catch (e) {
            console.warn(`[ShareCode] Failed to install ${skill.n} from ${skill.u}:`, e);
          }
        } else if (skill.c) {
          // Embedded skill: decode base64 content and create as local skill
          try {
            const binaryStr = atob(skill.c);
            const bytes = new Uint8Array(binaryStr.length);
            for (let i = 0; i < binaryStr.length; i++) bytes[i] = binaryStr.charCodeAt(i);
            const content = new TextDecoder().decode(bytes);
            await invoke("create_local_skill_from_content", { name: skill.n, content });
            installedNames.push(skill.n);
          } catch (e) {
            console.warn(`[ShareCode] Failed to create embedded skill ${skill.n}:`, e);
          }
        }
      }
      setInstalledCount(installedNames.length);
      setPhase("completed");
      if (installedNames.length > 0) onInstalled(installedNames);
      onClearShareCode?.();
    } catch (e) {
      setErrorMsg(String(e));
      setPhase("error");
    }
  }, [shareCodeData, t, onInstalled, onClearShareCode]);

  // ── Smart input handler: detect share code in URL input ──────
  const handleUrlInputChange = useCallback((value: string) => {
    setUrlInput(value);
    // Auto-detect share code when pasted (handle formatted messages too)
    const extracted = extractShareCode(value.trim());
    if (looksLikeShareCode(extracted)) {
      setTimeout(() => handleParseShareCode(value.trim()), 50);
    }
  }, [handleParseShareCode]);

  // ── Scan ───────────────────────────────────────────────────────
  const handleScan = useCallback(
    async (url?: string) => {
      const input = (url || urlInput).trim();
      if (!input) return;

      setPhase("scanning");
      setProgressMsg(t("githubImportModal.cloning"));

      try {
        const result = await invoke<ScanResult>("scan_github_repo", {
          url: input,
        });

        if (result.skills.length === 0) {
          setErrorMsg(t("githubImportModal.noSkillsFound"));
          setPhase("error");
          return;
        }

        setScanResult(result);

        // Pre-select: if a specific skill was requested, only select that one;
        // otherwise select all uninstalled skills.
        const targetSkill = preSelectedSkillRef.current;
        if (targetSkill) {
          const match = result.skills.find(
            (s) => s.id === targetSkill
          );
          setSelectedSkills(match ? new Set([match.id]) : new Set());
        } else {
          const uninstalled = result.skills
            .filter((s) => !s.already_installed)
            .map((s) => s.id);
          setSelectedSkills(new Set(uninstalled));
        }

        setPhase("selectSkills");
      } catch (e) {
        setErrorMsg(String(e));
        setPhase("error");
      }
    },
    [urlInput]
  );

  // ── Install ────────────────────────────────────────────────────
  const handleInstall = useCallback(async (shouldPack: boolean = false) => {
    // If it's an event (e.g. from onClick), it'll be an object. Check type.
    const pack = typeof shouldPack === "boolean" ? shouldPack : false;
    
    if (!scanResult || selectedSkills.size === 0) return;

    setPhase("installing");
    setProgressMsg(t("githubImportModal.installing", { count: selectedSkills.size }));

    const targets: SkillInstallTarget[] = scanResult.skills
      .filter((s) => selectedSkills.has(s.id))
      .map((s) => ({ id: s.id, folder_path: s.folder_path }));

    try {
      const installed = await invoke<string[]>("install_from_scan", {
        repoUrl: scanResult.source_url,
        source: scanResult.source,
        skills: targets,
      });

      setInstalledCount(installed.length);
      setPhase("completed");
      onInstalled(installed);
      
      if (pack && onPackGroup) {
        onPackGroup(installed);
      }
    } catch (e) {
      setErrorMsg(String(e));
      setPhase("error");
    }
  }, [scanResult, selectedSkills, onInstalled, onPackGroup]);

  // ── Helpers ────────────────────────────────────────────────────
  const toggleSkill = (id: string) => {
    setSelectedSkills((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const reset = () => {
    setPhase("inputURL");
    setUrlInput("");
    setScanResult(null);
    setSelectedSkills(new Set());
    setProgressMsg("");
    setErrorMsg("");
    setInstalledCount(0);
    setShareCodeData(null);
    setShareCodePassword("");
    setShareCodeError("");
    setShareCodeDetected(false);
    onClearShareCode?.();
  };

  const selectAll = (ids?: string[]) => {
    if (!scanResult) return;
    if (ids) {
      setSelectedSkills((prev) => {
        const next = new Set(prev);
        ids.forEach((id) => next.add(id));
        return next;
      });
    } else {
      const all = scanResult.skills
        .filter((s) => !s.already_installed)
        .map((s) => s.id);
      setSelectedSkills(new Set(all));
    }
  };

  const deselectAll = (ids?: string[]) => {
    if (ids) {
      setSelectedSkills((prev) => {
        const next = new Set(prev);
        ids.forEach((id) => next.delete(id));
        return next;
      });
    } else {
      setSelectedSkills(new Set());
    }
  };

  if (!isOpen) return null;

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Modal */}
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-lg z-50"
          >
            <div role="dialog" aria-modal="true" aria-label={t("common.import", { defaultValue: "Import" })} className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              {/* Header */}
              <div className="flex items-center justify-between px-6 pt-4 pb-3 shrink-0 border-b border-border/60">
                <div className="flex items-center gap-2.5">
                  <div className="w-8 h-8 rounded-xl bg-primary/10 flex items-center justify-center">
                    <Download className="w-4 h-4 text-primary" />
                  </div>
                  <h2 className="text-heading-sm">{t("common.import", { defaultValue: "Import" })}</h2>
                </div>
                <button
                  onClick={onClose}
                  aria-label={t("common.close")}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              {/* Body — phase content */}
              <div className="flex-1 overflow-y-auto">
                {phase === "inputURL" && (
                  <InputURLPhase
                    urlInput={urlInput}
                    setUrlInput={handleUrlInputChange}
                    onScan={() => handleScan()}
                    history={history}
                    onSelectHistory={(entry) => {
                      setUrlInput(entry.source);
                      handleScan(entry.source);
                    }}
                    onPickLocalFile={onPickLocalFile}
                    shareCodeDetected={shareCodeDetected}
                  />
                )}

                {phase === "scanning" && (
                  <LoadingPhase message={progressMsg} />
                )}

                {phase === "selectSkills" && scanResult && (
                  <SelectSkillsPhase
                    skills={scanResult.skills}
                    source={scanResult.source}
                    selectedSkills={selectedSkills}
                    onToggle={toggleSkill}
                    onSelectAll={selectAll}
                    onDeselectAll={deselectAll}
                    onInstall={handleInstall}
                    hasPackGroup={!!onPackGroup}
                  />
                )}

                {phase === "installing" && (
                  <LoadingPhase message={progressMsg} />
                )}

                {phase === "completed" && (
                  <CompletedPhase
                    count={installedCount}
                    onDone={onClose}
                  />
                )}

                {phase === "error" && (
                  <ErrorPhase message={errorMsg} onRetry={reset} />
                )}

                {phase === "shareCodePreview" && (
                  <ShareCodePreviewPhase
                    data={shareCodeData}
                    error={shareCodeError}
                    password={shareCodePassword}
                    onPasswordChange={setShareCodePassword}
                    onRetryWithPassword={() => handleParseShareCode(urlInput.trim())}
                    onInstall={handleShareCodeInstall}
                    onBack={reset}
                  />
                )}

                {phase === "shareCodeInstalling" && (
                  <LoadingPhase message={progressMsg} />
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


