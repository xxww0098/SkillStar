import { AnimatePresence, motion } from "framer-motion";
import { Orbit, Radar, Scan } from "lucide-react";
import { useMemo } from "react";
import { getFileTheme, getRiskTone } from "../../../lib/securityScanTheme";
import type { SecurityScanTrailItem } from "../../../types";

interface ScanFilePanelProps {
  activeSkills: string[];
  currentSkill: string | null;
  fileName: string | null;
  activeChunkFiles: {
    skillName: string;
    fileName: string;
    chunkCompleted?: number;
    chunkTotal?: number;
  }[];
  stage: string | null;
  syncPulseKey: number;
  recentFiles: SecurityScanTrailItem[];
  progressPercent: number;
  activeChunkWorkers: number;
  maxChunkWorkers: number;
}

function getStageLabel(stage: string | null): string {
  switch (stage) {
    case "collect":
      return "Preparing Workspace";
    case "static":
      return "Static Pattern Match";
    case "triage":
      return "AI Triage";
    case "ai":
    case "ai-analyze":
      return "AI Analysis";
    case "aggregator":
    case "aggregate":
      return "AI Consensus";
    default:
      return "Scanning";
  }
}

function getStagePrefix(stage: string | null): string {
  switch (stage) {
    case "collect":
      return "PREP";
    case "static":
      return "STATIC";
    case "triage":
      return "TRIAGE";
    case "ai":
    case "ai-analyze":
      return "AI ANALYSIS";
    case "aggregator":
    case "aggregate":
      return "CONSENSUS";
    default:
      return "SCAN";
  }
}

function getChunkLabel(
  focusChunkCompleted: number | null,
  focusChunkTotal: number | null,
  activeChunkCount: number,
): string {
  if (focusChunkTotal && focusChunkTotal > 0 && focusChunkCompleted !== null) {
    if (focusChunkTotal === 1) {
      return "single chunk";
    }

    const activeChunkIndex = Math.min(focusChunkCompleted + 1, focusChunkTotal);
    return `chunk ${activeChunkIndex}/${focusChunkTotal}`;
  }

  if (activeChunkCount <= 0) {
    return "waiting for chunks";
  }

  return `${activeChunkCount} active chunk${activeChunkCount === 1 ? "" : "s"}`;
}

export function ScanFilePanel({
  activeSkills,
  currentSkill,
  fileName,
  activeChunkFiles,
  stage,
  recentFiles,
  progressPercent,
  activeChunkWorkers,
  maxChunkWorkers,
}: ScanFilePanelProps) {
  const focusChunk = activeChunkFiles[0] ?? null;
  const focusFileName = focusChunk?.fileName ?? fileName;
  const focusChunkCompleted = typeof focusChunk?.chunkCompleted === "number" ? focusChunk.chunkCompleted : null;
  const focusChunkTotal = typeof focusChunk?.chunkTotal === "number" ? focusChunk.chunkTotal : null;
  const chunkLabel = getChunkLabel(focusChunkCompleted, focusChunkTotal, activeChunkFiles.length);
  const progress = progressPercent;
  const theme = useMemo(() => getFileTheme(focusFileName), [focusFileName]);
  const Icon = theme.icon;

  return (
    <motion.div
      initial={{ opacity: 0, x: 20, scale: 0.95 }}
      animate={{ opacity: 1, x: 0, scale: 1 }}
      exit={{ opacity: 0, x: 20, scale: 0.95 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      className="relative w-[min(24rem,calc(100vw-2rem))] min-h-[300px] rounded-2xl border border-border bg-card/95 backdrop-blur-md shadow-xl flex flex-col"
    >
      <div className="absolute inset-0 bg-gradient-to-br from-success/5 via-transparent to-transparent opacity-50 pointer-events-none" />

      {/* Header */}
      <div className="relative z-10 flex items-center justify-between p-4 border-b border-border/50 bg-muted/20">
        <div className="flex items-center gap-2">
          <div className="flex items-center justify-center w-6 h-6 rounded border border-success/30 bg-success/10 text-success">
            <Scan className="w-3.5 h-3.5" />
          </div>
          <span className="text-[10px] font-medium tracking-widest uppercase text-muted-foreground">Security Scan</span>
        </div>
        <motion.div
          key={stage}
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          className="text-[9px] px-2 py-0.5 rounded-full bg-success/15 text-success/90 border border-success/30 tracking-widest uppercase"
        >
          {getStagePrefix(stage)}
        </motion.div>
      </div>

      <div className="relative z-10 p-5 flex flex-col gap-5 flex-1">
        {/* Skills being scanned + Current file */}
        <AnimatePresence mode="wait">
          {focusFileName ? (
            <motion.div
              key={focusFileName}
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              className="flex flex-col gap-4"
            >
              {/* Skill chips */}
              <div className="flex items-center gap-3">
                <div
                  className={`flex items-center justify-center w-8 h-8 rounded-lg border bg-background shadow-sm ${theme.chip}`}
                >
                  <Icon className={`w-4 h-4 ${theme.tintText}`} />
                </div>
                <div className="flex flex-col">
                  <span className="text-[10px] font-medium tracking-widest uppercase text-muted-foreground mb-0.5">
                    Scanning
                  </span>
                  <div className="flex flex-wrap items-center gap-1.5 max-w-[260px] max-h-[4.5rem] overflow-y-auto pr-0.5">
                    {activeSkills.length > 0 ? (
                      activeSkills.map((skill) => {
                        const isCurrent = skill === currentSkill;
                        return (
                          <span
                            key={skill}
                            className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] ${
                              isCurrent
                                ? "border-success/30 bg-success/12 text-foreground"
                                : "border-border/60 bg-background/70 text-muted-foreground"
                            }`}
                          >
                            {isCurrent ? (
                              <span className="relative flex h-1.5 w-1.5">
                                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75" />
                                <span className="relative inline-flex rounded-full h-1.5 w-1.5 bg-success" />
                              </span>
                            ) : (
                              <span className="h-1.5 w-1.5 rounded-full bg-success/60" />
                            )}
                            {skill}
                          </span>
                        );
                      })
                    ) : (
                      <span className="text-xs font-semibold text-foreground">Scanning...</span>
                    )}
                  </div>
                </div>
              </div>

              {/* Current file name */}
              <div className="font-mono text-lg font-bold text-foreground break-all leading-tight">
                {focusFileName.split("/").pop() || focusFileName}
                <motion.span
                  animate={{ opacity: [0.2, 1, 0.2] }}
                  transition={{ duration: 0.9, repeat: Infinity, ease: "linear" }}
                  className={`ml-1 ${theme.tintText}`}
                >
                  |
                </motion.span>
              </div>

              {/* Files being analyzed — simple list without worker/chunk detail */}
              {activeChunkFiles.length > 0 && (
                <div className="space-y-1.5">
                  <div className="text-[10px] uppercase tracking-wider text-muted-foreground">Analyzing</div>
                  <div className="max-h-[7rem] space-y-1 overflow-y-auto pr-1">
                    {activeChunkFiles.map((chunk) => (
                      <div
                        key={`${chunk.skillName}:${chunk.fileName}`}
                        className="flex min-w-0 items-center gap-1.5 rounded-md border border-border/50 bg-muted/35 px-2 py-1"
                      >
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-success" />
                        <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-foreground/90">
                          {chunk.fileName.split("/").pop() || chunk.fileName}
                        </span>
                        <span className="shrink-0 rounded bg-background/60 px-1 py-0.5 text-[9px] text-muted-foreground/70">
                          {chunk.skillName}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </motion.div>
          ) : (
            <motion.div
              key="waiting"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="py-6 text-sm text-muted-foreground flex items-center justify-center min-h-[120px]"
            >
              Preparing file queue...
            </motion.div>
          )}
        </AnimatePresence>

        {/* Progress */}
        <div className="flex flex-col gap-2 mt-auto">
          <div className="flex items-center justify-between text-xs">
            <div className="flex items-center gap-1.5 font-medium text-foreground">
              <Radar className="w-3.5 h-3.5 text-success" />
              {getStageLabel(stage)}
            </div>
            <span className="text-success font-bold tabular-nums">{progress}%</span>
          </div>

          <div className="flex items-center justify-between text-[10px] text-muted-foreground">
            <span className="tabular-nums">
              {activeChunkWorkers}/{Math.max(maxChunkWorkers, 1)} workers
            </span>
            <span className="tabular-nums">{chunkLabel}</span>
          </div>

          <div className="relative h-1.5 rounded-full border border-border/50 bg-muted overflow-hidden">
            <motion.div
              className="absolute inset-0 rounded-full bg-success shadow-[0_0_8px_rgba(var(--color-success-rgb),0.5)]"
              initial={{ scaleX: 0 }}
              animate={{ scaleX: progress / 100 }}
              transition={{ duration: 0.4, ease: "easeOut" }}
              style={{ transformOrigin: "left center" }}
            />
          </div>
        </div>

        {/* Recent Results */}
        {recentFiles.length > 0 ? (
          <div className="flex flex-col gap-3 pt-4 border-t border-border/50 mt-1">
            <div className="flex items-center gap-2 text-[10px] font-medium tracking-widest uppercase text-muted-foreground">
              <Orbit className="w-3.5 h-3.5 text-success/80" />
              Recent Results
            </div>
            <div className="space-y-2">
              <AnimatePresence mode="popLayout">
                {recentFiles.map((item, index) => {
                  const riskTone = getRiskTone(item.riskLevel);
                  return (
                    <motion.div
                      key={`${item.fileName}-${item.timestamp}`}
                      initial={{ opacity: 0, y: -6, scale: 0.98 }}
                      animate={{ opacity: 1 - index * 0.15, y: 0, scale: 1 }}
                      exit={{ opacity: 0, y: 8, scale: 0.98 }}
                      transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
                      className={`flex items-start gap-2 rounded-lg border border-border/50 bg-muted/40 px-2 py-1.5 ${riskTone.glow}`}
                    >
                      <span className={`mt-1 h-1.5 w-1.5 rounded-full shrink-0 ${riskTone.dot}`} />
                      <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center justify-between gap-2">
                          <div className="flex items-center min-w-0 flex-shrink gap-1.5">
                            <span className="truncate text-xs font-semibold text-foreground">
                              {item.fileName.split("/").pop()}
                            </span>
                            <span className="truncate text-[10px] text-muted-foreground">
                              {item.fileName.split("/").slice(0, -1).pop()}
                            </span>
                          </div>
                          <span
                            className={`shrink-0 rounded bg-background px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider border ${riskTone.text} ${riskTone.pill}`}
                          >
                            {item.riskLevel ?? "Safe"}
                          </span>
                        </div>
                        {item.reasonLabels && item.reasonLabels.length > 0 && (
                          <div className="mt-1 flex flex-wrap gap-1">
                            {item.reasonLabels.map((label) => (
                              <span
                                key={label}
                                className="rounded border border-border bg-muted/50 px-1.5 py-0.5 text-[8px] uppercase tracking-wider text-muted-foreground"
                              >
                                {label}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </div>
          </div>
        ) : null}
      </div>
    </motion.div>
  );
}
