import { useState, useEffect, useCallback, useRef } from "react";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { X, Download } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { looksLikeShareCode, parseShareCode, extractShareCode, type ShareCodeData, type ShareCodeType } from "../../../lib/shareCode";
import type {
  ScanResult,
  SkillInstallTarget,
  RepoHistoryEntry,
  Skill,
} from "../../../types";
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

type ShareCodeSkipReason = "repo_missing" | "no_source" | "install_failed";

interface ShareCodeSkippedSkill {
  name: string;
  reason: ShareCodeSkipReason;
}

interface ShareCodeInstallSummary {
  requestedCount: number;
  existingNames: string[];
  installedNames: string[];
  skipped: ShareCodeSkippedSkill[];
}

function normalizeSkillName(name: string): string {
  return name.trim().toLowerCase();
}

function decodeShareSkillContent(encoded: string): string {
  const binaryStr = atob(encoded);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}

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
  const [shareCodeExistingNames, setShareCodeExistingNames] = useState<string[]>([]);
  const [shareCodeSummary, setShareCodeSummary] = useState<ShareCodeInstallSummary | null>(null);
  const [shareCodeType, setShareCodeType] = useState<ShareCodeType>("skills");

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
      setShareCodeExistingNames([]);
      setShareCodeSummary(null);

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

  const loadInstalledNameSet = useCallback(async () => {
    try {
      const installed = await invoke<Skill[]>("list_skills");
      return new Set(installed.map((skill) => normalizeSkillName(skill.name)));
    } catch (e) {
      console.warn("[ShareCode] Failed to list installed skills:", e);
      return new Set<string>();
    }
  }, []);

  const collectExistingNames = useCallback(
    (data: ShareCodeData, installedSet: Set<string>) => {
      const existingNames: string[] = [];
      const seen = new Set<string>();

      for (const skill of data.s) {
        const name = skill.n?.trim();
        if (!name) continue;
        const key = normalizeSkillName(name);
        if (seen.has(key)) continue;
        seen.add(key);
        if (installedSet.has(key)) {
          existingNames.push(name);
        }
      }

      return existingNames;
    },
    [],
  );

  // ── Share Code Parse ─────────────────────────────────────────
  const handleParseShareCode = useCallback(async (code: string) => {
    setShareCodeError("");
    setShareCodeSummary(null);
    setProgressMsg(t("shareCodeImport.parsing"));
    try {
      // Extract raw share code from formatted message or use as-is
      const rawCode = extractShareCode(code);
      const { data, type } = await parseShareCode(rawCode);
      setShareCodeData(data);
      setShareCodeType(type);
      const installedSet = await loadInstalledNameSet();
      setShareCodeExistingNames(collectExistingNames(data, installedSet));
      setPhase("shareCodePreview");
    } catch (e) {
      const errMsg = String(e);
      if (errMsg.includes("expired")) {
        setShareCodeError(errMsg.replace(/^Error:\s*/, ""));
        setShareCodeData(null);
        setShareCodeExistingNames([]);
      } else {
        setShareCodeError(errMsg);
        setPhase("shareCodePreview");
        setShareCodeData(null);
        setShareCodeExistingNames([]);
      }
    }
  }, [collectExistingNames, loadInstalledNameSet, t]);

  const handleShareCodeInstall = useCallback(async () => {
    if (!shareCodeData) return;
    setPhase("shareCodeInstalling");
    setProgressMsg(t("shareCodeImport.installing"));
    const installedNames: string[] = [];
    const existingNames: string[] = [];
    const skipped: ShareCodeSkippedSkill[] = [];
    const installedSeen = new Set<string>();
    const existingSeen = new Set<string>();
    const skippedSeen = new Set<string>();

    const pushInstalled = (name: string) => {
      const key = normalizeSkillName(name);
      if (installedSeen.has(key)) return;
      installedSeen.add(key);
      installedNames.push(name);
    };

    const pushExisting = (name: string) => {
      const key = normalizeSkillName(name);
      if (existingSeen.has(key)) return;
      existingSeen.add(key);
      existingNames.push(name);
    };

    const pushSkipped = (name: string, reason: ShareCodeSkipReason) => {
      const key = `${normalizeSkillName(name)}::${reason}`;
      if (skippedSeen.has(key)) return;
      skippedSeen.add(key);
      skipped.push({ name, reason });
    };

    const installedNameSet = await loadInstalledNameSet();

    try {
      for (const skill of shareCodeData.s) {
        const skillName = skill.n?.trim();
        if (!skillName) continue;
        const skillKey = normalizeSkillName(skillName);

        // Already installed locally -> skip installation, keep explicit summary.
        if (installedNameSet.has(skillKey)) {
          pushExisting(skillName);
          continue;
        }

        const installEmbedded = async () => {
          if (!skill.c) return false;
          try {
            const content = decodeShareSkillContent(skill.c);
            await invoke("create_local_skill_from_content", { name: skillName, content });
            installedNameSet.add(skillKey);
            pushInstalled(skillName);
            return true;
          } catch (e) {
            console.warn(`[ShareCode] Failed to create embedded skill ${skillName}:`, e);
            return false;
          }
        };

        if (skill.u) {
          // Git-backed skill: use repo scanner flow
          try {
            const result = await invoke<ScanResult>("scan_github_repo", { url: skill.u });
            const target = result.skills.find((s) => normalizeSkillName(s.id) === skillKey);
            if (target) {
              const installed = await invoke<string[]>("install_from_scan", {
                repoUrl: result.source_url,
                source: result.source,
                skills: [{ id: target.id, folder_path: target.folder_path }],
              });
              if (installed.length > 0) {
                for (const name of installed) {
                  installedNameSet.add(normalizeSkillName(name));
                  pushInstalled(name);
                }
                continue;
              }
              // Fallback: use embedded content if provided.
              if (await installEmbedded()) {
                continue;
              }
              pushSkipped(skillName, "install_failed");
              continue;
            }
            // Repo exists, but target skill not found. Fallback to embedded, otherwise skip.
            if (await installEmbedded()) {
              continue;
            }
            pushSkipped(skillName, "repo_missing");
          } catch (e) {
            console.warn(`[ShareCode] Failed to install ${skillName} from ${skill.u}:`, e);
            if (await installEmbedded()) {
              continue;
            }
            pushSkipped(skillName, "install_failed");
          }
        } else if (skill.c) {
          if (!(await installEmbedded())) {
            pushSkipped(skillName, "install_failed");
          }
        } else {
          pushSkipped(skillName, "no_source");
        }
      }

      const summary: ShareCodeInstallSummary = {
        requestedCount: shareCodeData.s.length,
        existingNames,
        installedNames,
        skipped,
      };
      setShareCodeSummary(summary);
      setShareCodeExistingNames(summary.existingNames);
      setInstalledCount(installedNames.length);
      setPhase("completed");
      if (installedNames.length > 0) onInstalled(installedNames);

      // If it's a deck share code, auto-create a group entry
      if (shareCodeType === "deck" && shareCodeData.n) {
        const allSkillNames = shareCodeData.s.map((s) => s.n).filter(Boolean);
        const sources: Record<string, string> = {};
        for (const s of shareCodeData.s) {
          if (s.u) sources[s.n] = s.u;
        }
        try {
          await invoke("create_skill_group", {
            name: shareCodeData.n,
            description: shareCodeData.d || "",
            icon: shareCodeData.i || "📦",
            skills: allSkillNames,
            skillSources: sources,
          });
        } catch (e) {
          // Deck may already exist — not a blocking error
          console.warn("[ShareCode] Failed to auto-create deck:", e);
        }
      }

      onClearShareCode?.();
    } catch (e) {
      setErrorMsg(String(e));
      setPhase("error");
    }
  }, [loadInstalledNameSet, onClearShareCode, onInstalled, shareCodeData, shareCodeType, t]);

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

    setShareCodeSummary(null);
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
    setShareCodeExistingNames([]);
    setShareCodeSummary(null);
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
            <div role="dialog" aria-modal="true" aria-label={t("common.import", { defaultValue: "Import" })} className="modal-surface">
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
                    summary={shareCodeSummary}
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
                    existingNames={shareCodeExistingNames}
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
