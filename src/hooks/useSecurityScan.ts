import {
  createContext,
  createElement,
  useState,
  useCallback,
  useEffect,
  useRef,
  useContext,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  SecurityScanEvent,
  SecurityScanResult,
  SecurityScanTrailItem,
  RiskLevel,
} from "../types";

export type ScanPhase = "idle" | "scanning" | "done";
export type ScanMode = "static" | "smart" | "deep";

export interface SecurityScanActiveChunkFile {
  skillName: string;
  fileName: string;
  chunkCompleted: number;
  chunkTotal: number;
  updatedAt: number;
}

export interface SecurityScanState {
  phase: ScanPhase;
  results: SecurityScanResult[];
  /** All skills currently being scanned (up to 4 concurrent) */
  activeSkills: string[];
  /** First active skill (backward compat convenience) */
  currentSkill: string | null;
  currentMode: ScanMode;
  currentStage: string | null;
  currentFile: string | null;
  syncPulseKey: number;
  scanAngle: number;
  scanStartedAt: number | null;
  recentFiles: SecurityScanTrailItem[];
  scanned: number;
  total: number;
  skillFileProgress: Record<string, { scanned: number; total: number }>;
  skillChunkProgress: Record<string, { completed: number; total: number }>;
  /** Best-effort list of concurrently active chunk files (live worker lanes) */
  activeChunkFiles: SecurityScanActiveChunkFile[];
  activeChunkWorkers: number;
  maxChunkWorkers: number;
  errors: { skillName: string; message: string }[];
  /** Risk level badge lookup: skill_name → RiskLevel */
  riskMap: Record<string, RiskLevel>;
}

interface SecurityScanContextValue extends SecurityScanState {
  progressPercent: number;
  startScan: (skillNames?: string[], force?: boolean, mode?: ScanMode) => Promise<void>;
  resetScan: () => void;
  loadCached: () => Promise<void>;
  clearCache: () => Promise<void>;
  cancelScan: () => Promise<void>;
}

const SecurityScanContext = createContext<SecurityScanContextValue | null>(null);

function pushRecentFile(
  items: SecurityScanTrailItem[],
  fileName: string | null,
  skillName: string | null,
  stage: string | null,
  riskLevel?: RiskLevel,
  reasonLabels?: string[]
): SecurityScanTrailItem[] {
  if (!fileName) return items;

  return [
    { fileName, skillName, stage, riskLevel, reasonLabels, timestamp: Date.now() },
    ...items.filter((item) => item.fileName !== fileName),
  ].slice(0, 3);
}

function getFileRiskFromResult(result: SecurityScanResult | undefined, fileName: string | null): RiskLevel | undefined {
  if (!result || !fileName) return undefined;

  const severities: RiskLevel[] = [
    ...result.static_findings.filter((finding) => finding.file_path === fileName).map((finding) => finding.severity),
    ...result.ai_findings.filter((finding) => finding.file_path === fileName).map((finding) => finding.severity),
  ];

  if (severities.includes("Critical")) return "Critical";
  if (severities.includes("High")) return "High";
  if (severities.includes("Medium")) return "Medium";
  if (severities.includes("Low")) return "Low";
  return undefined;
}

function getFileReasonLabels(result: SecurityScanResult | undefined, fileName: string | null): string[] {
  if (!result || !fileName) return [];

  const labels = new Set<string>();

  for (const finding of result.static_findings) {
    if (finding.file_path !== fileName) continue;

    const pattern = finding.pattern_id.toLowerCase();
    if (pattern.includes("shell") || pattern.includes("command") || pattern.includes("exec")) {
      labels.add("shell");
    }
    if (pattern.includes("network") || pattern.includes("http") || pattern.includes("url")) {
      labels.add("network");
    }
    if (pattern.includes("secret") || pattern.includes("token") || pattern.includes("key")) {
      labels.add("secret");
    }
  }

  for (const finding of result.ai_findings) {
    if (finding.file_path !== fileName) continue;

    const text = `${finding.category} ${finding.description} ${finding.evidence}`.toLowerCase();
    if (text.includes("shell") || text.includes("command") || text.includes("exec")) {
      labels.add("shell");
    }
    if (text.includes("network") || text.includes("request") || text.includes("http") || text.includes("webhook")) {
      labels.add("network");
    }
    if (text.includes("secret") || text.includes("token") || text.includes("credential") || text.includes("api key")) {
      labels.add("secret");
    }
  }

  if (labels.size === 0 && (result.static_findings.some((f) => f.file_path === fileName) || result.ai_findings.some((f) => f.file_path === fileName))) {
    labels.add("review");
  }

  return Array.from(labels).slice(0, 3);
}

function upsertSkillFileProgress(
  progressMap: Record<string, { scanned: number; total: number }>,
  skillName: string | null | undefined,
  scanned?: number,
  total?: number
): Record<string, { scanned: number; total: number }> {
  if (!skillName) return progressMap;

  const current = progressMap[skillName] ?? { scanned: 0, total: 0 };
  const nextScanned =
    typeof scanned === "number" ? Math.max(current.scanned, scanned) : current.scanned;
  const nextTotal = typeof total === "number" ? Math.max(current.total, total) : current.total;

  if (current.scanned === nextScanned && current.total === nextTotal) {
    return progressMap;
  }

  return {
    ...progressMap,
    [skillName]: { scanned: nextScanned, total: nextTotal },
  };
}

function removeSkillFileProgress(
  progressMap: Record<string, { scanned: number; total: number }>,
  skillName: string | null | undefined
): Record<string, { scanned: number; total: number }> {
  if (!skillName || !(skillName in progressMap)) return progressMap;
  const { [skillName]: _removed, ...rest } = progressMap;
  return rest;
}

function upsertSkillChunkProgress(
  progressMap: Record<string, { completed: number; total: number }>,
  skillName: string | null | undefined,
  completed?: number,
  total?: number
): Record<string, { completed: number; total: number }> {
  if (!skillName) return progressMap;

  const current = progressMap[skillName] ?? { completed: 0, total: 0 };
  const nextCompleted =
    typeof completed === "number" ? Math.max(current.completed, completed) : current.completed;
  const nextTotal = typeof total === "number" ? Math.max(current.total, total) : current.total;

  if (current.completed === nextCompleted && current.total === nextTotal) {
    return progressMap;
  }

  return {
    ...progressMap,
    [skillName]: { completed: nextCompleted, total: nextTotal },
  };
}

function removeSkillChunkProgress(
  progressMap: Record<string, { completed: number; total: number }>,
  skillName: string | null | undefined
): Record<string, { completed: number; total: number }> {
  if (!skillName || !(skillName in progressMap)) return progressMap;
  const { [skillName]: _removed, ...rest } = progressMap;
  return rest;
}

const MAX_ACTIVE_CHUNK_FILES = 8;

function upsertActiveChunkFile(
  items: SecurityScanActiveChunkFile[],
  skillName: string | null | undefined,
  fileName: string | null | undefined,
  chunkCompleted: number,
  chunkTotal: number
): SecurityScanActiveChunkFile[] {
  if (!skillName || !fileName) return items;
  const next: SecurityScanActiveChunkFile = {
    skillName,
    fileName,
    chunkCompleted,
    chunkTotal,
    updatedAt: Date.now(),
  };

  const filtered = items.filter(
    (item) => !(item.skillName === skillName && item.fileName === fileName)
  );
  return [next, ...filtered].slice(0, MAX_ACTIVE_CHUNK_FILES);
}

function removeActiveChunkFile(
  items: SecurityScanActiveChunkFile[],
  skillName: string | null | undefined,
  fileName: string | null | undefined
): SecurityScanActiveChunkFile[] {
  if (!skillName || !fileName) return items;
  return items.filter(
    (item) => !(item.skillName === skillName && item.fileName === fileName)
  );
}

function removeActiveChunkFilesForSkill(
  items: SecurityScanActiveChunkFile[],
  skillName: string | null | undefined
): SecurityScanActiveChunkFile[] {
  if (!skillName) return items;
  return items.filter((item) => item.skillName !== skillName);
}

function calculateProgressPercent(
  scanned: number,
  total: number,
  fileProgressMap: Record<string, { scanned: number; total: number }>,
  chunkProgressMap: Record<string, { completed: number; total: number }>
): number {
  if (total <= 0) return 0;

  const completedSkills = Math.min(scanned, total);
  let partialSkills = 0;
  const skillNames = new Set([
    ...Object.keys(fileProgressMap),
    ...Object.keys(chunkProgressMap),
  ]);
  for (const skillName of skillNames) {
    const chunk = chunkProgressMap[skillName];
    if (chunk && chunk.total > 0) {
      partialSkills += Math.min(chunk.completed / chunk.total, 0.99);
      continue;
    }

    const file = fileProgressMap[skillName];
    if (!file || file.total <= 0) continue;
    partialSkills += Math.min(file.scanned / file.total, 0.99);
  }

  const composite = Math.min(total, completedSkills + partialSkills);
  const percent = Math.floor((composite / total) * 100);

  if (completedSkills >= total) return 100;
  if (percent <= 0 && (completedSkills > 0 || partialSkills > 0)) return 1;
  return Math.min(99, Math.max(0, percent));
}

export function SecurityScanProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SecurityScanState>({
    phase: "idle",
    results: [],
    activeSkills: [],
    currentSkill: null,
    currentMode: "smart",
    currentStage: null,
    currentFile: null,
    syncPulseKey: 0,
    scanAngle: 0,
    scanStartedAt: null,
    recentFiles: [],
    scanned: 0,
    total: 0,
    skillFileProgress: {},
    skillChunkProgress: {},
    activeChunkFiles: [],
    activeChunkWorkers: 0,
    maxChunkWorkers: 0,
    errors: [],
    riskMap: {},
  });

  const unlistenRef = useRef<UnlistenFn | null>(null);

  // Load cached results on mount (for badge display)
  const loadCached = useCallback(async () => {
    try {
      const cached = await invoke<SecurityScanResult[]>("get_cached_scan_results");
      if (cached.length === 0) return;
      const riskMap: Record<string, RiskLevel> = {};
      for (const cachedResult of cached) {
        riskMap[cachedResult.skill_name] = cachedResult.risk_level;
      }
      setState((prev) => ({
        ...prev,
        results: cached,
        riskMap,
        phase: "done",
        scanned: cached.length,
        total: cached.length,
        skillFileProgress: {},
        skillChunkProgress: {},
        activeChunkFiles: [],
        activeChunkWorkers: 0,
        maxChunkWorkers: 0,
      }));
    } catch {
      // Silently ignore — cache may not exist yet
    }
  }, []);

  useEffect(() => {
    loadCached();
  }, [loadCached]);

  // Start scan
  const startScan = useCallback(
    async (skillNames: string[] = [], force: boolean = false, mode: ScanMode = "smart") => {
      const requestId = `scan-${Date.now()}`;
      const targetSet = skillNames.length > 0 ? new Set(skillNames) : null;

      setState((prev) => ({
        ...prev,
        phase: "scanning",
        activeSkills: [],
        currentSkill: null,
        currentMode: mode,
        currentStage: null,
        currentFile: null,
        syncPulseKey: prev.syncPulseKey + 1,
        scanAngle: 0,
        scanStartedAt: Date.now(),
        recentFiles: [],
        scanned: 0,
        total: 0,
        skillFileProgress: {},
        skillChunkProgress: {},
        activeChunkFiles: [],
        activeChunkWorkers: 0,
        maxChunkWorkers: 0,
        errors: [],
        results: targetSet
          ? prev.results.filter((result) => !targetSet.has(result.skill_name))
          : [],
        riskMap: targetSet
          ? Object.fromEntries(
              Object.entries(prev.riskMap).filter(([name]) => !targetSet.has(name))
            )
          : {},
      }));

      // Listen for scan events
      if (unlistenRef.current) {
        unlistenRef.current();
      }

      unlistenRef.current = await listen<SecurityScanEvent>(
        "ai://security-scan",
        (event) => {
          const payload = event.payload;
          if (payload.requestId !== requestId) return;

          switch (payload.event) {
            case "skill-start": {
              const skillName = payload.skillName || null;
              const stage = payload.phase || payload.message || "collect";
              setState((prev) => {
                const newActive = skillName && !prev.activeSkills.includes(skillName)
                  ? [...prev.activeSkills, skillName]
                  : prev.activeSkills;
                const nextSkillFileProgress = upsertSkillFileProgress(
                  prev.skillFileProgress,
                  skillName,
                  payload.skillFileScanned,
                  payload.skillFileTotal
                );
                const nextSkillChunkProgress = upsertSkillChunkProgress(
                  prev.skillChunkProgress,
                  skillName,
                  payload.skillChunkCompleted,
                  payload.skillChunkTotal
                );
                const nextActiveChunkFiles = removeActiveChunkFilesForSkill(
                  prev.activeChunkFiles,
                  skillName
                );
                return {
                  ...prev,
                  activeSkills: newActive,
                  currentSkill: skillName ?? newActive[0] ?? null,
                  currentStage: stage,
                  currentFile: null,
                  syncPulseKey: prev.syncPulseKey + 1,
                  scanAngle: prev.scanAngle,
                  scanStartedAt: prev.scanStartedAt ?? Date.now(),
                  scanned: payload.scanned ?? prev.scanned,
                  total: payload.total ?? prev.total,
                  skillFileProgress: nextSkillFileProgress,
                  skillChunkProgress: nextSkillChunkProgress,
                  activeChunkFiles: nextActiveChunkFiles,
                  activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                  maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
                };
              });
              break;
            }

            case "file-start":
              setState((prev) => {
                const nextFile = payload.fileName || null;
                const stage = payload.phase || payload.message || prev.currentStage;
                // Smooth rotation: advance by a fixed step per file event (wraps at 360)
                const nextAngle = (prev.scanAngle + 17) % 360;
                return {
                  ...prev,
                  currentSkill: payload.skillName ?? prev.currentSkill,
                  currentStage: stage,
                  currentFile: nextFile,
                  scanAngle: nextAngle,
                  skillFileProgress: upsertSkillFileProgress(
                    prev.skillFileProgress,
                    payload.skillName,
                    payload.skillFileScanned,
                    payload.skillFileTotal
                  ),
                  skillChunkProgress: upsertSkillChunkProgress(
                    prev.skillChunkProgress,
                    payload.skillName,
                    payload.skillChunkCompleted,
                    payload.skillChunkTotal
                  ),
                  activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                  maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
                  recentFiles:
                    nextFile !== prev.currentFile
                      ? pushRecentFile(prev.recentFiles, prev.currentFile, prev.currentSkill, prev.currentStage)
                      : prev.recentFiles,
                };
              });
              break;

            case "progress": {
              const stage = payload.phase || payload.message || null;
              const progressSkill = payload.skillName ?? null;
              setState((prev) => {
                const previousSkillChunkCompleted = progressSkill
                  ? prev.skillChunkProgress[progressSkill]?.completed ?? 0
                  : 0;
                const reportedChunkCompleted =
                  typeof payload.skillChunkCompleted === "number"
                    ? payload.skillChunkCompleted
                    : previousSkillChunkCompleted;
                const reportedChunkTotal =
                  typeof payload.skillChunkTotal === "number"
                    ? payload.skillChunkTotal
                    : progressSkill
                    ? prev.skillChunkProgress[progressSkill]?.total ?? 0
                    : 0;
                const nextSkillFileProgress = upsertSkillFileProgress(
                  prev.skillFileProgress,
                  payload.skillName,
                  payload.skillFileScanned,
                  payload.skillFileTotal
                );
                const nextSkillChunkProgress = upsertSkillChunkProgress(
                  prev.skillChunkProgress,
                  payload.skillName,
                  payload.skillChunkCompleted,
                  payload.skillChunkTotal
                );

                let nextActiveChunkFiles = prev.activeChunkFiles;
                const isAiAnalyze = stage === "ai-analyze" || stage === "ai";
                const hasFileName = Boolean(payload.fileName);

                if (isAiAnalyze && hasFileName && progressSkill) {
                  const isCompletionTick =
                    reportedChunkCompleted > previousSkillChunkCompleted;
                  nextActiveChunkFiles = isCompletionTick
                    ? removeActiveChunkFile(nextActiveChunkFiles, progressSkill, payload.fileName)
                    : upsertActiveChunkFile(
                        nextActiveChunkFiles,
                        progressSkill,
                        payload.fileName,
                        reportedChunkCompleted,
                        reportedChunkTotal
                      );
                }

                if (
                  progressSkill &&
                  !hasFileName &&
                  (stage === "aggregate" || stage === "done" || stage === "error")
                ) {
                  nextActiveChunkFiles = removeActiveChunkFilesForSkill(
                    nextActiveChunkFiles,
                    progressSkill
                  );
                }

                const focusedFile = nextActiveChunkFiles[0]?.fileName ?? payload.fileName ?? prev.currentFile;
                return {
                  ...prev,
                  currentSkill: payload.skillName ?? prev.currentSkill,
                  currentStage: stage,
                  currentFile: focusedFile,
                  scanAngle: prev.scanAngle,
                  skillFileProgress: nextSkillFileProgress,
                  skillChunkProgress: nextSkillChunkProgress,
                  activeChunkFiles: nextActiveChunkFiles,
                  activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                  maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
                };
              });
              break;
            }

            case "skill-complete":
              setState((prev) => {
                const newResults = payload.result
                  ? [...prev.results.filter((r) => r.skill_name !== payload.result!.skill_name), payload.result]
                  : prev.results;
                const newRiskMap = { ...prev.riskMap };
                if (payload.result) {
                  newRiskMap[payload.result.skill_name] = payload.result.risk_level;
                }
                const completedName = payload.skillName || payload.result?.skill_name;
                const newActive = completedName
                  ? prev.activeSkills.filter((s) => s !== completedName)
                  : prev.activeSkills;
                const nextSkillFileProgress = removeSkillFileProgress(
                  prev.skillFileProgress,
                  completedName
                );
                const nextSkillChunkProgress = removeSkillChunkProgress(
                  prev.skillChunkProgress,
                  completedName
                );
                const nextActiveChunkFiles = removeActiveChunkFilesForSkill(
                  prev.activeChunkFiles,
                  completedName
                );
                return {
                  ...prev,
                  results: newResults,
                  riskMap: newRiskMap,
                  activeSkills: newActive,
                  currentSkill: newActive[0] || null,
                  currentStage: newActive.length > 0 ? prev.currentStage : null,
                  currentFile: nextActiveChunkFiles[0]?.fileName ?? null,
                  syncPulseKey: prev.syncPulseKey + 1,
                  scanAngle: newActive.length > 0 ? prev.scanAngle : 0,
                  scanStartedAt: prev.scanStartedAt,
                  recentFiles: pushRecentFile(
                    prev.recentFiles,
                    prev.currentFile,
                    prev.currentSkill,
                    prev.currentStage,
                    getFileRiskFromResult(payload.result, prev.currentFile),
                    getFileReasonLabels(payload.result, prev.currentFile)
                  ),
                  scanned: payload.scanned ?? prev.scanned,
                  total: payload.total ?? prev.total,
                  skillFileProgress: nextSkillFileProgress,
                  skillChunkProgress: nextSkillChunkProgress,
                  activeChunkFiles: nextActiveChunkFiles,
                  activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                  maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
                };
              });
              break;

            case "chunk-error":
              setState((prev) => ({
                ...prev,
                errors: [
                  ...prev.errors,
                  { skillName: payload.skillName || "unknown", message: payload.message || "Chunk analysis failed" },
                ],
                activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
              }));
              break;

            case "error":
              setState((prev) => {
                const newActive = payload.skillName
                  ? prev.activeSkills.filter((skill) => skill !== payload.skillName)
                  : prev.activeSkills;
                const nextActiveChunkFiles = removeActiveChunkFilesForSkill(
                  prev.activeChunkFiles,
                  payload.skillName
                );
                return {
                  ...prev,
                  activeSkills: newActive,
                  currentSkill: newActive[0] ?? null,
                  currentFile: nextActiveChunkFiles[0]?.fileName ?? prev.currentFile,
                  errors: [
                    ...prev.errors,
                    { skillName: payload.skillName || "unknown", message: payload.message || "Unknown error" },
                  ],
                  scanned: payload.scanned ?? prev.scanned,
                  total: payload.total ?? prev.total,
                  skillFileProgress: removeSkillFileProgress(prev.skillFileProgress, payload.skillName),
                  skillChunkProgress: removeSkillChunkProgress(prev.skillChunkProgress, payload.skillName),
                  activeChunkFiles: nextActiveChunkFiles,
                  activeChunkWorkers: payload.activeChunkWorkers ?? prev.activeChunkWorkers,
                  maxChunkWorkers: payload.maxChunkWorkers ?? prev.maxChunkWorkers,
                };
              });
              break;

            case "done":
              setState((prev) => ({
                ...prev,
                phase: "done",
                activeSkills: [],
                currentSkill: null,
                currentStage: null,
                currentFile: null,
                syncPulseKey: prev.syncPulseKey + 1,
                scanAngle: 0,
                scanStartedAt: null,
                recentFiles: pushRecentFile(prev.recentFiles, prev.currentFile, prev.currentSkill, prev.currentStage),
                scanned: payload.scanned ?? prev.scanned,
                total: payload.total ?? prev.total,
                skillFileProgress: {},
                skillChunkProgress: {},
                activeChunkFiles: [],
                activeChunkWorkers: 0,
                maxChunkWorkers: prev.maxChunkWorkers,
              }));
              break;
          }
        }
      );

      // Fire the command (non-blocking — results come via events)
      try {
        const exactResults = await invoke<SecurityScanResult[]>("ai_security_scan", {
          requestId,
          skillNames,
          force,
          mode,
        });
        const exactRiskMap: Record<string, RiskLevel> = {};
        for (const result of exactResults) {
          exactRiskMap[result.skill_name] = result.risk_level;
        }
        setState((prev) => ({
          ...prev,
          phase: "done",
          activeSkills: [],
          currentSkill: null,
          currentStage: null,
          currentFile: null,
          syncPulseKey: prev.syncPulseKey + 1,
          scanAngle: 0,
          scanStartedAt: null,
          recentFiles: pushRecentFile(prev.recentFiles, prev.currentFile, prev.currentSkill, prev.currentStage),
          skillFileProgress: {},
          skillChunkProgress: {},
          activeChunkFiles: [],
          activeChunkWorkers: 0,
          maxChunkWorkers: prev.maxChunkWorkers,
          results: targetSet
            ? [
                ...prev.results.filter((result) => !targetSet.has(result.skill_name)),
                ...exactResults,
              ]
            : exactResults,
          riskMap: targetSet
            ? {
                ...Object.fromEntries(
                  Object.entries(prev.riskMap).filter(([name]) => !targetSet.has(name))
                ),
                ...exactRiskMap,
              }
            : exactRiskMap,
        }));
      } catch (e) {
        setState((prev) => ({
          ...prev,
          phase: "done",
          activeSkills: [],
          currentSkill: null,
          currentStage: null,
          currentFile: null,
          syncPulseKey: prev.syncPulseKey + 1,
          scanAngle: 0,
          scanStartedAt: null,
          recentFiles: pushRecentFile(prev.recentFiles, prev.currentFile, prev.currentSkill, prev.currentStage),
          skillFileProgress: {},
          skillChunkProgress: {},
          activeChunkFiles: [],
          activeChunkWorkers: 0,
          maxChunkWorkers: prev.maxChunkWorkers,
          errors: [...prev.errors, { skillName: "", message: String(e) }],
        }));
      }
    },
    []
  );

  const resetScan = useCallback(() => {
    setState((prev) => ({
      ...prev,
      phase: "idle",
      activeSkills: [],
      currentSkill: null,
      currentStage: null,
      currentFile: null,
      syncPulseKey: prev.syncPulseKey + 1,
      scanAngle: 0,
      scanStartedAt: null,
      recentFiles: [],
      scanned: 0,
      total: 0,
      skillFileProgress: {},
      skillChunkProgress: {},
      activeChunkFiles: [],
      activeChunkWorkers: 0,
      maxChunkWorkers: 0,
      errors: [],
    }));
  }, []);

  const clearCache = useCallback(async () => {
    try {
      await invoke("clear_security_scan_cache");
      setState((prev) => ({
        ...prev,
        phase: "idle",
        activeSkills: [],
        currentSkill: null,
        currentStage: null,
        currentFile: null,
        syncPulseKey: prev.syncPulseKey + 1,
        scanAngle: 0,
        scanStartedAt: null,
        recentFiles: [],
        scanned: 0,
        total: 0,
        skillFileProgress: {},
        skillChunkProgress: {},
        activeChunkFiles: [],
        activeChunkWorkers: 0,
        maxChunkWorkers: 0,
        results: [],
        riskMap: {},
      }));
    } catch {
      // ignore
    }
  }, []);

  const cancelScan = useCallback(async () => {
    try {
      if (state.phase !== "scanning") return;
      await invoke("cancel_security_scan");
      setState((prev) => ({
        ...prev,
        phase: "done",
        activeSkills: [],
        currentSkill: null,
        currentStage: null,
        currentFile: null,
        syncPulseKey: prev.syncPulseKey + 1,
        scanAngle: 0,
        scanStartedAt: null,
        recentFiles: pushRecentFile(prev.recentFiles, prev.currentFile, prev.currentSkill, prev.currentStage),
        skillFileProgress: {},
        skillChunkProgress: {},
        activeChunkFiles: [],
        activeChunkWorkers: 0,
        maxChunkWorkers: 0,
      }));
    } catch {
      // ignore
    }
  }, [state.phase]);

  // Cleanup listener on unmount (provider level — only on app close)
  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  const value: SecurityScanContextValue = {
    ...state,
    progressPercent: calculateProgressPercent(
      state.scanned,
      state.total,
      state.skillFileProgress,
      state.skillChunkProgress
    ),
    startScan,
    resetScan,
    loadCached,
    clearCache,
    cancelScan,
  };

  return createElement(SecurityScanContext.Provider, { value }, children);
}

export function useSecurityScan(): SecurityScanContextValue {
  const ctx = useContext(SecurityScanContext);
  if (!ctx) {
    throw new Error("useSecurityScan must be used within SecurityScanProvider");
  }
  return ctx;
}
