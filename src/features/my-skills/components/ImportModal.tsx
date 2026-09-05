import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { Download } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { tauriInvoke } from "../../../lib/ipc";
import {
  extractShareCode,
  looksLikeShareCode,
  parseShareCode,
  type ShareCodeData,
  type ShareCodeType,
} from "../../../lib/shareCode";
import type {
  GitOperationProgress,
  RepoHistoryEntry,
  ScanResult,
  ShareCodeSkillInput,
  SkillInstallTarget,
} from "../../../types";
import { deckNameFromRepoSource } from "../lib/deckNameFromRepoSource";
import {
  CompletedPhase,
  ErrorPhase,
  InputURLPhase,
  LoadingPhase,
  SelectSkillsPhase,
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

/** Summary shape expected by {@link CompletedPhase}. */
interface ShareCodeInstallSummary {
  requestedCount: number;
  existingNames: string[];
  installedNames: string[];
  skipped: ShareCodeSkippedSkill[];
}

function normalizeSkillName(name: string): string {
  return name.trim().toLowerCase();
}

function gitOperationErrorMessage(
  error: unknown,
  t: (key: string, options?: { defaultValue: string }) => string,
): string {
  const raw = String(error);
  const messages: Array<[string, string]> = [
    ["token_expired:", "Your GitHub session expired. Refresh it in Settings, then retry."],
    ["not_authenticated:", "Sign in to GitHub in Settings, then retry this private repository."],
    ["credential_unavailable:", "Unlock the system credential store, then retry."],
    ["unauthorized:", "The signed-in GitHub user does not have access to this repository."],
    ["app_not_installed:", "Install or authorize the SkillStar GitHub App for this repository, then retry."],
    ["network:", "GitHub could not be reached. Check the SkillStar proxy and your network, then retry."],
    ["cancelled:", "The repository operation was cancelled."],
    ["unsafe_remote:", "Remove credentials from the repository URL and use SkillStar GitHub login instead."],
  ];
  const match = messages.find(([code]) => raw.includes(code));
  return match ? t(`githubImportModal.gitError.${match[0].slice(0, -1)}`, { defaultValue: match[1] }) : raw;
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
  /** Callback to pack installed skills into a deck immediately.
   *  `defaultName` is the repository name after the slash (`owner/repo` → `repo`). */
  onPackGroup?: (skillNames: string[], defaultName: string) => void;
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

  // Keep a ref so handleScan always reads the latest value
  // (avoids stale closure when called from setTimeout)
  const preSelectedSkillRef = useRef(preSelectedSkill);
  preSelectedSkillRef.current = preSelectedSkill;
  const activeGitSessionRef = useRef<string | null>(null);

  // ── State ──────────────────────────────────────────────────────
  const [phase, setPhase] = useState<Phase>("inputURL");
  const [urlInput, setUrlInput] = useState("");
  const [fullDepthScan, setFullDepthScan] = useState(false);
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

  useEffect(() => {
    if (!isOpen) return;
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<GitOperationProgress>("skillstar://git-progress", ({ payload }) => {
      if (payload.session_id !== activeGitSessionRef.current) return;
      const message =
        payload.phase === "preparing"
          ? t("githubImportModal.gitPreparing", { defaultValue: "Preparing secure repository access..." })
          : payload.phase === "running"
            ? t("githubImportModal.gitRunning", { defaultValue: "Downloading repository data..." })
            : payload.phase === "cancelled"
              ? t("githubImportModal.gitCancelled", { defaultValue: "Cancelling repository operation..." })
              : null;
      if (message) setProgressMsg(message);
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stopListening = unlisten;
      })
      .catch(() => {
        // Browser development mode has no native event bus; command mocks still
        // provide deterministic scan/install behavior.
      });
    return () => {
      disposed = true;
      stopListening?.();
    };
  }, [isOpen, t]);

  const cancelActiveGitOperation = useCallback(() => {
    const sessionId = activeGitSessionRef.current;
    if (!sessionId) return;
    activeGitSessionRef.current = null;
    void tauriInvoke("cancel_git_operation", { sessionId });
  }, []);

  const handleClose = useCallback(() => {
    cancelActiveGitOperation();
    onClose();
  }, [cancelActiveGitOperation, onClose]);

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
      setFullDepthScan(false);

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
      tauriInvoke("list_repo_history")
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
      const installed = await tauriInvoke("list_skills");
      return new Set(installed.map((skill) => normalizeSkillName(skill.name)));
    } catch (e) {
      if (import.meta.env.DEV) console.warn("[ShareCode] Failed to list installed skills:", e);
      return new Set<string>();
    }
  }, []);

  const collectExistingNames = useCallback((data: ShareCodeData, installedSet: Set<string>) => {
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
  }, []);

  // ── Share Code Parse ─────────────────────────────────────────
  const handleParseShareCode = useCallback(
    async (code: string) => {
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
    },
    [collectExistingNames, loadInstalledNameSet, t],
  );

  const handleShareCodeInstall = useCallback(async () => {
    if (!shareCodeData) return;
    setPhase("shareCodeInstalling");
    setProgressMsg(t("shareCodeImport.installing"));

    try {
      const payload: ShareCodeSkillInput[] = shareCodeData.s.map((skill) => ({
        n: skill.n,
        u: skill.u ?? "",
        c: skill.c,
        p: skill.p,
      }));

      const summary = await tauriInvoke("install_from_share_code", {
        skills: payload,
      });

      const installedTotal = [...summary.installed_names, ...summary.embedded_names];
      const knownReasons: ReadonlyArray<ShareCodeSkipReason> = ["repo_missing", "no_source", "install_failed"];
      const skipReason = (raw: string): ShareCodeSkipReason =>
        (knownReasons as ReadonlyArray<string>).includes(raw) ? (raw as ShareCodeSkipReason) : "install_failed";

      setShareCodeSummary({
        requestedCount: summary.requested_count,
        existingNames: summary.existing_names,
        installedNames: installedTotal,
        skipped: summary.skipped.map((entry) => ({
          name: entry.name,
          reason: skipReason(entry.reason),
        })),
      });
      setShareCodeExistingNames(summary.existing_names);
      setInstalledCount(installedTotal.length);
      setPhase("completed");
      if (installedTotal.length > 0) onInstalled(installedTotal);

      // Deck share code → auto-create a local group entry for convenience.
      if (shareCodeType === "deck" && shareCodeData.n) {
        const sources: Record<string, string> = {};
        for (const entry of shareCodeData.s) {
          if (entry.u) sources[entry.n] = entry.u;
        }
        try {
          await tauriInvoke("create_skill_group", {
            name: shareCodeData.n,
            description: shareCodeData.d || "",
            icon: shareCodeData.i || "📦",
            skills: shareCodeData.s.map((entry) => entry.n).filter(Boolean),
            skillSources: sources,
          });
        } catch (e) {
          if (import.meta.env.DEV) console.warn("[ShareCode] Failed to auto-create deck:", e);
        }
      }

      onClearShareCode?.();
    } catch (e) {
      setErrorMsg(String(e));
      setPhase("error");
    }
  }, [onClearShareCode, onInstalled, shareCodeData, shareCodeType, t]);

  // ── Smart input handler: detect share code in URL input ──────
  const handleUrlInputChange = useCallback(
    (value: string) => {
      setUrlInput(value);
      // Auto-detect share code when pasted (handle formatted messages too)
      const extracted = extractShareCode(value.trim());
      if (looksLikeShareCode(extracted)) {
        setTimeout(() => handleParseShareCode(value.trim()), 50);
      }
    },
    [handleParseShareCode],
  );

  // ── Scan ───────────────────────────────────────────────────────
  const handleScan = useCallback(
    async (url?: string, scanDepthOverride?: boolean) => {
      const input = (url || urlInput).trim();
      if (!input) return;
      const useFullDepth = scanDepthOverride ?? fullDepthScan;
      const sessionId = crypto.randomUUID();
      activeGitSessionRef.current = sessionId;

      setPhase("scanning");
      setProgressMsg(t("githubImportModal.cloning"));

      try {
        const result = await tauriInvoke("scan_github_repo", {
          url: input,
          fullDepth: useFullDepth,
          sessionId,
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
          const match = result.skills.find((s) => s.id === targetSkill);
          setSelectedSkills(match?.installable ? new Set([match.id]) : new Set());
        } else {
          const uninstalled = result.skills.filter((s) => s.installable && !s.already_installed).map((s) => s.id);
          setSelectedSkills(new Set(uninstalled));
        }

        setPhase("selectSkills");
      } catch (e) {
        setErrorMsg(gitOperationErrorMessage(e, t));
        setPhase("error");
      } finally {
        if (activeGitSessionRef.current === sessionId) activeGitSessionRef.current = null;
      }
    },
    [fullDepthScan, t, urlInput],
  );

  const handleDeepScan = useCallback(() => {
    if (!scanResult) return;
    setFullDepthScan(true);
    void handleScan(scanResult.source_url, true);
  }, [handleScan, scanResult]);

  // ── Adopt from local folder ───────────────────────────────────
  const handlePickLocalFolder = useCallback(async () => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: t("importModal.adoptFolderTitle", { defaultValue: "Pick a folder containing SKILL.md" }),
      });
      if (!selected) return;
      const folderPath = typeof selected === "string" ? selected : selected[0];
      if (!folderPath) return;

      setPhase("installing");
      setProgressMsg(t("importModal.adoptingFolder", { defaultValue: "Adopting local skills..." }));

      const result = await tauriInvoke("adopt_local_folder", {
        folderPath,
      });

      setInstalledCount(result.adopted.length);
      setPhase("completed");
      if (result.adopted.length > 0) {
        onInstalled(result.adopted.map((s) => s.name));
      }
    } catch (e) {
      setErrorMsg(String(e));
      setPhase("error");
    }
  }, [onInstalled, t]);

  // ── Install ────────────────────────────────────────────────────
  const handleInstall = useCallback(
    async (shouldPack: boolean = false) => {
      // If it's an event (e.g. from onClick), it'll be an object. Check type.
      const pack = typeof shouldPack === "boolean" ? shouldPack : false;

      if (!scanResult || selectedSkills.size === 0) return;

      setShareCodeSummary(null);
      setPhase("installing");
      setProgressMsg(t("githubImportModal.installing", { count: selectedSkills.size }));

      const targets: SkillInstallTarget[] = scanResult.skills
        .filter((s) => s.installable && selectedSkills.has(s.id))
        .map((s) => ({ id: s.id, folder_path: s.folder_path }));
      const sessionId = crypto.randomUUID();
      activeGitSessionRef.current = sessionId;

      try {
        const installed = await tauriInvoke("install_from_scan", {
          repoUrl: scanResult.source_url,
          source: scanResult.source,
          skills: targets,
          sessionId,
        });

        setInstalledCount(installed.length);
        setPhase("completed");
        onInstalled(installed);

        if (pack && onPackGroup) {
          onPackGroup(installed, deckNameFromRepoSource(scanResult.source));
        }
      } catch (e) {
        setErrorMsg(gitOperationErrorMessage(e, t));
        setPhase("error");
      } finally {
        if (activeGitSessionRef.current === sessionId) activeGitSessionRef.current = null;
      }
    },
    [scanResult, selectedSkills, onInstalled, onPackGroup, t],
  );

  // ── Helpers ────────────────────────────────────────────────────
  const toggleSkill = (id: string) => {
    if (!scanResult?.skills.find((skill) => skill.id === id)?.installable) return;
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
    setFullDepthScan(false);
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
      const installable = new Set(scanResult.skills.filter((skill) => skill.installable).map((skill) => skill.id));
      setSelectedSkills((prev) => {
        const next = new Set(prev);
        ids.filter((id) => installable.has(id)).forEach((id) => next.add(id));
        return next;
      });
    } else {
      const all = scanResult.skills.filter((s) => s.installable && !s.already_installed).map((s) => s.id);
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

  return (
    <ModalShell
      open={isOpen}
      onClose={handleClose}
      ariaLabel={t("common.import", { defaultValue: "Import" })}
      panelClassName="max-w-lg"
    >
      {/* Header */}
      <ModalHeader
        icon={<Download className="w-4 h-4 text-primary" />}
        title={t("common.import", { defaultValue: "Import" })}
        onClose={handleClose}
      />

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
            onPickLocalFolder={handlePickLocalFolder}
            shareCodeDetected={shareCodeDetected}
          />
        )}

        {phase === "scanning" && <LoadingPhase message={progressMsg} onCancel={cancelActiveGitOperation} />}

        {phase === "selectSkills" && scanResult && (
          <SelectSkillsPhase
            skills={scanResult.skills}
            source={scanResult.source}
            selectedSkills={selectedSkills}
            onToggle={toggleSkill}
            onSelectAll={selectAll}
            onDeselectAll={deselectAll}
            onInstall={handleInstall}
            fullDepthEnabled={fullDepthScan}
            onDeepScan={handleDeepScan}
            hasPackGroup={!!onPackGroup}
          />
        )}

        {phase === "installing" && <LoadingPhase message={progressMsg} onCancel={cancelActiveGitOperation} />}

        {phase === "completed" && (
          <CompletedPhase count={installedCount} summary={shareCodeSummary} onDone={handleClose} />
        )}

        {phase === "error" && <ErrorPhase message={errorMsg} onRetry={reset} />}

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

        {phase === "shareCodeInstalling" && <LoadingPhase message={progressMsg} />}
      </div>
    </ModalShell>
  );
}
