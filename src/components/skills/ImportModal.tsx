import { useState, useEffect, useCallback, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  X,
  GitBranch,
  Search,
  Check,
  Loader2,
  CheckCircle2,
  AlertTriangle,
  Clock,
  Download,
  RotateCcw,
  Package,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { cn } from "../../lib/utils";
import type {
  DiscoveredSkill,
  ScanResult,
  SkillInstallTarget,
  RepoHistoryEntry,
} from "../../types";

type Phase =
  | "inputURL"
  | "scanning"
  | "selectSkills"
  | "installing"
  | "completed"
  | "error";

export interface ImportModalProps {
  open: boolean;
  onClose: () => void;
  onInstalled: (installedNames: string[]) => void;
  /** Pre-fill URL and auto-scan (for Marketplace Install flow) */
  initialUrl?: string;
  autoScan?: boolean;
  /** When set, only pre-select this specific skill after scan (instead of all) */
  preSelectedSkill?: string;
  /** Callback to trigger local file (.agentskill) import flow */
  onPickLocalFile?: () => void;
  /** Callback to pack installed skills into a deck immediately */
  onPackGroup?: (skillNames: string[]) => void;
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
}: ImportModalProps) {
  const { t } = useTranslation();

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

  // ── Reset on open ──────────────────────────────────────────────
  useEffect(() => {
    if (isOpen) {
      setPhase("inputURL");
      setUrlInput(initialUrl || "");
      setScanResult(null);
      setSelectedSkills(new Set());
      setProgressMsg("");
      setErrorMsg("");
      setInstalledCount(0);
      // Load history
      invoke<RepoHistoryEntry[]>("list_repo_history")
        .then(setHistory)
        .catch(() => setHistory([]));

      // Auto-scan if initialUrl is provided
      if (initialUrl && autoScan) {
        // Slight delay to allow modal animation
        setTimeout(() => handleScan(initialUrl), 100);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

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
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Modal */}
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-lg z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              {/* Header */}
              <div className="flex items-center justify-between px-6 pt-4 pb-3 shrink-0 border-b border-border/60">
                <div className="flex items-center gap-2.5">
                  <div className="w-8 h-8 rounded-xl bg-blue-500/10 flex items-center justify-center">
                    <Download className="w-4 h-4 text-blue-500" />
                  </div>
                  <h2 className="text-heading-sm">{t("common.import", { defaultValue: "导入" })}</h2>
                </div>
                <button
                  onClick={onClose}
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
                    setUrlInput={setUrlInput}
                    onScan={() => handleScan()}
                    history={history}
                    onSelectHistory={(entry) => {
                      setUrlInput(entry.source);
                      handleScan(entry.source);
                    }}
                    onPickLocalFile={onPickLocalFile}
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
              </div>
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

// ── Phase Components ──────────────────────────────────────────────

function InputURLPhase({
  urlInput,
  setUrlInput,
  onScan,
  history,
  onSelectHistory,
  onPickLocalFile,
}: {
  urlInput: string;
  setUrlInput: (v: string) => void;
  onScan: () => void;
  history: RepoHistoryEntry[];
  onSelectHistory: (entry: RepoHistoryEntry) => void;
  onPickLocalFile?: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="px-6 py-6 space-y-5">
      {/* Illustration */}
      <div className="flex flex-col items-center gap-3 py-2">
        <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-blue-500/15 to-violet-500/15 flex items-center justify-center">
          <Download className="w-7 h-7 text-blue-500/80" />
        </div>
        <p className="text-sm text-muted-foreground text-center">
          {t("githubImportModal.description")}
        </p>
      </div>

      {/* URL input */}
      <div className="flex items-center gap-2.5">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground/80" />
          <Input
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            placeholder={t("githubImportModal.placeholder")}
            className="h-11 rounded-2xl border-border/80 bg-background/80 pl-9 placeholder:text-muted-foreground/80 shadow-inner"
            onKeyDown={(e) => {
              if (e.key === "Enter" && urlInput.trim()) onScan();
            }}
          />
        </div>
        <Button
          size="sm"
          onClick={onScan}
          disabled={!urlInput.trim()}
          className="h-11 min-w-[108px] rounded-2xl border border-primary/40 bg-primary text-primary-foreground px-4 shadow-[0_10px_24px_-12px_rgba(59,130,246,0.85)] hover:bg-primary-hover"
        >
          <Search className="w-3.5 h-3.5 mr-1.5" />
          {t("githubImportModal.scan")}
        </Button>
      </div>

      {onPickLocalFile && (
        <div className="space-y-4 pt-1">
          <div className="flex items-center gap-4">
            <div className="flex-1 h-px bg-border/60"></div>
            <span className="text-[11px] text-muted-foreground font-medium uppercase tracking-wider">{t("common.or", "OR")}</span>
            <div className="flex-1 h-px bg-border/60"></div>
          </div>
          <Button
            variant="outline"
            className="w-full h-11 rounded-2xl border-dashed border-border hover:border-primary/40 hover:bg-primary/5 transition-all text-muted-foreground hover:text-foreground cursor-pointer shadow-sm"
            onClick={onPickLocalFile}
          >
            <Package className="w-4 h-4 mr-2" />
            {t("importBundleModal.pickFile", { defaultValue: "Import from Local File (.agentskill)" })}
          </Button>
        </div>
      )}

      {/* History */}
      {history.length > 0 && (
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground font-medium uppercase tracking-wider">
            {t("githubImportModal.recentRepos")}
          </p>
          <div className="max-h-36 overflow-y-auto rounded-lg space-y-0.5">
            {history.map((entry) => (
              <button
                key={entry.source}
                onClick={() => onSelectHistory(entry)}
                className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-muted transition-colors text-left cursor-pointer group"
              >
                <Clock className="w-3.5 h-3.5 text-muted-foreground shrink-0 group-hover:text-foreground transition-colors" />
                <span className="text-sm truncate">{entry.source}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function LoadingPhase({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center justify-center py-16 gap-4">
      <div className="relative">
        <div className="w-12 h-12 rounded-2xl bg-blue-500/10 flex items-center justify-center">
          <Loader2 className="w-6 h-6 text-blue-500 animate-spin" />
        </div>
      </div>
      <p className="text-sm text-muted-foreground">{message}</p>
    </div>
  );
}

function SelectSkillsPhase({
  skills,
  source,
  selectedSkills,
  onToggle,
  onSelectAll,
  onDeselectAll,
  onInstall,
  hasPackGroup,
}: {
  skills: DiscoveredSkill[];
  source: string;
  selectedSkills: Set<string>;
  onToggle: (id: string) => void;
  onSelectAll: (ids?: string[]) => void;
  onDeselectAll: (ids?: string[]) => void;
  onInstall: (pack?: boolean) => void;
  hasPackGroup?: boolean;
}) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState("");

  const filteredSkills = skills.filter(
    (s) =>
      s.id.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (s.description?.toLowerCase() || "").includes(searchQuery.toLowerCase())
  );

  const installableFiltered = filteredSkills.filter((s) => !s.already_installed);
  const installableCount = installableFiltered.length;
  const allSelected =
    filteredSkills.length > 0 &&
    (installableCount > 0
      ? installableFiltered.every((s) => selectedSkills.has(s.id))
      : filteredSkills.every((s) => selectedSkills.has(s.id)));

  const handleSelectAll = () => {
    if (allSelected) {
      onDeselectAll(filteredSkills.map((s) => s.id));
    } else {
      const targets = installableCount > 0 ? installableFiltered : filteredSkills;
      onSelectAll(targets.map((s) => s.id));
    }
  };

  return (
    <div className="flex flex-col">
      {/* Source info */}
      <div className="px-6 pt-4 pb-2 space-y-3 shrink-0">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <GitBranch className="w-3.5 h-3.5 text-muted-foreground" />
            <span className="text-xs text-muted-foreground font-medium">
              {source}
            </span>
            <span className="text-[10px] bg-muted px-1.5 py-0.5 rounded-md text-muted-foreground/80">
              {skills.length} skill{skills.length !== 1 ? "s" : ""}
            </span>
          </div>
          <div className="flex items-center gap-3">
            {hasPackGroup && selectedSkills.size > 0 && (
              <button
                onClick={() => onInstall(true)}
                className="text-xs text-amber-500 bg-amber-500/10 hover:bg-amber-500/20 px-2 py-1 rounded-md transition-colors flex items-center gap-1 cursor-pointer font-medium"
              >
                <Package className="w-3.5 h-3.5" />
                {t("githubImportModal.quickPack", "快速打包")}
              </button>
            )}
            <button
              onClick={handleSelectAll}
              className="text-xs text-primary hover:text-primary/80 transition-colors cursor-pointer"
            >
              {allSelected ? t("common.deselectAll", "反选") : t("common.selectAll", "全选")}
            </button>
          </div>
        </div>

        {/* Search */}
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/80" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("common.search", "Search skills...")}
            className="h-8 text-xs rounded-lg border-border/80 bg-background/50 pl-8 placeholder:text-muted-foreground/80 shadow-inner"
          />
        </div>
      </div>

      {/* Skill list */}
      <div className="px-6 pb-2 max-h-[38vh] overflow-y-auto">
        <div className="space-y-0.5">
          {filteredSkills.length === 0 && (
            <div className="py-8 text-center text-xs text-muted-foreground">
              {t("common.noResults", "No skills found")}
            </div>
          )}
          {filteredSkills.map((skill) => {
            const isInstalled = skill.already_installed;
            const isSelected = selectedSkills.has(skill.id);
            // Use folder_path as key since id may repeat across agent dirs
            const uniqueKey = skill.folder_path || skill.id;

            return (
              <motion.div
                key={uniqueKey}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.15 }}
                className={cn(
                  "w-full flex items-center justify-between px-3 py-2 rounded-xl text-left transition-all group",
                  isSelected
                    ? "bg-primary/5"
                    : "hover:bg-muted"
                )}
              >
                <div
                  onClick={() => onToggle(skill.id)}
                  className="flex items-center gap-3 flex-1 min-w-0 cursor-pointer py-0.5"
                >
                  {/* Checkbox */}
                  <div
                    className={cn(
                      "w-4 h-4 rounded border-[1.5px] flex items-center justify-center shrink-0 transition-all",
                      isSelected
                        ? "bg-primary border-primary"
                        : isInstalled
                          ? "bg-emerald-500/20 border-emerald-500/40"
                          : "border-muted-foreground/30"
                    )}
                  >
                    {(isSelected || (isInstalled && !isSelected)) && (
                      <Check
                        className={cn(
                          "w-2.5 h-2.5",
                          isSelected ? "text-white" : "text-emerald-500"
                        )}
                        strokeWidth={3}
                      />
                    )}
                  </div>

                  {/* Info */}
                  <div className="flex-1 min-w-0 pr-4">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          "text-[13px] font-medium truncate",
                          isSelected ? "text-primary" : "text-foreground"
                        )}
                      >
                        {skill.id}
                      </span>
                      {isInstalled && !isSelected && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-emerald-500/10 text-emerald-600 font-medium shrink-0">
                          {t("githubImportModal.installed")}
                        </span>
                      )}
                      {isSelected && isInstalled && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-amber-500/10 text-amber-600 font-medium shrink-0">
                          Reinstall
                        </span>
                      )}
                    </div>
                    {skill.description && (
                      <p className="text-xs text-muted-foreground truncate mt-0.5">
                        {skill.description}
                      </p>
                    )}
                  </div>
                </div>

                {/* Right side actions */}
                {isInstalled && (
                  <Button
                    variant={isSelected ? "secondary" : "ghost"}
                    size="sm"
                    className={cn(
                      "h-7 text-[11px] px-2.5 transition-opacity whitespace-nowrap cursor-pointer",
                      !isSelected && "opacity-0 group-hover:opacity-100"
                    )}
                    onClick={() => onToggle(skill.id)}
                  >
                    <RotateCcw className="w-3 h-3" />
                  </Button>
                )}
              </motion.div>
            );
          })}
        </div>
      </div>

      {/* Install bar */}
      <div className="px-6 py-3.5 border-t border-border/60 flex items-center justify-between">
        <span className="text-xs text-muted-foreground">
          {t("githubImportModal.selected", { count: selectedSkills.size })}
        </span>
        <div className="flex items-center gap-2">
          {/* Reinstall All button when every skill is already installed */}
          {skills.every((s) => s.already_installed) && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                onSelectAll(skills.map((s) => s.id));
              }}
              className="text-xs px-3"
            >
              <RotateCcw className="w-3 h-3 mr-1.5" />
              {t("githubImportModal.reinstallAll", "Reinstall All")}
            </Button>
          )}
          <Button
            size="sm"
            onClick={() => onInstall(false)}
            disabled={selectedSkills.size === 0}
            className="px-5"
          >
            <Download className="w-3.5 h-3.5 mr-1.5" />
            {t("githubImportModal.install")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function CompletedPhase({
  count,
  onDone,
}: {
  count: number;
  onDone: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-14 gap-4 px-6">
      <div className="w-14 h-14 rounded-2xl bg-emerald-500/10 flex items-center justify-center">
        <CheckCircle2 className="w-7 h-7 text-emerald-500" />
      </div>
      <div className="text-center space-y-1">
        <h3 className="text-heading-sm">{t("githubImportModal.titleComplete")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("githubImportModal.descComplete", { count })}
        </p>
      </div>
      <div className="flex gap-2 mt-2">
        <Button size="sm" onClick={onDone}>
          {t("githubImportModal.done")}
        </Button>
      </div>
    </div>
  );
}

function ErrorPhase({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center py-14 gap-4 px-6">
      <div className="w-14 h-14 rounded-2xl bg-amber-500/10 flex items-center justify-center">
        <AlertTriangle className="w-7 h-7 text-amber-500" />
      </div>
      <div className="text-center space-y-1">
        <h3 className="text-heading-sm">{t("githubImportModal.somethingWrong")}</h3>
        <p className="text-sm text-muted-foreground max-w-xs">{message}</p>
      </div>
      <Button variant="ghost" size="sm" onClick={onRetry} className="mt-1">
        <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
        {t("githubImportModal.tryAgain")}
      </Button>
    </div>
  );
}
